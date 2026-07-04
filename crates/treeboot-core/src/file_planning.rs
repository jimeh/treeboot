use std::fs::{self, Metadata};
use std::path::{Path, PathBuf};

use crate::file_actions::{
    FileAction, MetadataPolicy, MetadataTarget, PlannedFileOperationActions,
};
use crate::file_system::{
    conflict, file_sync_changed, maybe_metadata, metadata, metadata_drifted, preserved_source_link,
    raw_source_path, relative_path,
};
use crate::path_filter::{
    PathIgnoreRules, PathIncludeRules, invalid_include_pattern, subtree_contains_included,
};
use crate::{
    ActionPlan, Error, FileOperationKind, PlannedFileOperation, PlannedFileStatus, Result,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct FilePlanningOptions {
    pub(crate) strict: bool,
    pub(crate) force: bool,
}

#[derive(Debug, Clone, Copy)]
struct CopyEntry<'a> {
    source_path: &'a Path,
    target_path: &'a Path,
    source: &'a Path,
    target: &'a Path,
    /// Whether this entry passes the include gate, either directly or through
    /// an included ancestor. Always true when no include rules are active.
    included: bool,
}

#[derive(Debug, Clone, Copy)]
struct TreeFilterContext<'a> {
    source_root_path: &'a Path,
    ignore: Option<&'a PathIgnoreRules>,
    include: Option<&'a PathIncludeRules>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SymlinkActionPlan {
    operation: FileOperationKind,
    source: PathBuf,
    target: PathBuf,
    target_path: PathBuf,
    preserved_source_path: Option<PathBuf>,
    link_target: PathBuf,
    final_target: PathBuf,
    target_is_dir: bool,
}

impl SymlinkActionPlan {
    fn into_action(self, replace: bool) -> FileAction {
        FileAction::CreateSymlink {
            operation: self.operation,
            source: self.source,
            target: self.target,
            target_path: self.target_path,
            preserved_source_path: self.preserved_source_path,
            link_target: self.link_target,
            final_target: self.final_target,
            target_is_dir: self.target_is_dir,
            replace,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum TreePlanMode {
    Copy { options: FilePlanningOptions },
    Sync,
}

pub(crate) fn plan_file_operation_group(
    plan: &ActionPlan,
    operation: &PlannedFileOperation,
    options: FilePlanningOptions,
) -> Result<PlannedFileOperationActions> {
    let mut actions = Vec::new();
    plan_operation(plan, operation, options, &mut actions)?;

    Ok(PlannedFileOperationActions {
        operation: operation.operation(),
        source: operation.source().to_path_buf(),
        target: operation.target().to_path_buf(),
        expanded: operation_source_is_directory(plan, operation),
        actions,
    })
}

fn operation_source_is_directory(plan: &ActionPlan, operation: &PlannedFileOperation) -> bool {
    if operation.status() == PlannedFileStatus::SkippedMissingSource {
        return false;
    }

    raw_source_path(plan, operation)
        .symlink_metadata()
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false)
}

fn plan_operation(
    plan: &ActionPlan,
    operation: &PlannedFileOperation,
    options: FilePlanningOptions,
    actions: &mut Vec<FileAction>,
) -> Result<()> {
    if operation.status() == PlannedFileStatus::SkippedMissingSource {
        actions.push(FileAction::Skip {
            operation: operation.operation(),
            target: operation.target().to_path_buf(),
            reason: "missing source".to_owned(),
        });
        return Ok(());
    }

    match operation.operation() {
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
    let metadata = metadata(&source_path, operation.operation())?;
    let ignore_rules = operation_ignore_rules(operation, &source_path)?;
    let include_rules = operation_include_rules(operation, &source_path)?;
    let filter = TreeFilterContext {
        source_root_path: &source_path,
        ignore: ignore_rules.as_ref(),
        include: include_rules.as_ref(),
    };
    // The top-level operation source is never filtered by include or ignore
    // rules; the gates apply only to paths inside a directory source.
    plan_tree_entry(
        plan,
        operation,
        filter,
        CopyEntry {
            source_path: &source_path,
            target_path: operation.target_path(),
            source: operation.source(),
            target: operation.target(),
            included: false,
        },
        &metadata,
        mode,
        actions,
    )
}

fn operation_ignore_rules(
    operation: &PlannedFileOperation,
    source_path: &Path,
) -> Result<Option<PathIgnoreRules>> {
    if operation.ignore().is_empty() {
        return Ok(None);
    }

    PathIgnoreRules::new(source_path, operation.ignore())
        .map(Some)
        .map_err(|source| Error::FileOperationInvalid {
            operation: operation.operation().as_str(),
            message: format!("invalid ignore pattern: {source}"),
        })
}

fn operation_include_rules(
    operation: &PlannedFileOperation,
    source_path: &Path,
) -> Result<Option<PathIncludeRules>> {
    if operation.include().is_empty() {
        return Ok(None);
    }

    for pattern in operation.include() {
        if let Some(issue) = invalid_include_pattern(pattern) {
            return Err(Error::FileOperationInvalid {
                operation: operation.operation().as_str(),
                message: issue.message(pattern),
            });
        }
    }

    PathIncludeRules::new(source_path, operation.include())
        .map(Some)
        .map_err(|source| Error::FileOperationInvalid {
            operation: operation.operation().as_str(),
            message: format!("invalid include pattern: {source}"),
        })
}

fn plan_tree_entry(
    plan: &ActionPlan,
    operation: &PlannedFileOperation,
    filter: TreeFilterContext<'_>,
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
        return plan_tree_directory(plan, operation, filter, entry, mode, actions);
    }

    conflict(
        operation.operation(),
        entry.source_path.to_path_buf(),
        "source file type is unsupported",
    )
}

fn plan_tree_directory(
    plan: &ActionPlan,
    operation: &PlannedFileOperation,
    filter: TreeFilterContext<'_>,
    entry: CopyEntry<'_>,
    mode: TreePlanMode,
    actions: &mut Vec<FileAction>,
) -> Result<()> {
    let mut directory_metadata = None;
    match maybe_metadata(entry.target_path, operation.operation())? {
        Some(metadata) if metadata.file_type().is_symlink() || metadata.is_file() => {
            return conflict(
                operation.operation(),
                entry.target_path.to_path_buf(),
                "target is a file or symlink",
            );
        }
        Some(metadata) if metadata.is_dir() => {
            if let TreePlanMode::Copy { options } = mode
                && options.strict
            {
                return conflict(
                    operation.operation(),
                    entry.target_path.to_path_buf(),
                    "target directory exists",
                );
            }
            if matches!(mode, TreePlanMode::Sync)
                && metadata_drifted(
                    operation.operation(),
                    entry.source_path,
                    entry.target_path,
                    &metadata,
                    MetadataTarget::Directory,
                    MetadataPolicy::from_ignored(operation.ignore_metadata()),
                )?
            {
                directory_metadata = Some(FileAction::RepairMetadata {
                    operation: operation.operation(),
                    source: entry.source.to_path_buf(),
                    target: entry.target.to_path_buf(),
                    source_path: entry.source_path.to_path_buf(),
                    target_path: entry.target_path.to_path_buf(),
                    metadata_policy: MetadataPolicy::from_ignored(operation.ignore_metadata()),
                    target_kind: MetadataTarget::Directory,
                    report: true,
                });
            }
        }
        Some(_) => {
            return conflict(
                operation.operation(),
                entry.target_path.to_path_buf(),
                "target file type is unsupported",
            );
        }
        None => {
            actions.push(FileAction::CreateDirectory {
                operation: operation.operation(),
                source: entry.source.to_path_buf(),
                target: entry.target.to_path_buf(),
                target_path: entry.target_path.to_path_buf(),
            });
            directory_metadata = Some(FileAction::RepairMetadata {
                operation: operation.operation(),
                source: entry.source.to_path_buf(),
                target: entry.target.to_path_buf(),
                source_path: entry.source_path.to_path_buf(),
                target_path: entry.target_path.to_path_buf(),
                metadata_policy: MetadataPolicy::from_ignored(operation.ignore_metadata()),
                target_kind: MetadataTarget::Directory,
                report: false,
            });
        }
    }

    plan_tree_directory_children(plan, operation, filter, entry, mode, actions)?;

    if let Some(action) = directory_metadata {
        actions.push(action);
    }

    Ok(())
}

fn plan_ignored_tree_directory(
    plan: &ActionPlan,
    operation: &PlannedFileOperation,
    filter: TreeFilterContext<'_>,
    entry: CopyEntry<'_>,
    mode: TreePlanMode,
    actions: &mut Vec<FileAction>,
) -> Result<()> {
    plan_tree_directory_children(plan, operation, filter, entry, mode, actions)
}

fn plan_tree_directory_children(
    plan: &ActionPlan,
    operation: &PlannedFileOperation,
    filter: TreeFilterContext<'_>,
    entry: CopyEntry<'_>,
    mode: TreePlanMode,
    actions: &mut Vec<FileAction>,
) -> Result<()> {
    let ancestor_included = entry.included;
    for child in fs::read_dir(entry.source_path).map_err(|source| Error::FileOperationIo {
        operation: operation.operation().as_str(),
        path: entry.source_path.to_path_buf(),
        source,
    })? {
        let child = child.map_err(|source| Error::FileOperationIo {
            operation: operation.operation().as_str(),
            path: entry.source_path.to_path_buf(),
            source,
        })?;
        let child_source_path = child.path();
        let child_target_path = entry.target_path.join(child.file_name());
        let child_source = entry.source.join(child.file_name());
        let child_target = entry.target.join(child.file_name());
        let child_metadata = metadata(&child_source_path, operation.operation())?;
        let child_included =
            ancestor_included || included_source_entry(filter, &child_source_path, &child_metadata);

        if ignored_source_entry(
            filter.source_root_path,
            &child_source_path,
            &child_metadata,
            filter.ignore,
        ) {
            // Ignored directories are only traversed for re-included
            // descendants, and only when the include gate could still pass
            // somewhere underneath.
            if child_metadata.is_dir()
                && filter
                    .ignore
                    .map(PathIgnoreRules::has_negation)
                    .unwrap_or(false)
                && (child_included || include_viable_dir(filter, &child_source_path))
            {
                plan_ignored_tree_directory(
                    plan,
                    operation,
                    filter,
                    CopyEntry {
                        source_path: &child_source_path,
                        target_path: &child_target_path,
                        source: &child_source,
                        target: &child_target,
                        included: child_included,
                    },
                    mode,
                    actions,
                )?;
            }
            continue;
        }

        // Include gate: non-included files are skipped; non-included
        // directories only get target actions when their subtree contains an
        // included entry, and are pruned when no descendant can match.
        if filter.include.is_some()
            && !child_included
            && (!child_metadata.is_dir()
                || !included_descendants_possible(filter, &child_source_path))
        {
            continue;
        }

        plan_tree_entry(
            plan,
            operation,
            filter,
            CopyEntry {
                source_path: &child_source_path,
                target_path: &child_target_path,
                source: &child_source,
                target: &child_target,
                included: child_included,
            },
            &child_metadata,
            mode,
            actions,
        )?;
    }

    if matches!(mode, TreePlanMode::Sync) && operation.delete().unwrap_or(false) {
        // Only recursive delete planning needs the ignored-preserved flag.
        // Include cannot be combined with sync delete, so delete planning
        // stays include-unaware.
        let _ = plan_sync_deletes(operation, entry, filter.ignore, actions)?;
    }

    Ok(())
}

fn ignored_source_entry(
    source_root_path: &Path,
    source_path: &Path,
    metadata: &Metadata,
    ignore_rules: Option<&PathIgnoreRules>,
) -> bool {
    ignore_rules
        .zip(source_path.strip_prefix(source_root_path).ok())
        .is_some_and(|(rules, relative)| rules.is_ignored(relative, metadata.is_dir()))
}

fn included_source_entry(
    filter: TreeFilterContext<'_>,
    source_path: &Path,
    metadata: &Metadata,
) -> bool {
    match filter.include {
        None => true,
        Some(include) => source_path
            .strip_prefix(filter.source_root_path)
            .is_ok_and(|relative| include.is_included(relative, metadata.is_dir())),
    }
}

fn include_viable_dir(filter: TreeFilterContext<'_>, source_path: &Path) -> bool {
    let Some(include) = filter.include else {
        return true;
    };
    let Ok(relative) = source_path.strip_prefix(filter.source_root_path) else {
        return true;
    };

    include.dir_may_contain_matches(relative)
}

fn included_descendants_possible(filter: TreeFilterContext<'_>, source_path: &Path) -> bool {
    let Some(include) = filter.include else {
        return true;
    };

    include_viable_dir(filter, source_path)
        && subtree_contains_included(filter.source_root_path, source_path, include, filter.ignore)
}

fn plan_tree_file(
    operation: &PlannedFileOperation,
    entry: CopyEntry<'_>,
    mode: TreePlanMode,
    actions: &mut Vec<FileAction>,
) -> Result<()> {
    match maybe_metadata(entry.target_path, operation.operation())? {
        Some(metadata) if metadata.is_dir() => conflict(
            operation.operation(),
            entry.target_path.to_path_buf(),
            "target is a directory",
        ),
        Some(metadata) => match mode {
            TreePlanMode::Copy { options } if options.strict => conflict(
                operation.operation(),
                entry.target_path.to_path_buf(),
                "target exists",
            ),
            TreePlanMode::Copy { options } if options.force => {
                actions.push(FileAction::CopyFile {
                    operation: operation.operation(),
                    source: entry.source.to_path_buf(),
                    target: entry.target.to_path_buf(),
                    source_path: entry.source_path.to_path_buf(),
                    target_path: entry.target_path.to_path_buf(),
                    metadata_policy: MetadataPolicy::from_ignored(operation.ignore_metadata()),
                    replace: true,
                });
                Ok(())
            }
            TreePlanMode::Copy { .. } => {
                actions.push(FileAction::Skip {
                    operation: operation.operation(),
                    target: entry.target.to_path_buf(),
                    reason: "target exists".to_owned(),
                });
                Ok(())
            }
            TreePlanMode::Sync if !metadata.is_file() && !metadata.file_type().is_symlink() => {
                conflict(
                    operation.operation(),
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
                    operation: operation.operation(),
                    source: entry.source.to_path_buf(),
                    target: entry.target.to_path_buf(),
                    source_path: entry.source_path.to_path_buf(),
                    target_path: entry.target_path.to_path_buf(),
                    metadata_policy: MetadataPolicy::from_ignored(operation.ignore_metadata()),
                    replace: true,
                });
                Ok(())
            }
            TreePlanMode::Sync
                if metadata_drifted(
                    operation.operation(),
                    entry.source_path,
                    entry.target_path,
                    &metadata,
                    MetadataTarget::File,
                    MetadataPolicy::from_ignored(operation.ignore_metadata()),
                )? =>
            {
                actions.push(FileAction::RepairMetadata {
                    operation: operation.operation(),
                    source: entry.source.to_path_buf(),
                    target: entry.target.to_path_buf(),
                    source_path: entry.source_path.to_path_buf(),
                    target_path: entry.target_path.to_path_buf(),
                    metadata_policy: MetadataPolicy::from_ignored(operation.ignore_metadata()),
                    target_kind: MetadataTarget::File,
                    report: true,
                });
                Ok(())
            }
            TreePlanMode::Sync => Ok(()),
        },
        None => {
            actions.push(FileAction::CopyFile {
                operation: operation.operation(),
                source: entry.source.to_path_buf(),
                target: entry.target.to_path_buf(),
                source_path: entry.source_path.to_path_buf(),
                target_path: entry.target_path.to_path_buf(),
                metadata_policy: MetadataPolicy::from_ignored(operation.ignore_metadata()),
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
        operation.operation(),
        entry.source_path,
        entry.target_path,
    )?;
    let symlink_plan = SymlinkActionPlan {
        operation: operation.operation(),
        source: entry.source.to_path_buf(),
        target: entry.target.to_path_buf(),
        target_path: entry.target_path.to_path_buf(),
        preserved_source_path: Some(entry.source_path.to_path_buf()),
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
                actions.push(plan.into_action(true));
            }
            Ok(())
        }
        Some(_) => {
            actions.push(plan.into_action(true));
            Ok(())
        }
        None => {
            actions.push(plan.into_action(false));
            Ok(())
        }
    }
}

fn plan_symlink(
    operation: &PlannedFileOperation,
    options: FilePlanningOptions,
    actions: &mut Vec<FileAction>,
) -> Result<()> {
    let target_parent = operation
        .target_path()
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let link_target = relative_path(target_parent, operation.source_path())
        .unwrap_or_else(|| operation.source_path().to_path_buf());

    plan_symlink_action(
        SymlinkActionPlan {
            operation: operation.operation(),
            source: operation.source().to_path_buf(),
            target: operation.target().to_path_buf(),
            target_path: operation.target_path().to_path_buf(),
            preserved_source_path: None,
            link_target,
            final_target: operation.source_path().to_path_buf(),
            target_is_dir: operation.source_path().is_dir(),
        },
        options,
        actions,
    )
}

fn plan_symlink_action(
    plan: SymlinkActionPlan,
    options: FilePlanningOptions,
    actions: &mut Vec<FileAction>,
) -> Result<()> {
    match maybe_metadata(&plan.target_path, plan.operation)? {
        Some(metadata) if metadata.is_dir() => {
            conflict(plan.operation, plan.target_path, "target is a directory")
        }
        Some(_) if options.strict => conflict(plan.operation, plan.target_path, "target exists"),
        Some(_) if options.force => {
            actions.push(plan.into_action(true));
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
            actions.push(plan.into_action(false));
            Ok(())
        }
    }
}

fn plan_sync_deletes(
    operation: &PlannedFileOperation,
    entry: CopyEntry<'_>,
    ignore_rules: Option<&PathIgnoreRules>,
    actions: &mut Vec<FileAction>,
) -> Result<bool> {
    let Some(target_metadata) = maybe_metadata(entry.target_path, operation.operation())? else {
        return Ok(false);
    };
    if !target_metadata.is_dir() {
        return Ok(false);
    }

    let mut preserves_ignored = false;
    for child in fs::read_dir(entry.target_path).map_err(|source| Error::FileOperationIo {
        operation: operation.operation().as_str(),
        path: entry.target_path.to_path_buf(),
        source,
    })? {
        let child = child.map_err(|source| Error::FileOperationIo {
            operation: operation.operation().as_str(),
            path: entry.target_path.to_path_buf(),
            source,
        })?;
        let child_target_path = child.path();
        let child_source_path = entry.source_path.join(child.file_name());
        let child_source = entry.source.join(child.file_name());
        let child_target = entry.target.join(child.file_name());
        let child_target_metadata = metadata(&child_target_path, operation.operation())?;
        if ignored_target_entry(
            operation,
            &child_target_path,
            &child_target_metadata,
            ignore_rules,
        ) {
            preserves_ignored = true;
            if child_target_metadata.is_dir()
                && ignore_rules
                    .map(PathIgnoreRules::has_negation)
                    .unwrap_or(false)
            {
                let _ = plan_sync_deletes(
                    operation,
                    CopyEntry {
                        source_path: &child_source_path,
                        target_path: &child_target_path,
                        source: &child_source,
                        target: &child_target,
                        included: entry.included,
                    },
                    ignore_rules,
                    actions,
                )?;
            }
            continue;
        }
        if maybe_metadata(&child_source_path, operation.operation())?.is_none() {
            if child_target_metadata.is_dir() && ignore_rules.is_some() {
                let mut child_actions = Vec::new();
                let child_preserves_ignored = plan_sync_deletes(
                    operation,
                    CopyEntry {
                        source_path: &child_source_path,
                        target_path: &child_target_path,
                        source: &child_source,
                        target: &child_target,
                        included: entry.included,
                    },
                    ignore_rules,
                    &mut child_actions,
                )?;
                if child_preserves_ignored {
                    preserves_ignored = true;
                    actions.extend(child_actions);
                    continue;
                }
            }
            actions.push(FileAction::Delete {
                target: child_target,
                target_path: child_target_path,
            });
        }
    }

    Ok(preserves_ignored)
}

fn ignored_target_entry(
    operation: &PlannedFileOperation,
    target_path: &Path,
    metadata: &Metadata,
    ignore_rules: Option<&PathIgnoreRules>,
) -> bool {
    ignore_rules
        .zip(target_path.strip_prefix(operation.target_path()).ok())
        .is_some_and(|(rules, relative)| rules.is_ignored(relative, metadata.is_dir()))
}

#[cfg(test)]
mod tests;
