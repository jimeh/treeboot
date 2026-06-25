use std::collections::BTreeSet;
use std::fs::{self, File, FileTimes, Metadata};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use crate::{
    ActionPlan, Error, FileOperationKind, MetadataField, OutputEvent, PlannedFileOperation,
    PlannedFileStatus, Reporter, Result, SyncCompare,
};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct FileApplyOptions {
    pub(crate) strict: bool,
    pub(crate) force: bool,
    pub(crate) dry_run: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct FileApplyReport {
    pub(crate) action_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum FileAction {
    CreateDirectory {
        operation: FileOperationKind,
        source: PathBuf,
        target: PathBuf,
        target_path: PathBuf,
    },
    CopyFile {
        operation: FileOperationKind,
        source: PathBuf,
        target: PathBuf,
        source_path: PathBuf,
        target_path: PathBuf,
        metadata_policy: MetadataPolicy,
        replace: bool,
    },
    RepairMetadata {
        operation: FileOperationKind,
        source: PathBuf,
        target: PathBuf,
        source_path: PathBuf,
        target_path: PathBuf,
        metadata_policy: MetadataPolicy,
        target_kind: MetadataTarget,
        report: bool,
    },
    CreateSymlink {
        operation: FileOperationKind,
        source: PathBuf,
        target: PathBuf,
        target_path: PathBuf,
        link_target: PathBuf,
        final_target: PathBuf,
        target_is_dir: bool,
        replace: bool,
    },
    Delete {
        target: PathBuf,
        target_path: PathBuf,
    },
    Skip {
        operation: FileOperationKind,
        target: PathBuf,
        reason: String,
    },
    Warning {
        path: PathBuf,
        reason: String,
    },
}

impl FileAction {
    fn counts(&self) -> bool {
        !matches!(self, Self::RepairMetadata { report: false, .. })
    }
}

#[derive(Debug, Clone, Copy)]
struct CopyEntry<'a> {
    source_path: &'a Path,
    target_path: &'a Path,
    source: &'a Path,
    target: &'a Path,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SymlinkActionPlan {
    operation: FileOperationKind,
    source: PathBuf,
    target: PathBuf,
    target_path: PathBuf,
    link_target: PathBuf,
    final_target: PathBuf,
    target_is_dir: bool,
}

#[derive(Debug, Clone, Copy)]
enum TreePlanMode {
    Copy { options: FileApplyOptions },
    Sync,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MetadataPolicy {
    permissions: bool,
    owner: bool,
    group: bool,
}

impl MetadataPolicy {
    fn from_ignored(fields: &[MetadataField]) -> Self {
        Self {
            permissions: !fields.contains(&MetadataField::Permissions),
            owner: !fields.contains(&MetadataField::Owner),
            group: !fields.contains(&MetadataField::Group),
        }
    }
}

impl Default for MetadataPolicy {
    fn default() -> Self {
        Self {
            permissions: true,
            owner: true,
            group: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MetadataTarget {
    File,
    Directory,
}

pub(crate) fn apply_file_operations(
    plan: &ActionPlan,
    options: FileApplyOptions,
    reporter: &mut dyn Reporter,
) -> Result<FileApplyReport> {
    let mut actions = Vec::new();
    for operation in &plan.files {
        plan_operation(plan, operation, options, &mut actions)?;
    }
    add_symlink_warnings(&mut actions);
    let action_count = actions.iter().filter(|action| action.counts()).count();

    for action in &actions {
        if options.dry_run {
            report_dry_run(action, reporter)?;
        } else {
            apply_action(plan, action, reporter)?;
        }
    }

    Ok(FileApplyReport { action_count })
}

fn plan_operation(
    plan: &ActionPlan,
    operation: &PlannedFileOperation,
    options: FileApplyOptions,
    actions: &mut Vec<FileAction>,
) -> Result<()> {
    if operation.status == PlannedFileStatus::SkippedMissingSource {
        actions.push(FileAction::Skip {
            operation: operation.operation,
            target: operation.target.clone(),
            reason: "missing source".to_owned(),
        });
        return Ok(());
    }

    match operation.operation {
        FileOperationKind::Copy => {
            plan_tree(plan, operation, TreePlanMode::Copy { options }, actions)
        }
        FileOperationKind::Symlink => plan_symlink(operation, options, actions),
        FileOperationKind::Sync => plan_tree(plan, operation, TreePlanMode::Sync, actions),
    }
}

fn plan_tree(
    plan: &ActionPlan,
    operation: &PlannedFileOperation,
    mode: TreePlanMode,
    actions: &mut Vec<FileAction>,
) -> Result<()> {
    let source_path = raw_source_path(plan, operation);
    let metadata = metadata(&source_path, operation.operation)?;
    plan_tree_entry(
        plan,
        operation,
        CopyEntry {
            source_path: &source_path,
            target_path: &operation.target_path,
            source: &operation.source,
            target: &operation.target,
        },
        &metadata,
        mode,
        actions,
    )
}

fn plan_tree_entry(
    plan: &ActionPlan,
    operation: &PlannedFileOperation,
    entry: CopyEntry<'_>,
    source_metadata: &Metadata,
    mode: TreePlanMode,
    actions: &mut Vec<FileAction>,
) -> Result<()> {
    if source_metadata.file_type().is_symlink() {
        return plan_tree_symlink(plan, operation, entry, mode, actions);
    }
    if source_metadata.is_file() {
        return plan_tree_file(operation, entry, mode, actions);
    }
    if source_metadata.is_dir() {
        return plan_tree_directory(plan, operation, entry, mode, actions);
    }

    conflict(
        operation.operation,
        entry.source_path.to_path_buf(),
        "source file type is unsupported",
    )
}

fn plan_tree_directory(
    plan: &ActionPlan,
    operation: &PlannedFileOperation,
    entry: CopyEntry<'_>,
    mode: TreePlanMode,
    actions: &mut Vec<FileAction>,
) -> Result<()> {
    let mut directory_metadata = None;
    match maybe_metadata(entry.target_path, operation.operation)? {
        Some(metadata) if metadata.file_type().is_symlink() || metadata.is_file() => {
            return conflict(
                operation.operation,
                entry.target_path.to_path_buf(),
                "target is a file or symlink",
            );
        }
        Some(metadata) if metadata.is_dir() => {
            if let TreePlanMode::Copy { options } = mode {
                if options.strict {
                    return conflict(
                        operation.operation,
                        entry.target_path.to_path_buf(),
                        "target directory exists",
                    );
                }

                if !options.force {
                    actions.push(FileAction::Skip {
                        operation: operation.operation,
                        target: entry.target.to_path_buf(),
                        reason: "target directory exists".to_owned(),
                    });
                }
            }
            if matches!(mode, TreePlanMode::Sync)
                && metadata_drifted(
                    operation.operation,
                    entry.source_path,
                    entry.target_path,
                    &metadata,
                    MetadataTarget::Directory,
                    MetadataPolicy::from_ignored(&operation.ignore_metadata),
                )?
            {
                directory_metadata = Some(FileAction::RepairMetadata {
                    operation: operation.operation,
                    source: entry.source.to_path_buf(),
                    target: entry.target.to_path_buf(),
                    source_path: entry.source_path.to_path_buf(),
                    target_path: entry.target_path.to_path_buf(),
                    metadata_policy: MetadataPolicy::from_ignored(&operation.ignore_metadata),
                    target_kind: MetadataTarget::Directory,
                    report: true,
                });
            }
        }
        Some(_) => {
            return conflict(
                operation.operation,
                entry.target_path.to_path_buf(),
                "target file type is unsupported",
            );
        }
        None => {
            actions.push(FileAction::CreateDirectory {
                operation: operation.operation,
                source: entry.source.to_path_buf(),
                target: entry.target.to_path_buf(),
                target_path: entry.target_path.to_path_buf(),
            });
            directory_metadata = Some(FileAction::RepairMetadata {
                operation: operation.operation,
                source: entry.source.to_path_buf(),
                target: entry.target.to_path_buf(),
                source_path: entry.source_path.to_path_buf(),
                target_path: entry.target_path.to_path_buf(),
                metadata_policy: MetadataPolicy::from_ignored(&operation.ignore_metadata),
                target_kind: MetadataTarget::Directory,
                report: false,
            });
        }
    }

    for child in fs::read_dir(entry.source_path).map_err(|source| Error::FileOperationIo {
        operation: operation.operation.as_str(),
        path: entry.source_path.to_path_buf(),
        source,
    })? {
        let child = child.map_err(|source| Error::FileOperationIo {
            operation: operation.operation.as_str(),
            path: entry.source_path.to_path_buf(),
            source,
        })?;
        let child_source_path = child.path();
        let child_target_path = entry.target_path.join(child.file_name());
        let child_source = entry.source.join(child.file_name());
        let child_target = entry.target.join(child.file_name());
        let child_metadata = metadata(&child_source_path, operation.operation)?;

        plan_tree_entry(
            plan,
            operation,
            CopyEntry {
                source_path: &child_source_path,
                target_path: &child_target_path,
                source: &child_source,
                target: &child_target,
            },
            &child_metadata,
            mode,
            actions,
        )?;
    }

    if matches!(mode, TreePlanMode::Sync) && operation.delete.unwrap_or(false) {
        plan_sync_deletes(operation, entry, actions)?;
    }

    if let Some(action) = directory_metadata {
        actions.push(action);
    }

    Ok(())
}

fn plan_tree_file(
    operation: &PlannedFileOperation,
    entry: CopyEntry<'_>,
    mode: TreePlanMode,
    actions: &mut Vec<FileAction>,
) -> Result<()> {
    match maybe_metadata(entry.target_path, operation.operation)? {
        Some(metadata) if metadata.is_dir() => conflict(
            operation.operation,
            entry.target_path.to_path_buf(),
            "target is a directory",
        ),
        Some(metadata) => match mode {
            TreePlanMode::Copy { options } if options.strict => conflict(
                operation.operation,
                entry.target_path.to_path_buf(),
                "target exists",
            ),
            TreePlanMode::Copy { options } if options.force => {
                actions.push(FileAction::CopyFile {
                    operation: operation.operation,
                    source: entry.source.to_path_buf(),
                    target: entry.target.to_path_buf(),
                    source_path: entry.source_path.to_path_buf(),
                    target_path: entry.target_path.to_path_buf(),
                    metadata_policy: MetadataPolicy::from_ignored(&operation.ignore_metadata),
                    replace: true,
                });
                Ok(())
            }
            TreePlanMode::Copy { .. } => {
                actions.push(FileAction::Skip {
                    operation: operation.operation,
                    target: entry.target.to_path_buf(),
                    reason: "target exists".to_owned(),
                });
                Ok(())
            }
            TreePlanMode::Sync if !metadata.is_file() && !metadata.file_type().is_symlink() => {
                conflict(
                    operation.operation,
                    entry.target_path.to_path_buf(),
                    "target file type is unsupported",
                )
            }
            TreePlanMode::Sync
                if file_sync_changed(
                    operation,
                    entry.source_path,
                    entry.target_path,
                    &metadata,
                )? =>
            {
                actions.push(FileAction::CopyFile {
                    operation: operation.operation,
                    source: entry.source.to_path_buf(),
                    target: entry.target.to_path_buf(),
                    source_path: entry.source_path.to_path_buf(),
                    target_path: entry.target_path.to_path_buf(),
                    metadata_policy: MetadataPolicy::from_ignored(&operation.ignore_metadata),
                    replace: true,
                });
                Ok(())
            }
            TreePlanMode::Sync
                if metadata_drifted(
                    operation.operation,
                    entry.source_path,
                    entry.target_path,
                    &metadata,
                    MetadataTarget::File,
                    MetadataPolicy::from_ignored(&operation.ignore_metadata),
                )? =>
            {
                actions.push(FileAction::RepairMetadata {
                    operation: operation.operation,
                    source: entry.source.to_path_buf(),
                    target: entry.target.to_path_buf(),
                    source_path: entry.source_path.to_path_buf(),
                    target_path: entry.target_path.to_path_buf(),
                    metadata_policy: MetadataPolicy::from_ignored(&operation.ignore_metadata),
                    target_kind: MetadataTarget::File,
                    report: true,
                });
                Ok(())
            }
            TreePlanMode::Sync => Ok(()),
        },
        None => {
            actions.push(FileAction::CopyFile {
                operation: operation.operation,
                source: entry.source.to_path_buf(),
                target: entry.target.to_path_buf(),
                source_path: entry.source_path.to_path_buf(),
                target_path: entry.target_path.to_path_buf(),
                metadata_policy: MetadataPolicy::from_ignored(&operation.ignore_metadata),
                replace: false,
            });
            Ok(())
        }
    }
}

fn plan_tree_symlink(
    plan: &ActionPlan,
    operation: &PlannedFileOperation,
    entry: CopyEntry<'_>,
    mode: TreePlanMode,
    actions: &mut Vec<FileAction>,
) -> Result<()> {
    let (link_target, final_target, target_is_dir) = preserved_source_link(
        plan,
        operation.operation,
        entry.source_path,
        entry.target_path,
    )?;
    let symlink_plan = SymlinkActionPlan {
        operation: operation.operation,
        source: entry.source.to_path_buf(),
        target: entry.target.to_path_buf(),
        target_path: entry.target_path.to_path_buf(),
        link_target,
        final_target,
        target_is_dir,
    };

    match mode {
        TreePlanMode::Copy { options } => plan_symlink_action(symlink_plan, options, actions),
        TreePlanMode::Sync => plan_sync_symlink_action(symlink_plan, actions),
    }
}

fn plan_sync_symlink_action(plan: SymlinkActionPlan, actions: &mut Vec<FileAction>) -> Result<()> {
    match maybe_metadata(&plan.target_path, plan.operation)? {
        Some(metadata) if metadata.is_dir() => {
            conflict(plan.operation, plan.target_path, "target is a directory")
        }
        Some(metadata) if metadata.file_type().is_symlink() => {
            let existing =
                fs::read_link(&plan.target_path).map_err(|source| Error::FileOperationIo {
                    operation: plan.operation.as_str(),
                    path: plan.target_path.clone(),
                    source,
                })?;
            if existing != plan.link_target {
                actions.push(FileAction::CreateSymlink {
                    operation: plan.operation,
                    source: plan.source,
                    target: plan.target,
                    target_path: plan.target_path,
                    link_target: plan.link_target,
                    final_target: plan.final_target,
                    target_is_dir: plan.target_is_dir,
                    replace: true,
                });
            }
            Ok(())
        }
        Some(_) => {
            actions.push(FileAction::CreateSymlink {
                operation: plan.operation,
                source: plan.source,
                target: plan.target,
                target_path: plan.target_path,
                link_target: plan.link_target,
                final_target: plan.final_target,
                target_is_dir: plan.target_is_dir,
                replace: true,
            });
            Ok(())
        }
        None => {
            actions.push(FileAction::CreateSymlink {
                operation: plan.operation,
                source: plan.source,
                target: plan.target,
                target_path: plan.target_path,
                link_target: plan.link_target,
                final_target: plan.final_target,
                target_is_dir: plan.target_is_dir,
                replace: false,
            });
            Ok(())
        }
    }
}

fn plan_symlink(
    operation: &PlannedFileOperation,
    options: FileApplyOptions,
    actions: &mut Vec<FileAction>,
) -> Result<()> {
    let target_parent = operation
        .target_path
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let link_target = relative_path(target_parent, &operation.source_path)
        .unwrap_or_else(|| operation.source_path.clone());

    plan_symlink_action(
        SymlinkActionPlan {
            operation: operation.operation,
            source: operation.source.clone(),
            target: operation.target.clone(),
            target_path: operation.target_path.clone(),
            link_target,
            final_target: operation.source_path.clone(),
            target_is_dir: operation.source_path.is_dir(),
        },
        options,
        actions,
    )
}

fn plan_symlink_action(
    plan: SymlinkActionPlan,
    options: FileApplyOptions,
    actions: &mut Vec<FileAction>,
) -> Result<()> {
    match maybe_metadata(&plan.target_path, plan.operation)? {
        Some(metadata) if metadata.is_dir() => {
            conflict(plan.operation, plan.target_path, "target is a directory")
        }
        Some(_) if options.strict => conflict(plan.operation, plan.target_path, "target exists"),
        Some(_) if options.force => {
            actions.push(FileAction::CreateSymlink {
                operation: plan.operation,
                source: plan.source,
                target: plan.target,
                target_path: plan.target_path,
                link_target: plan.link_target,
                final_target: plan.final_target,
                target_is_dir: plan.target_is_dir,
                replace: true,
            });
            Ok(())
        }
        Some(_) => {
            actions.push(FileAction::Skip {
                operation: plan.operation,
                target: plan.target,
                reason: "target exists".to_owned(),
            });
            Ok(())
        }
        None => {
            actions.push(FileAction::CreateSymlink {
                operation: plan.operation,
                source: plan.source,
                target: plan.target,
                target_path: plan.target_path,
                link_target: plan.link_target,
                final_target: plan.final_target,
                target_is_dir: plan.target_is_dir,
                replace: false,
            });
            Ok(())
        }
    }
}

fn plan_sync_deletes(
    operation: &PlannedFileOperation,
    entry: CopyEntry<'_>,
    actions: &mut Vec<FileAction>,
) -> Result<()> {
    let Some(target_metadata) = maybe_metadata(entry.target_path, operation.operation)? else {
        return Ok(());
    };
    if !target_metadata.is_dir() {
        return Ok(());
    }

    for child in fs::read_dir(entry.target_path).map_err(|source| Error::FileOperationIo {
        operation: operation.operation.as_str(),
        path: entry.target_path.to_path_buf(),
        source,
    })? {
        let child = child.map_err(|source| Error::FileOperationIo {
            operation: operation.operation.as_str(),
            path: entry.target_path.to_path_buf(),
            source,
        })?;
        let child_target_path = child.path();
        let child_source_path = entry.source_path.join(child.file_name());
        if maybe_metadata(&child_source_path, operation.operation)?.is_none() {
            actions.push(FileAction::Delete {
                target: entry.target.join(child.file_name()),
                target_path: child_target_path,
            });
        }
    }

    Ok(())
}

fn file_sync_changed(
    operation: &PlannedFileOperation,
    source_path: &Path,
    target_path: &Path,
    target_metadata: &Metadata,
) -> Result<bool> {
    if target_metadata.file_type().is_symlink() {
        return Ok(true);
    }

    match operation.compare.unwrap_or(SyncCompare::Metadata) {
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
    let source_metadata = metadata(source_path, operation.operation)?;
    if source_metadata.len() != target_metadata.len() {
        return Ok(true);
    }

    let source_modified = source_metadata
        .modified()
        .map_err(|source| Error::FileOperationIo {
            operation: operation.operation.as_str(),
            path: source_path.to_path_buf(),
            source,
        })?;
    let target_modified = target_metadata
        .modified()
        .map_err(|source| Error::FileOperationIo {
            operation: operation.operation.as_str(),
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
    let source_metadata = metadata(source_path, operation.operation)?;
    let target_metadata = metadata(target_path, operation.operation)?;
    if source_metadata.len() != target_metadata.len() {
        return Ok(true);
    }

    let mut source_file = File::open(source_path).map_err(|source| Error::FileOperationIo {
        operation: operation.operation.as_str(),
        path: source_path.to_path_buf(),
        source,
    })?;
    let mut target_file = File::open(target_path).map_err(|source| Error::FileOperationIo {
        operation: operation.operation.as_str(),
        path: target_path.to_path_buf(),
        source,
    })?;
    let mut source_buf = [0; 8192];
    let mut target_buf = [0; 8192];

    loop {
        let source_read =
            source_file
                .read(&mut source_buf)
                .map_err(|source| Error::FileOperationIo {
                    operation: operation.operation.as_str(),
                    path: source_path.to_path_buf(),
                    source,
                })?;
        let target_read =
            target_file
                .read(&mut target_buf)
                .map_err(|source| Error::FileOperationIo {
                    operation: operation.operation.as_str(),
                    path: target_path.to_path_buf(),
                    source,
                })?;

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

fn metadata_drifted(
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

fn add_symlink_warnings(actions: &mut Vec<FileAction>) {
    let created_paths = actions
        .iter()
        .filter_map(|action| match action {
            FileAction::CreateDirectory { target_path, .. }
            | FileAction::CopyFile { target_path, .. }
            | FileAction::CreateSymlink { target_path, .. } => Some(target_path.clone()),
            FileAction::RepairMetadata { .. }
            | FileAction::Delete { .. }
            | FileAction::Skip { .. }
            | FileAction::Warning { .. } => None,
        })
        .collect::<BTreeSet<_>>();
    let warnings = actions
        .iter()
        .filter_map(|action| match action {
            FileAction::CreateSymlink {
                target,
                final_target,
                ..
            } if !final_target.exists() && !created_paths.contains(final_target) => {
                Some(FileAction::Warning {
                    path: target.clone(),
                    reason: "symlink target does not exist".to_owned(),
                })
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    actions.extend(warnings);
}

fn report_dry_run(action: &FileAction, reporter: &mut dyn Reporter) -> Result<()> {
    match action {
        FileAction::CreateDirectory {
            operation,
            source,
            target,
            ..
        }
        | FileAction::CopyFile {
            operation,
            source,
            target,
            ..
        }
        | FileAction::CreateSymlink {
            operation,
            source,
            target,
            ..
        } => report(
            reporter,
            OutputEvent::FileWouldApply {
                operation: *operation,
                source: source.clone(),
                target: target.clone(),
            },
        ),
        FileAction::RepairMetadata {
            source,
            target,
            report: true,
            ..
        } => report(
            reporter,
            OutputEvent::FileMetadataWouldApply {
                source: source.clone(),
                target: target.clone(),
            },
        ),
        FileAction::RepairMetadata { report: false, .. } => Ok(()),
        FileAction::Delete { target, .. } => report(
            reporter,
            OutputEvent::FileWouldDelete {
                path: target.clone(),
            },
        ),
        FileAction::Skip {
            operation,
            target,
            reason,
        } => report(
            reporter,
            OutputEvent::FileWouldSkip {
                operation: *operation,
                target: target.clone(),
                reason: reason.clone(),
            },
        ),
        FileAction::Warning { path, reason } => report(
            reporter,
            OutputEvent::FileWarning {
                path: path.clone(),
                reason: reason.clone(),
            },
        ),
    }
}

fn apply_action(plan: &ActionPlan, action: &FileAction, reporter: &mut dyn Reporter) -> Result<()> {
    match action {
        FileAction::CreateDirectory {
            operation,
            source,
            target,
            target_path,
        } => {
            create_target_dir(*operation, target_path, &plan.context.worktree_path)?;
            report_applied(reporter, *operation, source, target)
        }
        FileAction::CopyFile {
            operation,
            source,
            target,
            source_path,
            target_path,
            metadata_policy,
            replace,
        } => {
            create_parent_dir(*operation, target_path, &plan.context.worktree_path)?;
            if *replace {
                remove_file_checked(*operation, target_path, &plan.context.worktree_path)?;
            }
            copy_file_with_metadata_with_policy(
                *operation,
                source_path,
                target_path,
                &plan.context.root_path,
                &plan.context.worktree_path,
                *metadata_policy,
                Some(reporter),
            )?;
            report_applied(reporter, *operation, source, target)
        }
        FileAction::RepairMetadata {
            operation,
            source,
            target,
            source_path,
            target_path,
            metadata_policy,
            target_kind,
            report: should_report,
        } => {
            apply_metadata(
                *operation,
                source_path,
                target_path,
                *metadata_policy,
                *target_kind,
                Some(reporter),
            )?;
            if *should_report {
                report(
                    reporter,
                    OutputEvent::FileMetadataApplied {
                        source: source.clone(),
                        target: target.clone(),
                    },
                )
            } else {
                Ok(())
            }
        }
        FileAction::CreateSymlink {
            operation,
            source,
            target,
            target_path,
            link_target,
            final_target: _,
            target_is_dir,
            replace,
        } => {
            create_parent_dir(*operation, target_path, &plan.context.worktree_path)?;
            if *replace {
                remove_file_checked(*operation, target_path, &plan.context.worktree_path)?;
            }
            create_symlink(
                *operation,
                link_target,
                *target_is_dir,
                target_path,
                &plan.context.worktree_path,
            )?;
            report_applied(reporter, *operation, source, target)
        }
        FileAction::Delete {
            target,
            target_path,
        } => {
            remove_any(
                FileOperationKind::Sync,
                target_path,
                &plan.context.worktree_path,
            )?;
            report(
                reporter,
                OutputEvent::FileDeleted {
                    path: target.clone(),
                },
            )
        }
        FileAction::Skip {
            operation,
            target,
            reason,
        } => report(
            reporter,
            OutputEvent::FileSkipped {
                operation: *operation,
                target: target.clone(),
                reason: reason.clone(),
            },
        ),
        FileAction::Warning { path, reason } => report(
            reporter,
            OutputEvent::FileWarning {
                path: path.clone(),
                reason: reason.clone(),
            },
        ),
    }
}

fn report_applied(
    reporter: &mut dyn Reporter,
    operation: FileOperationKind,
    source: &Path,
    target: &Path,
) -> Result<()> {
    report(
        reporter,
        OutputEvent::FileApplied {
            operation,
            source: source.to_path_buf(),
            target: target.to_path_buf(),
        },
    )
}

fn create_parent_dir(
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

fn create_target_dir(
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
fn copy_file_with_metadata(
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

fn copy_file_with_metadata_with_policy(
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

fn apply_metadata(
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

fn remove_file_checked(
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

fn remove_any(operation: FileOperationKind, path: &Path, worktree_path: &Path) -> Result<()> {
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

fn create_symlink(
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

fn preserved_source_link(
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
        raw_target.clone()
    } else {
        normalize_lexical(&source_parent.join(&raw_target))
    };
    let target_is_dir = fs::metadata(&resolved_target)
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false);
    let final_target = resolved_target
        .strip_prefix(&plan.context.root_path)
        .map_or(resolved_target.clone(), |relative| {
            plan.context.worktree_path.join(relative)
        });
    let target_parent = target_path.parent().unwrap_or_else(|| Path::new("."));
    let link_target =
        relative_path(target_parent, &final_target).unwrap_or_else(|| raw_target.clone());

    Ok((link_target, final_target, target_is_dir))
}

fn raw_source_path(plan: &ActionPlan, operation: &PlannedFileOperation) -> PathBuf {
    if operation.source.is_absolute() {
        operation.source.clone()
    } else {
        normalize_lexical(&plan.context.root_path.join(&operation.source))
    }
}

fn metadata(path: &Path, operation: FileOperationKind) -> Result<Metadata> {
    fs::symlink_metadata(path).map_err(|source| Error::FileOperationIo {
        operation: operation.as_str(),
        path: path.to_path_buf(),
        source,
    })
}

fn maybe_metadata(path: &Path, operation: FileOperationKind) -> Result<Option<Metadata>> {
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

fn conflict<T>(
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

fn report(reporter: &mut dyn Reporter, event: OutputEvent) -> Result<()> {
    reporter
        .report(event)
        .map_err(|source| Error::Output { source })
}

fn relative_path(from: &Path, to: &Path) -> Option<PathBuf> {
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

#[cfg(test)]
mod tests;
