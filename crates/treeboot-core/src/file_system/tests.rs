use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use super::*;
use crate::test_support::symlink_dir;
use crate::{ActionPlan, FileOperationKind, PlanOrigin, Worktree};

/// A [`Read`] adapter that hands back bytes in a scripted sequence of short
/// reads, simulating filesystems that return fewer bytes than requested even
/// when more data remains. Once the `chunks` script is exhausted it fills
/// whatever the caller's buffer allows.
struct ChunkedRead {
    data: Vec<u8>,
    chunks: Vec<usize>,
    pos: usize,
    chunk_index: usize,
}

impl ChunkedRead {
    fn new(data: Vec<u8>, chunks: Vec<usize>) -> Self {
        Self {
            data,
            chunks,
            pos: 0,
            chunk_index: 0,
        }
    }
}

impl Read for ChunkedRead {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.pos == self.data.len() {
            return Ok(0);
        }

        let chunk = self
            .chunks
            .get(self.chunk_index)
            .copied()
            .unwrap_or(buffer.len())
            .max(1);
        self.chunk_index += 1;

        let read = chunk.min(buffer.len()).min(self.data.len() - self.pos);
        buffer[..read].copy_from_slice(&self.data[self.pos..self.pos + read]);
        self.pos += read;
        Ok(read)
    }
}

/// A [`Read`] adapter that returns [`io::ErrorKind::Interrupted`] a fixed number
/// of times before yielding its data, used to confirm interrupted reads are
/// retried rather than surfaced as failures.
struct InterruptingReader {
    data: Vec<u8>,
    pos: usize,
    pending_interrupts: usize,
}

impl InterruptingReader {
    fn new(data: Vec<u8>, pending_interrupts: usize) -> Self {
        Self {
            data,
            pos: 0,
            pending_interrupts,
        }
    }
}

impl Read for InterruptingReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.pending_interrupts > 0 {
            self.pending_interrupts -= 1;
            return Err(io::Error::from(io::ErrorKind::Interrupted));
        }

        let read = buffer.len().min(self.data.len() - self.pos);
        buffer[..read].copy_from_slice(&self.data[self.pos..self.pos + read]);
        self.pos += read;
        Ok(read)
    }
}

/// A [`Read`] adapter that always fails, used to check error attribution.
struct FailingReader;

impl Read for FailingReader {
    fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::other("read failed"))
    }
}

fn temp_workspace(name: &str) -> (PathBuf, PathBuf) {
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after Unix epoch")
        .as_nanos();
    let base = std::env::temp_dir().join(format!("treeboot-file-system-{name}-{id}"));
    let root = base.join("root");
    let worktree = base.join("worktree");

    fs::create_dir_all(&root).expect("root should be created");
    fs::create_dir_all(&worktree).expect("worktree should be created");

    (root, worktree)
}

fn context(root_path: &Path, worktree_path: &Path) -> Worktree {
    Worktree::from_parts(
        root_path.to_path_buf(),
        worktree_path.to_path_buf(),
        "main".to_owned(),
        BTreeMap::from([("TREEBOOT_ROOT_PATH".to_owned(), OsString::from(root_path))]),
    )
}

fn empty_plan(root: &Path, worktree: &Path) -> ActionPlan {
    ActionPlan::from_parts_unchecked(
        context(root, worktree),
        PlanOrigin::Manifest {
            path: worktree.join(".treeboot.toml"),
        },
        Some(worktree.join(".treeboot.toml")),
        Vec::new(),
        Vec::new(),
    )
}

#[test]
fn reader_contents_changed_should_ignore_short_read_boundaries() {
    // Span more than one 8 KiB read buffer with mismatched short-read scripts so
    // the two handles desync per call; identical content must still compare
    // unchanged. Fails if the comparison loop reads with raw `Read::read`.
    let data: Vec<u8> = (0..(8192 + 137)).map(|i| (i % 251) as u8).collect();
    let mut source = ChunkedRead::new(data.clone(), vec![1, 8, 3, 2, 13]);
    let mut target = ChunkedRead::new(data, vec![5, 1, 1, 16, 4]);

    let changed = reader_contents_changed(&mut source, &mut target)
        .expect("identical readers should compare cleanly");

    assert!(!changed);
}

#[test]
fn reader_contents_changed_should_detect_equal_size_differences() {
    let source_data: Vec<u8> = (0..4096).map(|i| (i % 251) as u8).collect();
    let mut target_data = source_data.clone();
    *target_data.last_mut().expect("data is non-empty") ^= 0xFF;
    let mut source = ChunkedRead::new(source_data, vec![1, 8, 3]);
    let mut target = ChunkedRead::new(target_data, vec![5, 1, 1]);

    let changed = reader_contents_changed(&mut source, &mut target)
        .expect("equal-length readers should compare cleanly");

    assert!(changed);
}

#[test]
fn read_full_chunk_should_fill_buffer_across_short_reads() {
    let data: Vec<u8> = (0..100).map(|i| i as u8).collect();
    let mut reader = ChunkedRead::new(data.clone(), vec![3, 7, 11]);
    let mut buffer = [0u8; 100];

    let read = read_full_chunk(&mut reader, &mut buffer, ContentInput::Source)
        .expect("read_full_chunk should fill the buffer");

    assert_eq!(read, 100);
    assert_eq!(buffer, data.as_slice());
}

#[test]
fn read_full_chunk_should_return_short_count_at_eof() {
    let data: Vec<u8> = (0..10).map(|i| i as u8).collect();
    let mut reader = ChunkedRead::new(data, vec![3]);
    let mut buffer = [0u8; 64];

    let read = read_full_chunk(&mut reader, &mut buffer, ContentInput::Source)
        .expect("read_full_chunk should stop at EOF");

    assert_eq!(read, 10);
}

#[test]
fn read_full_chunk_should_retry_interrupted_reads() {
    let data: Vec<u8> = (0..32).map(|i| i as u8).collect();
    let mut reader = InterruptingReader::new(data.clone(), 2);
    let mut buffer = [0u8; 32];

    let read = read_full_chunk(&mut reader, &mut buffer, ContentInput::Source)
        .expect("interrupted reads should be retried");

    assert_eq!(read, 32);
    assert_eq!(buffer, data.as_slice());
}

#[test]
fn read_full_chunk_should_tag_errors_with_input_side() {
    let mut reader = FailingReader;
    let mut buffer = [0u8; 8];

    let error = read_full_chunk(&mut reader, &mut buffer, ContentInput::Target)
        .expect_err("hard read error should propagate");

    assert_eq!(error.input, ContentInput::Target);
}

#[test]
fn inspect_target_ancestors_should_accept_existing_directory_chain() {
    let (_root, worktree) = temp_workspace("inspect-existing-dirs");
    let parent = worktree.join("nested/config");
    fs::create_dir_all(&parent).expect("target parent should be created");

    inspect_target_ancestors(&parent, &worktree, true)
        .expect("existing directory ancestors should be accepted");
}

#[test]
fn inspect_target_ancestors_should_accept_missing_parent_when_not_required() {
    let (_root, worktree) = temp_workspace("inspect-missing-optional");
    let parent = worktree.join("nested/config");

    inspect_target_ancestors(&parent, &worktree, false)
        .expect("missing directory ancestors should be accepted before create");
}

#[test]
fn inspect_target_ancestors_should_reject_missing_parent_when_required() {
    let (_root, worktree) = temp_workspace("inspect-missing-required");
    let parent = worktree.join("nested/config");

    let issue = inspect_target_ancestors(&parent, &worktree, true)
        .expect_err("missing required ancestor should be rejected");

    match issue {
        TargetAncestorIssue::Io { path, source } => {
            assert_eq!(path, worktree.join("nested"));
            assert_eq!(source.kind(), io::ErrorKind::NotFound);
        }
        other => panic!("expected missing ancestor I/O issue, got {other:?}"),
    }
}

#[test]
fn inspect_target_ancestors_should_reject_symlink_parent() {
    let (_root, worktree) = temp_workspace("inspect-symlink-parent");
    let outside = worktree
        .parent()
        .expect("worktree should have parent")
        .join("outside-inspect");
    fs::create_dir_all(&outside).expect("outside dir should be created");
    symlink_dir(&outside, worktree.join("linked"))
        .expect("target parent symlink should be created");

    let issue = inspect_target_ancestors(&worktree.join("linked/config"), &worktree, false)
        .expect_err("symlink ancestor should be rejected");

    match issue {
        TargetAncestorIssue::Symlink { path } => assert_eq!(path, worktree.join("linked")),
        other => panic!("expected symlink ancestor issue, got {other:?}"),
    }
}

#[test]
fn inspect_target_ancestors_should_reject_file_parent() {
    let (_root, worktree) = temp_workspace("inspect-file-parent");
    fs::write(worktree.join("config"), "not a directory\n").expect("file parent should be written");

    let issue = inspect_target_ancestors(&worktree.join("config/master.key"), &worktree, false)
        .expect_err("file ancestor should be rejected");

    match issue {
        TargetAncestorIssue::NotDirectory { path } => assert_eq!(path, worktree.join("config")),
        other => panic!("expected non-directory ancestor issue, got {other:?}"),
    }
}

#[test]
fn inspect_target_ancestors_should_reject_outside_worktree_path() {
    let (_root, worktree) = temp_workspace("inspect-outside-worktree");
    let outside = worktree
        .parent()
        .expect("worktree should have parent")
        .join("outside-target");

    let issue = inspect_target_ancestors(&outside, &worktree, false)
        .expect_err("outside path should be rejected");

    match issue {
        TargetAncestorIssue::OutsideWorktree { path } => assert_eq!(path, outside),
        other => panic!("expected outside-worktree issue, got {other:?}"),
    }
}

#[test]
fn inspect_target_ancestors_should_reject_worktree_root_file() {
    let (root, _worktree) = temp_workspace("inspect-root-file");
    let file_worktree = root.join("file-worktree");
    fs::write(&file_worktree, "not a directory\n").expect("file worktree should be written");

    let issue = inspect_target_ancestors(&file_worktree, &file_worktree, true)
        .expect_err("file worktree root should be rejected");

    match issue {
        TargetAncestorIssue::NotDirectory { path } => assert_eq!(path, file_worktree),
        other => panic!("expected non-directory worktree root issue, got {other:?}"),
    }
}

#[test]
fn remove_any_should_reject_symlink_target_parent_before_delete() {
    let (_root, worktree) = temp_workspace("delete-symlink-parent");
    let outside = worktree
        .parent()
        .expect("worktree should have parent")
        .join("outside-delete");
    fs::create_dir_all(&outside).expect("outside dir should be created");
    fs::write(outside.join("extra"), "keep\n").expect("outside file should be written");
    symlink_dir(&outside, worktree.join("linked"))
        .expect("target parent symlink should be created");

    let error = remove_any(
        FileOperationKind::Sync,
        &worktree.join("linked/extra"),
        &worktree,
    )
    .expect_err("delete through symlink parent should fail");

    assert!(error.to_string().contains("target parent is a symlink"));
    assert_eq!(
        fs::read_to_string(outside.join("extra")).expect("outside file should remain readable"),
        "keep\n"
    );
}

#[cfg(unix)]
#[test]
fn with_writable_parent_should_restore_parent_permissions_when_action_fails() {
    let (_root, worktree) = temp_workspace("restore-parent-on-error");
    let parent = worktree.join("shared");
    fs::create_dir_all(&parent).expect("parent dir should be created");
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o555))
        .expect("parent should become read-only");

    let error = with_writable_parent(
        FileOperationKind::Copy,
        &parent.join("target"),
        &worktree,
        || {
            Err(Error::FileOperationConflict {
                operation: "copy",
                path: parent.join("target"),
                message: "simulated failure".to_owned(),
            })
        },
    )
    .expect_err("action error should propagate");

    let mode = fs::metadata(&parent)
        .expect("parent metadata should be readable")
        .permissions()
        .mode()
        & 0o777;
    assert!(error.to_string().contains("simulated failure"));
    assert_eq!(mode, 0o555);
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o755))
        .expect("parent permissions should be restored for cleanup");
}

#[test]
fn preserved_source_link_should_track_directory_target_type() {
    let (root, worktree) = temp_workspace("preserved-directory-symlink");
    let source_dir = root.join("shared");
    fs::create_dir_all(source_dir.join("dir")).expect("source dir should be created");
    symlink_dir("dir", source_dir.join("link")).expect("source symlink should be created");
    let plan = empty_plan(&root, &worktree);

    let (_, final_target, target_is_dir) = preserved_source_link(
        &plan,
        FileOperationKind::Copy,
        &source_dir.join("link"),
        &worktree.join("shared/link"),
    )
    .expect("preserved symlink should plan");

    assert_eq!(final_target, worktree.join("shared/dir"));
    assert!(target_is_dir);
}

#[test]
fn preserved_source_link_should_normalize_absolute_root_local_targets() {
    let (root, worktree) = temp_workspace("preserved-absolute-normalized-symlink");
    let source_dir = root.join("shared");
    fs::create_dir_all(source_dir.join("dir")).expect("source dir should be created");
    symlink_dir(root.join("shared/../shared/dir"), source_dir.join("link"))
        .expect("source symlink should be created");
    let plan = empty_plan(&root, &worktree);

    let (link_target, final_target, target_is_dir) = preserved_source_link(
        &plan,
        FileOperationKind::Copy,
        &source_dir.join("link"),
        &worktree.join("shared/link"),
    )
    .expect("preserved symlink should plan");

    assert_eq!(link_target, PathBuf::from("dir"));
    assert_eq!(final_target, worktree.join("shared/dir"));
    assert!(target_is_dir);
}
