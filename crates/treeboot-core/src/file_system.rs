use std::fs::{self, File, FileTimes, Metadata};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use crate::file_actions::{MetadataPolicy, MetadataTarget};
use crate::{
    ActionPlan, Error, FileOperationKind, OutputEvent, PlannedFileOperation, Reporter, Result,
    SyncCompare,
};

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};

/// Identifies which side of a checksum comparison produced a read error so the
/// caller can attribute it to the right path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContentInput {
    Source,
    Target,
}

/// A read error from one side of a checksum comparison, tagged with the side
/// it came from. The comparison itself is path-agnostic; the caller resolves
/// `input` back to a concrete path when building the public error.
#[derive(Debug)]
pub(crate) struct ContentReadError {
    pub(crate) input: ContentInput,
    pub(crate) source: io::Error,
}

pub(crate) fn file_sync_changed(
    operation: &PlannedFileOperation,
    source_path: &Path,
    target_path: &Path,
    target_metadata: &Metadata,
) -> Result<bool> {
    if target_metadata.file_type().is_symlink() {
        return Ok(true);
    }

    match operation.compare().unwrap_or(SyncCompare::Metadata) {
        SyncCompare::Metadata => {
            metadata_changed(operation, source_path, target_path, target_metadata)
        }
        SyncCompare::Checksum => contents_changed(operation, source_path, target_path),
    }
}

fn metadata_changed(
    operation: &PlannedFileOperation,
    source_path: &Path,
    target_path: &Path,
    target_metadata: &Metadata,
) -> Result<bool> {
    let source_metadata = metadata(source_path, operation.operation())?;
    if source_metadata.len() != target_metadata.len() {
        return Ok(true);
    }

    let source_modified = source_metadata
        .modified()
        .map_err(|source| Error::FileOperationIo {
            operation: operation.operation().as_str(),
            path: source_path.to_path_buf(),
            source,
        })?;
    let target_modified = target_metadata
        .modified()
        .map_err(|source| Error::FileOperationIo {
            operation: operation.operation().as_str(),
            path: target_path.to_path_buf(),
            source,
        })?;

    Ok(source_modified != target_modified)
}

fn contents_changed(
    operation: &PlannedFileOperation,
    source_path: &Path,
    target_path: &Path,
) -> Result<bool> {
    let source_metadata = metadata(source_path, operation.operation())?;
    let target_metadata = metadata(target_path, operation.operation())?;
    if source_metadata.len() != target_metadata.len() {
        return Ok(true);
    }

    let mut source_file = File::open(source_path).map_err(|source| Error::FileOperationIo {
        operation: operation.operation().as_str(),
        path: source_path.to_path_buf(),
        source,
    })?;
    let mut target_file = File::open(target_path).map_err(|source| Error::FileOperationIo {
        operation: operation.operation().as_str(),
        path: target_path.to_path_buf(),
        source,
    })?;

    reader_contents_changed(&mut source_file, &mut target_file).map_err(|error| {
        let path = match error.input {
            ContentInput::Source => source_path,
            ContentInput::Target => target_path,
        };
        Error::FileOperationIo {
            operation: operation.operation().as_str(),
            path: path.to_path_buf(),
            source: error.source,
        }
    })
}

/// Compare two readers byte-for-byte, treating any divergence as changed.
///
/// The caller guarantees the underlying sources are the same length, so a
/// difference in read counts can only come from a concurrent truncation and is
/// reported as changed. Each chunk is filled with [`read_full_chunk`] so a
/// short read on one side cannot masquerade as a content difference. The error
/// carries which side failed via [`ContentInput`]; the caller maps it back to
/// a path.
pub(crate) fn reader_contents_changed(
    source: &mut impl Read,
    target: &mut impl Read,
) -> std::result::Result<bool, ContentReadError> {
    let mut source_buf = [0; 8192];
    let mut target_buf = [0; 8192];

    loop {
        let source_read = read_full_chunk(source, &mut source_buf, ContentInput::Source)?;
        let target_read = read_full_chunk(target, &mut target_buf, ContentInput::Target)?;

        if source_read != target_read {
            return Ok(true);
        }
        if source_read == 0 {
            return Ok(false);
        }
        if source_buf[..source_read] != target_buf[..target_read] {
            return Ok(true);
        }
    }
}

/// Read into `buffer` until it is full or end of file is reached.
///
/// [`Read::read`] may return fewer bytes than requested even when more data
/// remains, so a single call is not a reliable chunk boundary. This retries
/// until `buffer` is completely filled or the reader reports EOF, and returns
/// the number of bytes read; a count below `buffer.len()` means EOF was
/// reached. [`io::ErrorKind::Interrupted`] is retried to match the standard
/// library's own read helpers. Read errors are tagged with `input` so the
/// caller can attribute them to the correct file.
pub(crate) fn read_full_chunk(
    reader: &mut impl Read,
    buffer: &mut [u8],
    input: ContentInput,
) -> std::result::Result<usize, ContentReadError> {
    let mut filled = 0;
    while filled < buffer.len() {
        match reader.read(&mut buffer[filled..]) {
            Ok(0) => break,
            Ok(read) => filled += read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(source) => return Err(ContentReadError { input, source }),
        }
    }
    Ok(filled)
}

pub(crate) fn metadata_drifted(
    operation: FileOperationKind,
    source_path: &Path,
    target_path: &Path,
    target_metadata: &Metadata,
    target_kind: MetadataTarget,
    policy: MetadataPolicy,
) -> Result<bool> {
    let source_metadata = metadata(source_path, operation)?;
    if policy.permissions && !permissions_match(&source_metadata, target_metadata) {
        return Ok(true);
    }
    if target_kind == MetadataTarget::File {
        let source_modified =
            source_metadata
                .modified()
                .map_err(|source| Error::FileOperationIo {
                    operation: operation.as_str(),
                    path: source_path.to_path_buf(),
                    source,
                })?;
        let target_modified =
            target_metadata
                .modified()
                .map_err(|source| Error::FileOperationIo {
                    operation: operation.as_str(),
                    path: target_path.to_path_buf(),
                    source,
                })?;
        if source_modified != target_modified {
            return Ok(true);
        }
    }
    Ok(ownership_drifted(&source_metadata, target_metadata, policy))
}

#[cfg(unix)]
fn permissions_match(source: &Metadata, target: &Metadata) -> bool {
    source.mode() == target.mode()
}

#[cfg(not(unix))]
fn permissions_match(source: &Metadata, target: &Metadata) -> bool {
    source.permissions().readonly() == target.permissions().readonly()
}

#[cfg(unix)]
fn ownership_drifted(source: &Metadata, target: &Metadata, policy: MetadataPolicy) -> bool {
    (policy.owner && source.uid() != target.uid()) || (policy.group && source.gid() != target.gid())
}

#[cfg(not(unix))]
fn ownership_drifted(_source: &Metadata, _target: &Metadata, _policy: MetadataPolicy) -> bool {
    false
}

pub(crate) fn with_writable_parent<F>(
    operation: FileOperationKind,
    target_path: &Path,
    worktree_path: &Path,
    action: F,
) -> Result<()>
where
    F: FnOnce() -> Result<()>,
{
    if target_path.starts_with(worktree_path) {
        ensure_target_ancestors(
            operation,
            target_parent(target_path, worktree_path),
            worktree_path,
            false,
        )?;
    }
    let restore = prepare_parent_for_writes(operation, target_path, worktree_path)?;
    let result = action();
    let restore_result = if let Some((path, permissions)) = restore {
        fs::set_permissions(&path, permissions).map_err(|source| Error::FileOperationIo {
            operation: operation.as_str(),
            path,
            source,
        })
    } else {
        Ok(())
    };

    match (result, restore_result) {
        (Err(error), _) => Err(error),
        (Ok(()), Ok(())) => Ok(()),
        (Ok(()), Err(error)) => Err(error),
    }
}

fn prepare_parent_for_writes(
    operation: FileOperationKind,
    target_path: &Path,
    worktree_path: &Path,
) -> Result<Option<(PathBuf, fs::Permissions)>> {
    if !target_path.starts_with(worktree_path) {
        return Ok(None);
    }

    let Some(parent) = nearest_existing_parent(
        operation,
        target_parent(target_path, worktree_path),
        worktree_path,
    )?
    else {
        return Ok(None);
    };
    let metadata = fs::symlink_metadata(&parent).map_err(|source| Error::FileOperationIo {
        operation: operation.as_str(),
        path: parent.clone(),
        source,
    })?;
    if !metadata.is_dir() {
        return Ok(None);
    }
    let permissions = metadata.permissions();
    if directory_permissions_allow_writes(&permissions) {
        return Ok(None);
    }

    let mut writable = permissions.clone();
    make_directory_permissions_writable(&mut writable);
    fs::set_permissions(&parent, writable).map_err(|source| Error::FileOperationIo {
        operation: operation.as_str(),
        path: parent.clone(),
        source,
    })?;
    Ok(Some((parent, permissions)))
}

fn nearest_existing_parent(
    operation: FileOperationKind,
    path: &Path,
    worktree_path: &Path,
) -> Result<Option<PathBuf>> {
    let mut current = path;
    loop {
        match fs::symlink_metadata(current) {
            Ok(_) => return Ok(Some(current.to_path_buf())),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                if current == worktree_path {
                    return Ok(None);
                }
                let Some(parent) = current.parent() else {
                    return Ok(None);
                };
                current = parent;
            }
            Err(source) => {
                return Err(Error::FileOperationIo {
                    operation: operation.as_str(),
                    path: current.to_path_buf(),
                    source,
                });
            }
        }
    }
}

#[cfg(unix)]
fn directory_permissions_allow_writes(permissions: &fs::Permissions) -> bool {
    permissions.mode() & 0o222 != 0
}

#[cfg(not(unix))]
fn directory_permissions_allow_writes(permissions: &fs::Permissions) -> bool {
    !permissions.readonly()
}

#[cfg(unix)]
fn make_directory_permissions_writable(permissions: &mut fs::Permissions) {
    permissions.set_mode(permissions.mode() | 0o200);
}

#[cfg(not(unix))]
fn make_directory_permissions_writable(permissions: &mut fs::Permissions) {
    permissions.set_readonly(false);
}

pub(crate) fn create_parent_dir(
    operation: FileOperationKind,
    target_path: &Path,
    worktree_path: &Path,
) -> Result<()> {
    let parent = target_parent(target_path, worktree_path);

    if target_path.starts_with(worktree_path) {
        ensure_target_ancestors(operation, parent, worktree_path, false)?;
    }

    fs::create_dir_all(parent).map_err(|source| Error::FileOperationIo {
        operation: operation.as_str(),
        path: parent.to_path_buf(),
        source,
    })?;
    if target_path.starts_with(worktree_path) {
        ensure_target_ancestors(operation, parent, worktree_path, true)?;
    }

    Ok(())
}

pub(crate) fn create_target_dir(
    operation: FileOperationKind,
    target_path: &Path,
    worktree_path: &Path,
) -> Result<()> {
    if target_path.starts_with(worktree_path) {
        ensure_target_ancestors(operation, target_path, worktree_path, false)?;
    }
    fs::create_dir_all(target_path).map_err(|source| Error::FileOperationIo {
        operation: operation.as_str(),
        path: target_path.to_path_buf(),
        source,
    })?;
    if target_path.starts_with(worktree_path) {
        ensure_target_ancestors(operation, target_path, worktree_path, true)?;
    }

    Ok(())
}

fn ensure_target_ancestors(
    operation: FileOperationKind,
    path: &Path,
    worktree_path: &Path,
    require_exists: bool,
) -> Result<()> {
    if !path.starts_with(worktree_path) {
        return conflict(
            operation,
            path.to_path_buf(),
            "target resolves outside worktree during apply",
        );
    }

    let mut chain = Vec::new();
    let mut current = path;
    loop {
        chain.push(current.to_path_buf());
        if current == worktree_path {
            break;
        }
        let Some(parent) = current.parent() else {
            return conflict(
                operation,
                path.to_path_buf(),
                "target resolves outside worktree during apply",
            );
        };
        current = parent;
    }

    for ancestor in chain.iter().rev() {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return conflict(operation, ancestor.clone(), "target parent is a symlink");
            }
            Ok(metadata) if !metadata.is_dir() => {
                return conflict(
                    operation,
                    ancestor.clone(),
                    "target parent is not a directory",
                );
            }
            Ok(_) => {}
            Err(source) if source.kind() == std::io::ErrorKind::NotFound && !require_exists => {
                return Ok(());
            }
            Err(source) => {
                return Err(Error::FileOperationIo {
                    operation: operation.as_str(),
                    path: ancestor.clone(),
                    source,
                });
            }
        }
    }

    Ok(())
}

#[cfg(test)]
pub(crate) fn copy_file_with_metadata(
    operation: FileOperationKind,
    source_path: &Path,
    target_path: &Path,
    root_path: &Path,
    worktree_path: &Path,
) -> Result<()> {
    copy_file_with_metadata_with_policy(
        operation,
        source_path,
        target_path,
        root_path,
        worktree_path,
        MetadataPolicy::default(),
        None,
    )
}

pub(crate) fn copy_file_with_metadata_with_policy(
    operation: FileOperationKind,
    source_path: &Path,
    target_path: &Path,
    root_path: &Path,
    worktree_path: &Path,
    metadata_policy: MetadataPolicy,
    reporter: Option<&mut dyn Reporter>,
) -> Result<()> {
    let source_metadata = ensure_source_file_safe(operation, source_path, root_path)?;
    let mut source_file = File::open(source_path).map_err(|source| Error::FileOperationIo {
        operation: operation.as_str(),
        path: source_path.to_path_buf(),
        source,
    })?;
    ensure_target_parent_exists(operation, target_path, worktree_path)?;
    let mut target_file = File::options()
        .write(true)
        .create_new(true)
        .open(target_path)
        .map_err(|source| Error::FileOperationIo {
            operation: operation.as_str(),
            path: target_path.to_path_buf(),
            source,
        })?;

    io::copy(&mut source_file, &mut target_file).map_err(|source| Error::FileOperationIo {
        operation: operation.as_str(),
        path: target_path.to_path_buf(),
        source,
    })?;
    drop(target_file);

    apply_metadata_from_source(
        operation,
        target_path,
        &source_metadata,
        metadata_policy,
        MetadataTarget::File,
        reporter,
    )
}

pub(crate) fn apply_metadata(
    operation: FileOperationKind,
    source_path: &Path,
    target_path: &Path,
    policy: MetadataPolicy,
    target_kind: MetadataTarget,
    reporter: Option<&mut dyn Reporter>,
) -> Result<()> {
    let metadata = metadata(source_path, operation)?;
    apply_metadata_from_source(
        operation,
        target_path,
        &metadata,
        policy,
        target_kind,
        reporter,
    )
}

fn apply_metadata_from_source(
    operation: FileOperationKind,
    target_path: &Path,
    metadata: &Metadata,
    policy: MetadataPolicy,
    target_kind: MetadataTarget,
    reporter: Option<&mut dyn Reporter>,
) -> Result<()> {
    apply_ownership(operation, target_path, metadata, policy, reporter)?;
    if target_kind == MetadataTarget::File {
        apply_file_times(operation, target_path, metadata)?;
    }
    if policy.permissions {
        fs::set_permissions(target_path, metadata.permissions()).map_err(|source| {
            Error::FileOperationIo {
                operation: operation.as_str(),
                path: target_path.to_path_buf(),
                source,
            }
        })?;
    }
    Ok(())
}

fn apply_file_times(
    operation: FileOperationKind,
    target_path: &Path,
    metadata: &Metadata,
) -> Result<()> {
    let mut times = FileTimes::new();
    if let Ok(accessed) = metadata.accessed() {
        times = times.set_accessed(accessed);
    }
    if let Ok(modified) = metadata.modified() {
        times = times.set_modified(modified);
    }
    File::open(target_path)
        .and_then(|file| file.set_times(times))
        .or_else(|source| {
            if source.kind() == std::io::ErrorKind::PermissionDenied {
                File::options()
                    .write(true)
                    .open(target_path)
                    .and_then(|file| file.set_times(times))
            } else {
                Err(source)
            }
        })
        .map_err(|source| Error::FileOperationIo {
            operation: operation.as_str(),
            path: target_path.to_path_buf(),
            source,
        })?;
    Ok(())
}

#[cfg(unix)]
fn apply_ownership(
    operation: FileOperationKind,
    target_path: &Path,
    metadata: &Metadata,
    policy: MetadataPolicy,
    reporter: Option<&mut dyn Reporter>,
) -> Result<()> {
    if !policy.owner && !policy.group {
        return Ok(());
    }

    let target_metadata =
        fs::symlink_metadata(target_path).map_err(|source| Error::FileOperationIo {
            operation: operation.as_str(),
            path: target_path.to_path_buf(),
            source,
        })?;
    let uid = (policy.owner && metadata.uid() != target_metadata.uid()).then_some(metadata.uid());
    let gid = (policy.group && metadata.gid() != target_metadata.gid()).then_some(metadata.gid());
    if uid.is_none() && gid.is_none() {
        return Ok(());
    }

    match std::os::unix::fs::chown(target_path, uid, gid) {
        Ok(()) => Ok(()),
        Err(source) if source.kind() == std::io::ErrorKind::PermissionDenied => {
            if let Some(reporter) = reporter {
                report(
                    reporter,
                    OutputEvent::OwnershipWarning {
                        path: target_path.to_path_buf(),
                        reason: source.to_string(),
                    },
                )?;
            }
            Ok(())
        }
        Err(source) => Err(Error::FileOperationIo {
            operation: operation.as_str(),
            path: target_path.to_path_buf(),
            source,
        }),
    }
}

#[cfg(not(unix))]
fn apply_ownership(
    _operation: FileOperationKind,
    _target_path: &Path,
    _metadata: &Metadata,
    _policy: MetadataPolicy,
    _reporter: Option<&mut dyn Reporter>,
) -> Result<()> {
    Ok(())
}

fn ensure_source_file_safe(
    operation: FileOperationKind,
    source_path: &Path,
    root_path: &Path,
) -> Result<Metadata> {
    let metadata = metadata(source_path, operation)?;
    if metadata.file_type().is_symlink() {
        return conflict(
            operation,
            source_path.to_path_buf(),
            "source changed to a symlink before apply",
        );
    }
    if !metadata.is_file() {
        return conflict(
            operation,
            source_path.to_path_buf(),
            "source file type changed before apply",
        );
    }

    let source_should_stay_in_root = source_path.starts_with(root_path);
    let source_path = fs::canonicalize(source_path).map_err(|source| Error::FileOperationIo {
        operation: operation.as_str(),
        path: source_path.to_path_buf(),
        source,
    })?;
    let root_path = fs::canonicalize(root_path).map_err(|source| Error::FileOperationIo {
        operation: operation.as_str(),
        path: root_path.to_path_buf(),
        source,
    })?;
    if source_should_stay_in_root && !source_path.starts_with(&root_path) {
        return conflict(
            operation,
            source_path,
            "source resolves outside root during apply",
        );
    }

    Ok(metadata)
}

pub(crate) fn ensure_preserved_source_symlink_safe(
    plan: &ActionPlan,
    operation: FileOperationKind,
    source_path: &Path,
    target_path: &Path,
    link_target: &Path,
    final_target: &Path,
    target_is_dir: bool,
) -> Result<()> {
    let metadata = metadata(source_path, operation)?;
    if !metadata.file_type().is_symlink() {
        return conflict(
            operation,
            source_path.to_path_buf(),
            "source changed from a symlink before apply",
        );
    }

    let source_target = fs::canonicalize(source_path).map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            return Error::FileOperationConflict {
                operation: operation.as_str(),
                path: source_path.to_path_buf(),
                message: "source symlink changed before apply".to_owned(),
            };
        }
        Error::FileOperationIo {
            operation: operation.as_str(),
            path: source_path.to_path_buf(),
            source,
        }
    })?;
    let root_path =
        fs::canonicalize(&plan.context().root_path).map_err(|source| Error::FileOperationIo {
            operation: operation.as_str(),
            path: plan.context().root_path.clone(),
            source,
        })?;
    if !source_target.starts_with(&root_path) {
        return conflict(
            operation,
            source_target,
            "source symlink resolves outside root during apply",
        );
    }

    let (current_link_target, current_final_target, current_target_is_dir) =
        preserved_source_link(plan, operation, source_path, target_path)?;
    if current_link_target != link_target
        || current_final_target != final_target
        || current_target_is_dir != target_is_dir
    {
        return conflict(
            operation,
            source_path.to_path_buf(),
            "source symlink changed before apply",
        );
    }

    Ok(())
}

pub(crate) fn remove_file_checked(
    operation: FileOperationKind,
    path: &Path,
    worktree_path: &Path,
) -> Result<()> {
    if path.starts_with(worktree_path) {
        ensure_target_ancestors(
            operation,
            target_parent(path, worktree_path),
            worktree_path,
            true,
        )?;
    }
    remove_file(operation, path)
}

fn ensure_target_parent_exists(
    operation: FileOperationKind,
    path: &Path,
    worktree_path: &Path,
) -> Result<()> {
    if path.starts_with(worktree_path) {
        ensure_target_ancestors(
            operation,
            target_parent(path, worktree_path),
            worktree_path,
            true,
        )?;
    }

    Ok(())
}

fn remove_file(operation: FileOperationKind, path: &Path) -> Result<()> {
    fs::remove_file(path).map_err(|source| Error::FileOperationIo {
        operation: operation.as_str(),
        path: path.to_path_buf(),
        source,
    })
}

pub(crate) fn remove_any(
    operation: FileOperationKind,
    path: &Path,
    worktree_path: &Path,
) -> Result<()> {
    if path.starts_with(worktree_path) {
        ensure_target_ancestors(
            operation,
            target_parent(path, worktree_path),
            worktree_path,
            true,
        )?;
    }

    let metadata = metadata(path, operation)?;
    if metadata.is_dir() {
        fs::remove_dir_all(path).map_err(|source| Error::FileOperationIo {
            operation: operation.as_str(),
            path: path.to_path_buf(),
            source,
        })
    } else {
        remove_file(operation, path)
    }
}

fn target_parent<'a>(path: &'a Path, worktree_path: &'a Path) -> &'a Path {
    if path == worktree_path {
        return worktree_path;
    }

    path.parent().unwrap_or(worktree_path)
}

pub(crate) fn create_symlink(
    operation: FileOperationKind,
    source: &Path,
    target_is_dir: bool,
    target: &Path,
    worktree_path: &Path,
) -> Result<()> {
    ensure_target_parent_exists(operation, target, worktree_path)?;
    create_symlink_impl(source, target, target_is_dir).map_err(|source| Error::FileOperationIo {
        operation: operation.as_str(),
        path: target.to_path_buf(),
        source,
    })
}

#[cfg(unix)]
fn create_symlink_impl(source: &Path, target: &Path, _target_is_dir: bool) -> std::io::Result<()> {
    std::os::unix::fs::symlink(source, target)
}

#[cfg(windows)]
fn create_symlink_impl(source: &Path, target: &Path, target_is_dir: bool) -> std::io::Result<()> {
    if target_is_dir {
        std::os::windows::fs::symlink_dir(source, target)
    } else {
        std::os::windows::fs::symlink_file(source, target)
    }
}

pub(crate) fn preserved_source_link(
    plan: &ActionPlan,
    operation: FileOperationKind,
    source_path: &Path,
    target_path: &Path,
) -> Result<(PathBuf, PathBuf, bool)> {
    let raw_target = fs::read_link(source_path).map_err(|source| Error::FileOperationIo {
        operation: operation.as_str(),
        path: source_path.to_path_buf(),
        source,
    })?;
    if raw_target.as_os_str().is_empty() {
        return conflict(
            operation,
            source_path.to_path_buf(),
            "source symlink target is empty",
        );
    }

    let source_parent = source_path.parent().unwrap_or_else(|| Path::new("."));
    let resolved_target = if raw_target.is_absolute() {
        normalize_lexical(&raw_target)
    } else {
        normalize_lexical(&source_parent.join(&raw_target))
    };
    let target_is_dir = fs::metadata(&resolved_target)
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false);
    let final_target = resolved_target
        .strip_prefix(&plan.context().root_path)
        .map_or(resolved_target.clone(), |relative| {
            plan.context().worktree_path.join(relative)
        });
    let target_parent = target_path.parent().unwrap_or_else(|| Path::new("."));
    let link_target =
        relative_path(target_parent, &final_target).unwrap_or_else(|| raw_target.clone());

    Ok((link_target, final_target, target_is_dir))
}

pub(crate) fn raw_source_path(plan: &ActionPlan, operation: &PlannedFileOperation) -> PathBuf {
    if operation.source().is_absolute() {
        operation.source().to_path_buf()
    } else {
        normalize_lexical(&plan.context().root_path.join(operation.source()))
    }
}

pub(crate) fn metadata(path: &Path, operation: FileOperationKind) -> Result<Metadata> {
    fs::symlink_metadata(path).map_err(|source| Error::FileOperationIo {
        operation: operation.as_str(),
        path: path.to_path_buf(),
        source,
    })
}

pub(crate) fn maybe_metadata(
    path: &Path,
    operation: FileOperationKind,
) -> Result<Option<Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(Error::FileOperationIo {
            operation: operation.as_str(),
            path: path.to_path_buf(),
            source,
        }),
    }
}

pub(crate) fn conflict<T>(
    operation: FileOperationKind,
    path: PathBuf,
    message: impl Into<String>,
) -> Result<T> {
    Err(Error::FileOperationConflict {
        operation: operation.as_str(),
        path,
        message: message.into(),
    })
}

pub(crate) fn relative_path(from: &Path, to: &Path) -> Option<PathBuf> {
    let from_components = comparable_components(from)?;
    let to_components = comparable_components(to)?;

    if from_components.first() != to_components.first() {
        return None;
    }

    let common_len = from_components
        .iter()
        .zip(&to_components)
        .take_while(|(left, right)| left == right)
        .count();
    let mut relative = PathBuf::new();

    for _ in &from_components[common_len..] {
        relative.push("..");
    }
    for component in &to_components[common_len..] {
        relative.push(component);
    }

    if relative.as_os_str().is_empty() {
        relative.push(".");
    }

    Some(relative)
}

fn comparable_components(path: &Path) -> Option<Vec<PathBuf>> {
    let mut components = Vec::new();

    for component in path.components() {
        match component {
            Component::Prefix(prefix) => components.push(PathBuf::from(prefix.as_os_str())),
            Component::RootDir => components.push(PathBuf::from(component.as_os_str())),
            Component::Normal(part) => components.push(PathBuf::from(part)),
            Component::CurDir => {}
            Component::ParentDir => return None,
        }
    }

    Some(components)
}

fn normalize_lexical(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() && !normalized.has_root() {
                    normalized.push(component.as_os_str());
                }
            }
            Component::Normal(part) => normalized.push(part),
        }
    }

    normalized
}

fn report(reporter: &mut dyn Reporter, event: OutputEvent) -> Result<()> {
    reporter
        .report(event)
        .map_err(|source| Error::Output { source })
}

#[cfg(test)]
mod tests;
