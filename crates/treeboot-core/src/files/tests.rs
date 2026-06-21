use std::collections::BTreeMap;
use std::ffi::OsString;
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;
use crate::{PlanOrigin, SourceSpan, Worktree};

#[derive(Default)]
struct VecReporter {
    events: Vec<OutputEvent>,
}

impl Reporter for VecReporter {
    fn report(&mut self, event: OutputEvent) -> std::io::Result<()> {
        self.events.push(event);
        Ok(())
    }
}

fn span() -> SourceSpan {
    SourceSpan {
        start: 0,
        end: 1,
        line: 1,
        column: 1,
    }
}

fn temp_workspace(name: &str) -> (PathBuf, PathBuf) {
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after Unix epoch")
        .as_nanos();
    let base = std::env::temp_dir().join(format!("treeboot-files-{name}-{id}"));
    let root = base.join("root");
    let worktree = base.join("worktree");

    fs::create_dir_all(&root).expect("root should be created");
    fs::create_dir_all(&worktree).expect("worktree should be created");

    (root, worktree)
}

fn context(root_path: &Path, worktree_path: &Path) -> Worktree {
    Worktree {
        root_path: root_path.to_path_buf(),
        worktree_path: worktree_path.to_path_buf(),
        default_branch: "main".to_owned(),
        environment: BTreeMap::from([("TREEBOOT_ROOT_PATH".to_owned(), OsString::from(root_path))]),
    }
}

fn operation(
    operation: FileOperationKind,
    root: &Path,
    worktree: &Path,
    source: &str,
    target: &str,
) -> PlannedFileOperation {
    PlannedFileOperation {
        operation,
        source: PathBuf::from(source),
        target: PathBuf::from(target),
        source_path: root.join(source),
        target_path: worktree.join(target),
        required: false,
        compare: None,
        delete: None,
        symlinks: None,
        status: PlannedFileStatus::Ready,
        declaration: span(),
    }
}

fn sync_operation(
    root: &Path,
    worktree: &Path,
    source: &str,
    target: &str,
) -> PlannedFileOperation {
    operation(FileOperationKind::Sync, root, worktree, source, target)
}

fn run_plan(root: &Path, worktree: &Path, files: Vec<PlannedFileOperation>) -> ActionPlan {
    ActionPlan {
        context: context(root, worktree),
        origin: PlanOrigin::Manifest {
            path: worktree.join(".treeboot.toml"),
        },
        config_path: Some(worktree.join(".treeboot.toml")),
        files,
        commands: Vec::new(),
    }
}

#[test]
fn apply_file_operations_should_copy_missing_directory_tree() {
    let (root, worktree) = temp_workspace("missing-directory-copy");
    let source_dir = root.join("shared/nested");
    fs::create_dir_all(&source_dir).expect("source dir should be created");
    fs::write(source_dir.join("config"), "value\n").expect("source should be written");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            "shared",
            "shared",
        )],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("directory tree should copy");

    let copied = fs::read_to_string(worktree.join("shared/nested/config"))
        .expect("nested target should be readable");
    assert_eq!(copied, "value\n");
}

#[test]
fn apply_file_operations_should_copy_missing_directory_files_only() {
    let (root, worktree) = temp_workspace("directory-copy");
    let source_dir = root.join("shared");
    let target_dir = worktree.join("shared");
    fs::create_dir_all(&source_dir).expect("source dir should be created");
    fs::create_dir_all(&target_dir).expect("target dir should be created");
    fs::write(source_dir.join("existing"), "new\n").expect("source should be written");
    fs::write(source_dir.join("missing"), "value\n").expect("source should be written");
    fs::write(target_dir.join("existing"), "old\n").expect("target should be written");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            "shared",
            "shared",
        )],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("directory copy should apply");

    let existing = fs::read_to_string(target_dir.join("existing"))
        .expect("existing target should be readable");
    let missing =
        fs::read_to_string(target_dir.join("missing")).expect("missing target should be copied");
    assert_eq!(existing, "old\n");
    assert_eq!(missing, "value\n");
}

#[test]
fn apply_file_operations_should_force_copy_directory_files_without_deleting_extras() {
    let (root, worktree) = temp_workspace("force-directory-copy");
    let source_dir = root.join("shared");
    let target_dir = worktree.join("shared");
    fs::create_dir_all(&source_dir).expect("source dir should be created");
    fs::create_dir_all(&target_dir).expect("target dir should be created");
    fs::write(source_dir.join("existing"), "new\n").expect("source should be written");
    fs::write(target_dir.join("existing"), "old\n").expect("target should be written");
    fs::write(target_dir.join("extra"), "keep\n").expect("extra target should be written");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            "shared",
            "shared",
        )],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(
        &plan,
        FileApplyOptions {
            force: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect("forced directory copy should apply");

    let existing = fs::read_to_string(target_dir.join("existing"))
        .expect("existing target should be readable");
    let extra = fs::read_to_string(target_dir.join("extra")).expect("extra target should remain");
    assert_eq!(existing, "new\n");
    assert_eq!(extra, "keep\n");
}

#[test]
fn apply_file_operations_should_reject_existing_directory_in_strict_before_children() {
    let (root, worktree) = temp_workspace("strict-directory-copy");
    let source_dir = root.join("shared");
    let target_dir = worktree.join("shared");
    fs::create_dir_all(&source_dir).expect("source dir should be created");
    fs::create_dir_all(&target_dir).expect("target dir should be created");
    fs::write(source_dir.join("missing"), "value\n").expect("source should be written");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            "shared",
            "shared",
        )],
    );
    let mut reporter = VecReporter::default();

    let error = apply_file_operations(
        &plan,
        FileApplyOptions {
            strict: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect_err("strict directory conflict should fail");

    assert!(error.to_string().contains("target directory exists"));
    assert!(!target_dir.join("missing").exists());
}

#[test]
fn apply_file_operations_should_reject_file_to_directory_target() {
    let (root, worktree) = temp_workspace("file-to-directory");
    fs::write(root.join(".env"), "TOKEN=1\n").expect("source should be written");
    fs::create_dir_all(worktree.join(".env")).expect("target dir should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            ".env",
            ".env",
        )],
    );
    let mut reporter = VecReporter::default();

    let error = apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect_err("file to directory should fail");

    assert!(error.to_string().contains("target is a directory"));
}

#[test]
fn apply_file_operations_should_reject_file_to_directory_target_even_with_force() {
    let (root, worktree) = temp_workspace("force-file-to-directory");
    fs::write(root.join(".env"), "TOKEN=1\n").expect("source should be written");
    fs::create_dir_all(worktree.join(".env")).expect("target dir should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            ".env",
            ".env",
        )],
    );
    let mut reporter = VecReporter::default();

    let error = apply_file_operations(
        &plan,
        FileApplyOptions {
            force: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect_err("force should not replace directory with file");

    assert!(error.to_string().contains("target is a directory"));
    assert!(worktree.join(".env").is_dir());
}

#[test]
fn apply_file_operations_should_reject_file_to_directory_target_in_dry_run() {
    let (root, worktree) = temp_workspace("dry-run-file-to-directory");
    fs::write(root.join(".env"), "TOKEN=1\n").expect("source should be written");
    fs::create_dir_all(worktree.join(".env")).expect("target dir should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            ".env",
            ".env",
        )],
    );
    let mut reporter = VecReporter::default();

    let error = apply_file_operations(
        &plan,
        FileApplyOptions {
            dry_run: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect_err("dry-run should report file-to-directory conflict");

    assert!(error.to_string().contains("target is a directory"));
    assert!(reporter.events.is_empty());
}

#[test]
fn apply_file_operations_should_reject_directory_to_file_target() {
    let (root, worktree) = temp_workspace("directory-to-file");
    fs::create_dir_all(root.join("shared")).expect("source dir should be created");
    fs::write(worktree.join("shared"), "old\n").expect("target file should be written");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            "shared",
            "shared",
        )],
    );
    let mut reporter = VecReporter::default();

    let error = apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect_err("directory to file should fail");

    assert!(error.to_string().contains("target is a file or symlink"));
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_reject_directory_to_symlink_target() {
    let (root, worktree) = temp_workspace("directory-to-symlink");
    fs::create_dir_all(root.join("shared")).expect("source dir should be created");
    fs::write(worktree.join("real-target"), "old\n").expect("target file should be written");
    std::os::unix::fs::symlink("real-target", worktree.join("shared"))
        .expect("target symlink should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            "shared",
            "shared",
        )],
    );
    let mut reporter = VecReporter::default();

    let error = apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect_err("directory to symlink should fail");

    assert!(error.to_string().contains("target is a file or symlink"));
    assert_eq!(
        fs::read_link(worktree.join("shared")).expect("target should remain a symlink"),
        PathBuf::from("real-target")
    );
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_reject_unsupported_source_file_type() {
    use std::os::unix::net::UnixListener;

    let (root, worktree) = temp_workspace("unsupported-source");
    let socket_path = root.join("socket");
    let _listener = UnixListener::bind(&socket_path).expect("source socket should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            "socket",
            "socket",
        )],
    );
    let mut reporter = VecReporter::default();

    let error = apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect_err("unsupported source should fail");

    assert!(
        error
            .to_string()
            .contains("source file type is unsupported")
    );
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_reject_unsupported_directory_target_file_type() {
    use std::os::unix::net::UnixListener;

    let (root, worktree) = temp_workspace("unsupported-directory-target");
    fs::create_dir_all(root.join("shared")).expect("source dir should be created");
    let socket_path = worktree.join("shared");
    let _listener = UnixListener::bind(&socket_path).expect("target socket should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            "shared",
            "shared",
        )],
    );
    let mut reporter = VecReporter::default();

    let error = apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect_err("unsupported target should fail");

    assert!(
        error
            .to_string()
            .contains("target file type is unsupported")
    );
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_reject_unsupported_sync_file_target_type() {
    use std::os::unix::net::UnixListener;

    let (root, worktree) = temp_workspace("unsupported-sync-target");
    fs::write(root.join(".env"), "TOKEN=1\n").expect("source should be written");
    let socket_path = worktree.join(".env");
    let _listener = UnixListener::bind(&socket_path).expect("target socket should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(&root, &worktree, ".env", ".env")],
    );
    let mut reporter = VecReporter::default();

    let error = apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect_err("unsupported sync target should fail");

    assert!(
        error
            .to_string()
            .contains("target file type is unsupported")
    );
}

#[test]
fn apply_file_operations_should_skip_optional_missing_sources() {
    let (root, worktree) = temp_workspace("missing-source");
    let mut missing = operation(FileOperationKind::Copy, &root, &worktree, ".env", ".env");
    missing.status = PlannedFileStatus::SkippedMissingSource;
    let plan = run_plan(&root, &worktree, vec![missing]);
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("missing optional source should skip");

    assert!(matches!(
        reporter.events.as_slice(),
        [OutputEvent::FileSkipped { reason, .. }] if reason == "missing source"
    ));
}

#[test]
fn apply_file_operations_should_skip_optional_missing_sources_with_strict_or_force() {
    for options in [
        FileApplyOptions {
            strict: true,
            ..FileApplyOptions::default()
        },
        FileApplyOptions {
            force: true,
            ..FileApplyOptions::default()
        },
    ] {
        let (root, worktree) = temp_workspace("missing-source-mode");
        let mut missing = operation(FileOperationKind::Copy, &root, &worktree, ".env", ".env");
        missing.status = PlannedFileStatus::SkippedMissingSource;
        let plan = run_plan(&root, &worktree, vec![missing]);
        let mut reporter = VecReporter::default();

        apply_file_operations(&plan, options, &mut reporter)
            .expect("missing optional source should skip");

        assert!(matches!(
            reporter.events.as_slice(),
            [OutputEvent::FileSkipped { reason, .. }] if reason == "missing source"
        ));
    }
}

#[test]
fn apply_file_operations_should_report_optional_missing_source_in_dry_run() {
    let (root, worktree) = temp_workspace("missing-source-dry-run");
    let mut missing = operation(FileOperationKind::Copy, &root, &worktree, ".env", ".env");
    missing.status = PlannedFileStatus::SkippedMissingSource;
    let plan = run_plan(&root, &worktree, vec![missing]);
    let mut reporter = VecReporter::default();

    apply_file_operations(
        &plan,
        FileApplyOptions {
            dry_run: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect("dry-run missing optional source should skip");

    assert!(matches!(
        reporter.events.as_slice(),
        [OutputEvent::FileWouldSkip { reason, .. }] if reason == "missing source"
    ));
}

#[test]
fn apply_file_operations_should_sync_missing_file() {
    let (root, worktree) = temp_workspace("sync-missing-file");
    fs::write(root.join(".env"), "TOKEN=1\n").expect("source should be written");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(&root, &worktree, ".env", ".env")],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("sync should create missing file");

    let synced = fs::read_to_string(worktree.join(".env")).expect("target should be readable");
    assert_eq!(synced, "TOKEN=1\n");
    assert!(matches!(
        reporter.events.as_slice(),
        [OutputEvent::FileApplied {
            operation: FileOperationKind::Sync,
            ..
        }]
    ));
}

#[test]
fn apply_file_operations_should_leave_unchanged_metadata_sync_silent() {
    let (root, worktree) = temp_workspace("sync-unchanged");
    let source = root.join(".env");
    let target = worktree.join(".env");
    fs::write(&source, "TOKEN=1\n").expect("source should be written");
    copy_file_with_metadata(FileOperationKind::Sync, &source, &target)
        .expect("target should be seeded");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(&root, &worktree, ".env", ".env")],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("unchanged sync should succeed");

    assert!(reporter.events.is_empty());
}

#[test]
fn apply_file_operations_should_leave_unchanged_sync_silent_in_dry_run() {
    let (root, worktree) = temp_workspace("sync-unchanged-dry-run");
    let source = root.join(".env");
    let target = worktree.join(".env");
    fs::write(&source, "TOKEN=1\n").expect("source should be written");
    copy_file_with_metadata(FileOperationKind::Sync, &source, &target)
        .expect("target should be seeded");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(&root, &worktree, ".env", ".env")],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(
        &plan,
        FileApplyOptions {
            dry_run: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect("unchanged dry-run sync should succeed");

    assert!(reporter.events.is_empty());
}

#[test]
fn apply_file_operations_should_update_changed_metadata_sync_file() {
    let (root, worktree) = temp_workspace("sync-metadata-update");
    fs::write(root.join(".env"), "new\n").expect("source should be written");
    fs::write(worktree.join(".env"), "old\n").expect("target should be written");
    let times = FileTimes::new().set_modified(UNIX_EPOCH);
    File::options()
        .write(true)
        .open(worktree.join(".env"))
        .and_then(|file| file.set_times(times))
        .expect("target mtime should be set");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(&root, &worktree, ".env", ".env")],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("changed sync should update target");

    let synced = fs::read_to_string(worktree.join(".env")).expect("target should be readable");
    assert_eq!(synced, "new\n");
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_replace_target_symlink_with_sync_file() {
    let (root, worktree) = temp_workspace("sync-file-over-symlink");
    fs::write(root.join(".env"), "new\n").expect("source should be written");
    fs::write(worktree.join("old"), "old\n").expect("old target should be written");
    std::os::unix::fs::symlink("old", worktree.join(".env"))
        .expect("target symlink should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(&root, &worktree, ".env", ".env")],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("sync file should replace target symlink");

    let synced = fs::read_to_string(worktree.join(".env")).expect("target should be readable");
    assert_eq!(synced, "new\n");
}

#[test]
fn apply_file_operations_should_reject_sync_file_to_directory_target() {
    let (root, worktree) = temp_workspace("sync-file-to-directory");
    fs::write(root.join(".env"), "new\n").expect("source should be written");
    fs::create_dir_all(worktree.join(".env")).expect("target dir should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(&root, &worktree, ".env", ".env")],
    );
    let mut reporter = VecReporter::default();

    let error = apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect_err("sync file to directory should fail");

    assert!(error.to_string().contains("target is a directory"));
}

#[test]
fn apply_file_operations_should_reject_sync_directory_to_file_target() {
    let (root, worktree) = temp_workspace("sync-directory-to-file");
    fs::create_dir_all(root.join("shared")).expect("source dir should be created");
    fs::write(worktree.join("shared"), "old\n").expect("target file should be written");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(&root, &worktree, "shared", "shared")],
    );
    let mut reporter = VecReporter::default();

    let error = apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect_err("sync directory to file should fail");

    assert!(error.to_string().contains("target is a file or symlink"));
}

#[test]
fn apply_file_operations_should_update_checksum_sync_when_metadata_matches() {
    let (root, worktree) = temp_workspace("sync-checksum-update");
    let source = root.join(".env");
    let target = worktree.join(".env");
    fs::write(&source, "ABC\n").expect("source should be written");
    fs::write(&target, "XYZ\n").expect("target should be written");
    let modified = fs::metadata(&source)
        .expect("source metadata should be readable")
        .modified()
        .expect("source mtime should be readable");
    let times = FileTimes::new().set_modified(modified);
    File::options()
        .read(true)
        .open(&target)
        .and_then(|file| file.set_times(times))
        .expect("target mtime should be aligned");
    let mut sync = sync_operation(&root, &worktree, ".env", ".env");
    sync.compare = Some(SyncCompare::Checksum);
    let plan = run_plan(&root, &worktree, vec![sync]);
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("checksum sync should update changed content");

    let synced = fs::read_to_string(target).expect("target should be readable");
    assert_eq!(synced, "ABC\n");
}

#[test]
fn apply_file_operations_should_leave_unchanged_checksum_sync_silent() {
    let (root, worktree) = temp_workspace("sync-checksum-unchanged");
    fs::write(root.join(".env"), "ABC\n").expect("source should be written");
    fs::write(worktree.join(".env"), "ABC\n").expect("target should be written");
    let mut sync = sync_operation(&root, &worktree, ".env", ".env");
    sync.compare = Some(SyncCompare::Checksum);
    let plan = run_plan(&root, &worktree, vec![sync]);
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("unchanged checksum sync should succeed");

    assert!(reporter.events.is_empty());
}

#[test]
fn apply_file_operations_should_update_checksum_sync_when_size_differs() {
    let (root, worktree) = temp_workspace("sync-checksum-size");
    fs::write(root.join(".env"), "longer\n").expect("source should be written");
    fs::write(worktree.join(".env"), "old\n").expect("target should be written");
    let mut sync = sync_operation(&root, &worktree, ".env", ".env");
    sync.compare = Some(SyncCompare::Checksum);
    let plan = run_plan(&root, &worktree, vec![sync]);
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("checksum sync should update size change");

    let synced = fs::read_to_string(worktree.join(".env")).expect("target should be readable");
    assert_eq!(synced, "longer\n");
}

#[test]
fn apply_file_operations_should_force_sync_file_like_default() {
    let (root, worktree) = temp_workspace("force-sync-file");
    fs::write(root.join(".env"), "new\n").expect("source should be written");
    fs::write(worktree.join(".env"), "old\n").expect("target should be written");
    let times = FileTimes::new().set_modified(UNIX_EPOCH);
    File::options()
        .write(true)
        .open(worktree.join(".env"))
        .and_then(|file| file.set_times(times))
        .expect("target mtime should be set");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(&root, &worktree, ".env", ".env")],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(
        &plan,
        FileApplyOptions {
            force: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect("forced sync should update target");

    let synced = fs::read_to_string(worktree.join(".env")).expect("target should be readable");
    assert_eq!(synced, "new\n");
}

#[test]
fn apply_file_operations_should_preserve_sync_directory_extras_by_default() {
    let (root, worktree) = temp_workspace("sync-no-delete");
    fs::create_dir_all(root.join("shared")).expect("source dir should be created");
    fs::create_dir_all(worktree.join("shared")).expect("target dir should be created");
    fs::write(root.join("shared/config"), "new\n").expect("source should be written");
    fs::write(worktree.join("shared/extra"), "keep\n").expect("extra should be written");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(&root, &worktree, "shared", "shared")],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("sync should preserve extras by default");

    assert!(worktree.join("shared/extra").exists());
}

#[test]
fn apply_file_operations_should_delete_target_only_entries_when_sync_delete_is_true() {
    let (root, worktree) = temp_workspace("sync-delete");
    fs::create_dir_all(root.join("shared")).expect("source dir should be created");
    fs::create_dir_all(worktree.join("shared/extra-dir"))
        .expect("target extra dir should be created");
    fs::write(root.join("shared/config"), "new\n").expect("source should be written");
    fs::write(worktree.join("shared/extra"), "remove\n").expect("extra should be written");
    fs::write(worktree.join("shared/extra-dir/file"), "remove\n")
        .expect("nested extra should be written");
    let mut sync = sync_operation(&root, &worktree, "shared", "shared");
    sync.delete = Some(true);
    let plan = run_plan(&root, &worktree, vec![sync]);
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("sync delete should remove target-only entries");

    assert!(!worktree.join("shared/extra").exists());
    assert!(!worktree.join("shared/extra-dir").exists());
    assert!(
        reporter
            .events
            .iter()
            .any(|event| matches!(event, OutputEvent::FileDeleted { .. }))
    );
}

#[test]
fn apply_file_operations_should_report_sync_delete_in_dry_run_without_mutation() {
    let (root, worktree) = temp_workspace("sync-delete-dry-run");
    fs::create_dir_all(root.join("shared")).expect("source dir should be created");
    fs::create_dir_all(worktree.join("shared")).expect("target dir should be created");
    fs::write(worktree.join("shared/extra"), "keep\n").expect("extra should be written");
    let mut sync = sync_operation(&root, &worktree, "shared", "shared");
    sync.delete = Some(true);
    let plan = run_plan(&root, &worktree, vec![sync]);
    let mut reporter = VecReporter::default();

    apply_file_operations(
        &plan,
        FileApplyOptions {
            dry_run: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect("dry-run sync delete should plan");

    assert!(worktree.join("shared/extra").exists());
    assert!(matches!(
        reporter.events.as_slice(),
        [OutputEvent::FileWouldDelete { .. }]
    ));
}

#[test]
fn apply_file_operations_should_report_sync_create_in_dry_run_without_mutation() {
    let (root, worktree) = temp_workspace("sync-create-dry-run");
    fs::write(root.join(".env"), "new\n").expect("source should be written");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(&root, &worktree, ".env", ".env")],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(
        &plan,
        FileApplyOptions {
            dry_run: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect("dry-run sync create should plan");

    assert!(!worktree.join(".env").exists());
    assert!(matches!(
        reporter.events.as_slice(),
        [OutputEvent::FileWouldApply {
            operation: FileOperationKind::Sync,
            ..
        }]
    ));
}

#[test]
fn apply_file_operations_should_report_sync_directory_create_in_dry_run() {
    let (root, worktree) = temp_workspace("sync-directory-create-dry-run");
    fs::create_dir_all(root.join("shared/nested")).expect("source dir should be created");
    fs::write(root.join("shared/nested/config"), "new\n").expect("source should be written");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(&root, &worktree, "shared", "shared")],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(
        &plan,
        FileApplyOptions {
            dry_run: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect("dry-run sync directory create should plan");

    assert!(!worktree.join("shared").exists());
    assert!(reporter.events.iter().any(|event| {
        matches!(
            event,
            OutputEvent::FileWouldApply {
                operation: FileOperationKind::Sync,
                target,
                ..
            } if target == Path::new("shared/nested/config")
        )
    }));
}

#[test]
fn apply_file_operations_should_report_sync_update_in_dry_run_without_mutation() {
    let (root, worktree) = temp_workspace("sync-update-dry-run");
    fs::write(root.join(".env"), "new-value\n").expect("source should be written");
    fs::write(worktree.join(".env"), "old\n").expect("target should be written");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(&root, &worktree, ".env", ".env")],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(
        &plan,
        FileApplyOptions {
            dry_run: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect("dry-run sync update should plan");

    let existing =
        fs::read_to_string(worktree.join(".env")).expect("target should remain readable");
    assert_eq!(existing, "old\n");
    assert!(matches!(
        reporter.events.as_slice(),
        [OutputEvent::FileWouldApply {
            operation: FileOperationKind::Sync,
            ..
        }]
    ));
}

#[test]
fn apply_file_operations_should_report_copy_skip_in_dry_run_without_mutation() {
    let (root, worktree) = temp_workspace("copy-skip-dry-run");
    fs::write(root.join(".env"), "new\n").expect("source should be written");
    fs::write(worktree.join(".env"), "old\n").expect("target should be written");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            ".env",
            ".env",
        )],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(
        &plan,
        FileApplyOptions {
            dry_run: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect("dry-run copy skip should plan");

    let existing =
        fs::read_to_string(worktree.join(".env")).expect("target should remain readable");
    assert_eq!(existing, "old\n");
    assert!(matches!(
        reporter.events.as_slice(),
        [OutputEvent::FileWouldSkip {
            operation: FileOperationKind::Copy,
            reason,
            ..
        }] if reason == "target exists"
    ));
}

#[test]
fn apply_file_operations_should_report_directory_create_in_dry_run_without_mutation() {
    let (root, worktree) = temp_workspace("directory-create-dry-run");
    fs::create_dir_all(root.join("shared")).expect("source dir should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            "shared",
            "shared",
        )],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(
        &plan,
        FileApplyOptions {
            dry_run: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect("dry-run directory copy should plan");

    assert!(!worktree.join("shared").exists());
    assert!(matches!(
        reporter.events.as_slice(),
        [OutputEvent::FileWouldApply {
            operation: FileOperationKind::Copy,
            ..
        }]
    ));
}

#[test]
fn apply_file_operations_should_delete_nested_target_only_entries() {
    let (root, worktree) = temp_workspace("sync-nested-delete");
    fs::create_dir_all(root.join("shared/nested")).expect("source dir should be created");
    fs::create_dir_all(worktree.join("shared/nested")).expect("target dir should be created");
    fs::write(root.join("shared/nested/config"), "keep\n").expect("source file should be written");
    fs::write(worktree.join("shared/nested/old"), "remove\n")
        .expect("nested extra should be written");
    let mut sync = sync_operation(&root, &worktree, "shared", "shared");
    sync.delete = Some(true);
    let plan = run_plan(&root, &worktree, vec![sync]);
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("sync delete should remove nested target-only entries");

    assert!(!worktree.join("shared/nested/old").exists());
    assert!(worktree.join("shared/nested/config").exists());
}

#[test]
fn apply_file_operations_should_leave_dry_run_unmutated() {
    let (root, worktree) = temp_workspace("dry-run");
    fs::write(root.join(".env"), "TOKEN=1\n").expect("source should be written");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            ".env",
            ".env",
        )],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(
        &plan,
        FileApplyOptions {
            dry_run: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect("dry-run should plan");

    assert!(!worktree.join(".env").exists());
    assert!(matches!(
        reporter.events.as_slice(),
        [OutputEvent::FileWouldApply { .. }]
    ));
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_force_copy_file_over_existing_symlink() {
    let (root, worktree) = temp_workspace("force-copy-over-symlink");
    fs::write(root.join(".env"), "new\n").expect("source should be written");
    fs::write(worktree.join("old"), "old\n").expect("old target should be written");
    std::os::unix::fs::symlink("old", worktree.join(".env"))
        .expect("target symlink should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            ".env",
            ".env",
        )],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(
        &plan,
        FileApplyOptions {
            force: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect("force copy should replace target symlink");

    let copied = fs::read_to_string(worktree.join(".env")).expect("target should be readable");
    assert_eq!(copied, "new\n");
    assert!(
        !fs::symlink_metadata(worktree.join(".env"))
            .expect("target metadata should be readable")
            .file_type()
            .is_symlink()
    );
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_copy_read_only_file() {
    use std::os::unix::fs::PermissionsExt;

    let (root, worktree) = temp_workspace("read-only-copy");
    let source = root.join(".env");
    let target = worktree.join(".env");
    fs::write(&source, "TOKEN=1\n").expect("source should be written");
    let mut permissions = fs::metadata(&source)
        .expect("source metadata should be readable")
        .permissions();
    permissions.set_mode(0o444);
    fs::set_permissions(&source, permissions).expect("source should become read-only");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            ".env",
            ".env",
        )],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("read-only source should copy");

    let copied = fs::read_to_string(&target).expect("target should be readable");
    let mode = fs::metadata(&target)
        .expect("target metadata should be readable")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(copied, "TOKEN=1\n");
    assert_eq!(mode, 0o444);
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_force_symlink_over_existing_symlink() {
    let (root, worktree) = temp_workspace("force-symlink-over-symlink");
    fs::write(root.join("tool"), "tool\n").expect("source should be written");
    fs::write(worktree.join("old"), "old\n").expect("old target should be written");
    std::os::unix::fs::symlink("old", worktree.join(".tool"))
        .expect("target symlink should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Symlink,
            &root,
            &worktree,
            "tool",
            ".tool",
        )],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(
        &plan,
        FileApplyOptions {
            force: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect("forced symlink should replace existing symlink");

    let link = fs::read_link(worktree.join(".tool")).expect("target should be symlink");
    assert_ne!(link, PathBuf::from("old"));
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_force_symlink_over_existing_file() {
    let (root, worktree) = temp_workspace("force-symlink");
    fs::write(root.join("tool"), "tool\n").expect("source should be written");
    fs::write(worktree.join(".tool"), "old\n").expect("target should be written");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Symlink,
            &root,
            &worktree,
            "tool",
            ".tool",
        )],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(
        &plan,
        FileApplyOptions {
            force: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect("forced symlink should apply");

    let link = fs::read_link(worktree.join(".tool")).expect("target should be symlink");
    assert!(!link.is_absolute());
}

#[test]
fn apply_file_operations_should_report_symlink_create_in_dry_run() {
    let (root, worktree) = temp_workspace("symlink-create-dry-run");
    fs::write(root.join("tool"), "tool\n").expect("source should be written");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Symlink,
            &root,
            &worktree,
            "tool",
            ".tool",
        )],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(
        &plan,
        FileApplyOptions {
            dry_run: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect("dry-run symlink should plan");

    assert!(!worktree.join(".tool").exists());
    assert!(matches!(
        reporter.events.as_slice(),
        [OutputEvent::FileWouldApply {
            operation: FileOperationKind::Symlink,
            ..
        }]
    ));
}

#[test]
fn apply_file_operations_should_skip_existing_symlink_target_by_default() {
    let (root, worktree) = temp_workspace("skip-symlink");
    fs::write(root.join("tool"), "tool\n").expect("source should be written");
    fs::write(worktree.join(".tool"), "old\n").expect("target should be written");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Symlink,
            &root,
            &worktree,
            "tool",
            ".tool",
        )],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("existing target should skip");

    assert!(matches!(
        reporter.events.as_slice(),
        [OutputEvent::FileSkipped { reason, .. }] if reason == "target exists"
    ));
}

#[test]
fn apply_file_operations_should_reject_existing_symlink_target_in_strict() {
    let (root, worktree) = temp_workspace("strict-symlink");
    fs::write(root.join("tool"), "tool\n").expect("source should be written");
    fs::write(worktree.join(".tool"), "old\n").expect("target should be written");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Symlink,
            &root,
            &worktree,
            "tool",
            ".tool",
        )],
    );
    let mut reporter = VecReporter::default();

    let error = apply_file_operations(
        &plan,
        FileApplyOptions {
            strict: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect_err("strict symlink target should fail");

    assert!(error.to_string().contains("target exists"));
}

#[test]
fn apply_file_operations_should_reject_symlink_to_existing_directory_even_with_force() {
    let (root, worktree) = temp_workspace("symlink-existing-dir");
    fs::write(root.join("tool"), "tool\n").expect("source should be written");
    fs::create_dir_all(worktree.join(".tool")).expect("target dir should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Symlink,
            &root,
            &worktree,
            "tool",
            ".tool",
        )],
    );
    let mut reporter = VecReporter::default();

    let error = apply_file_operations(
        &plan,
        FileApplyOptions {
            force: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect_err("symlink over directory should fail");

    assert!(error.to_string().contains("target is a directory"));
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_reject_sync_symlink_to_existing_directory() {
    let (root, worktree) = temp_workspace("sync-symlink-existing-dir");
    fs::write(root.join("tool"), "tool\n").expect("source target should be written");
    std::os::unix::fs::symlink("tool", root.join("link"))
        .expect("source symlink should be created");
    fs::create_dir_all(worktree.join("link")).expect("target dir should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(&root, &worktree, "link", "link")],
    );
    let mut reporter = VecReporter::default();

    let error = apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect_err("sync symlink over directory should fail");

    assert!(error.to_string().contains("target is a directory"));
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_preserve_copied_source_symlinks() {
    let (root, worktree) = temp_workspace("preserved-symlink");
    let source_dir = root.join("shared");
    fs::create_dir_all(&source_dir).expect("source dir should be created");
    fs::write(source_dir.join("config"), "value\n").expect("source should be written");
    std::os::unix::fs::symlink("config", source_dir.join("link"))
        .expect("source symlink should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            "shared",
            "shared",
        )],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("directory copy should apply");

    let link = fs::read_link(worktree.join("shared/link")).expect("copied symlink should exist");
    assert_eq!(link, PathBuf::from("config"));
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_warn_for_preserved_symlink_to_uncopied_target() {
    let (root, worktree) = temp_workspace("preserved-symlink-warning");
    let source_dir = root.join("shared");
    fs::create_dir_all(&source_dir).expect("source dir should be created");
    fs::write(source_dir.join("config"), "value\n").expect("source should be written");
    std::os::unix::fs::symlink("config", source_dir.join("link"))
        .expect("source symlink should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            "shared/link",
            "shared/link",
        )],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("copied symlink should apply");

    assert!(reporter.events.iter().any(|event| matches!(
        event,
        OutputEvent::FileWarning { reason, .. }
            if reason == "symlink target does not exist"
    )));
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_report_symlink_warning_in_dry_run() {
    let (root, worktree) = temp_workspace("preserved-symlink-warning-dry-run");
    let source_dir = root.join("shared");
    fs::create_dir_all(&source_dir).expect("source dir should be created");
    fs::write(source_dir.join("config"), "value\n").expect("source should be written");
    std::os::unix::fs::symlink("config", source_dir.join("link"))
        .expect("source symlink should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            "shared/link",
            "shared/link",
        )],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(
        &plan,
        FileApplyOptions {
            dry_run: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect("dry-run copied symlink should plan");

    assert!(matches!(
        reporter.events.as_slice(),
        [
            OutputEvent::FileWouldApply { .. },
            OutputEvent::FileWarning { reason, .. }
        ] if reason == "symlink target does not exist"
    ));
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_sync_preserved_source_symlinks() {
    let (root, worktree) = temp_workspace("sync-preserved-symlink");
    let source_dir = root.join("shared");
    fs::create_dir_all(&source_dir).expect("source dir should be created");
    fs::write(source_dir.join("config"), "value\n").expect("source should be written");
    std::os::unix::fs::symlink("config", source_dir.join("link"))
        .expect("source symlink should be created");
    fs::create_dir_all(worktree.join("shared")).expect("target dir should be created");
    fs::write(worktree.join("shared/link"), "old\n").expect("target should be written");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(&root, &worktree, "shared", "shared")],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("sync should preserve source symlink");

    let link = fs::read_link(worktree.join("shared/link")).expect("synced symlink should exist");
    assert_eq!(link, PathBuf::from("config"));
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_sync_top_level_source_symlink() {
    let (root, worktree) = temp_workspace("sync-top-level-symlink");
    fs::write(root.join("config"), "value\n").expect("source target should be written");
    std::os::unix::fs::symlink("config", root.join("link"))
        .expect("source symlink should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(&root, &worktree, "link", "link")],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("sync should create top-level symlink");

    let link = fs::read_link(worktree.join("link")).expect("synced symlink should exist");
    assert_eq!(link, PathBuf::from("config"));
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_warn_for_sync_symlink_to_uncopied_target() {
    let (root, worktree) = temp_workspace("sync-symlink-warning");
    let source_dir = root.join("shared");
    fs::create_dir_all(&source_dir).expect("source dir should be created");
    fs::write(source_dir.join("config"), "value\n").expect("source should be written");
    std::os::unix::fs::symlink("config", source_dir.join("link"))
        .expect("source symlink should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(
            &root,
            &worktree,
            "shared/link",
            "shared/link",
        )],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("synced symlink should apply");

    assert!(reporter.events.iter().any(|event| matches!(
        event,
        OutputEvent::FileWarning { reason, .. }
            if reason == "symlink target does not exist"
    )));
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_update_changed_sync_symlink_target() {
    let (root, worktree) = temp_workspace("sync-changed-symlink");
    fs::write(root.join("config"), "value\n").expect("source target should be written");
    fs::write(root.join("other"), "value\n").expect("other source target should be written");
    std::os::unix::fs::symlink("config", root.join("link"))
        .expect("source symlink should be created");
    std::os::unix::fs::symlink("../root/other", worktree.join("link"))
        .expect("target symlink should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(&root, &worktree, "link", "link")],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("sync should update changed symlink");

    let link = fs::read_link(worktree.join("link")).expect("synced symlink should exist");
    assert_eq!(link, PathBuf::from("config"));
}

#[cfg(unix)]
#[test]
fn preserved_source_link_should_track_directory_target_type() {
    let (root, worktree) = temp_workspace("preserved-directory-symlink");
    let source_dir = root.join("shared");
    fs::create_dir_all(source_dir.join("dir")).expect("source dir should be created");
    std::os::unix::fs::symlink("dir", source_dir.join("link"))
        .expect("source symlink should be created");
    let plan = run_plan(&root, &worktree, Vec::new());

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
