use std::collections::BTreeSet;
use std::fs::{self, File, FileTimes, Metadata};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use crate::ignore_rules::PathIgnoreRules;
use crate::{
    ActionPlan, Error, FileOperationKind, FileOperationSummary, MetadataField, OutputEvent,
    PlannedFileOperation, PlannedFileStatus, Reporter, Result, SyncCompare,
};

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct FileApplyOptions {
    pub(crate) strict: bool,
    pub(crate) force: bool,
    pub(crate) dry_run: bool,
    pub(crate) verbose: bool,
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
        preserved_source_path: Option<PathBuf>,
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
        !matches!(
            self,
            Self::RepairMetadata { report: false, .. } | Self::Warning { .. }
        )
    }
}

#[derive(Debug, Clone, Copy)]
struct CopyEntry<'a> {
    source_path: &'a Path,
    target_path: &'a Path,
    source: &'a Path,
    target: &'a Path,
}

#[derive(Debug, Clone, Copy)]
struct TreeIgnoreContext<'a> {
    source_root_path: &'a Path,
    rules: Option<&'a PathIgnoreRules>,
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

/// Identifies which side of a checksum comparison produced a read error so the
/// caller can attribute it to the right path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContentInput {
    Source,
    Target,
}

/// A read error from one side of a checksum comparison, tagged with the side
/// it came from. The comparison itself is path-agnostic; the caller resolves
/// `input` back to a concrete path when building the public error.
#[derive(Debug)]
struct ContentReadError {
    input: ContentInput,
    source: io::Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PlannedFileOperationActions {
    operation: FileOperationKind,
    source: PathBuf,
    target: PathBuf,
    expanded: bool,
    actions: Vec<FileAction>,
}

impl PlannedFileOperationActions {
    fn progress_action_count(&self) -> usize {
        self.actions.iter().filter(|action| action.counts()).count()
    }

    fn summary(&self) -> FileOperationSummary {
        summarize_actions(&self.actions, self.expanded)
    }
}

pub(crate) fn apply_file_operations(
    plan: &ActionPlan,
    options: FileApplyOptions,
    reporter: &mut dyn Reporter,
) -> Result<FileApplyReport> {
    let mut groups = Vec::new();
    for operation in plan.files() {
        if !options.verbose {
            report_callback(reporter.file_operation_planning_started(
                operation.operation(),
                operation.source(),
                operation.target(),
            ))?;
        }

        let group = plan_file_operation_group(plan, operation, options)?;

        if !options.verbose {
            report_callback(reporter.file_operation_planning_finished(
                group.operation,
                &group.source,
                &group.target,
                group.progress_action_count(),
            ))?;
        }

        groups.push(group);
    }
    add_symlink_warnings(&mut groups);

    let mut action_count = 0;
    for group in &groups {
        action_count += execute_file_operation_group(plan, group, options, reporter)?;
    }

    Ok(FileApplyReport { action_count })
}

fn plan_file_operation_group(
    plan: &ActionPlan,
    operation: &PlannedFileOperation,
    options: FileApplyOptions,
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

fn execute_file_operation_group(
    plan: &ActionPlan,
    group: &PlannedFileOperationActions,
    options: FileApplyOptions,
    reporter: &mut dyn Reporter,
) -> Result<usize> {
    if group.actions.is_empty() {
        return Ok(0);
    }

    let progress_action_count = group.progress_action_count();
    if !options.verbose {
        report_callback(reporter.file_operation_execution_started(
            group.operation,
            &group.source,
            &group.target,
            progress_action_count,
        ))?;
    }

    for action in &group.actions {
        let progress_action = action.counts();
        if options.dry_run {
            report_dry_run(action, reporter, options.verbose)?;
        } else {
            apply_action(plan, action, reporter, options.verbose)?;
        }

        if !options.verbose && progress_action {
            report_callback(reporter.file_operation_action_advanced(
                group.operation,
                &group.source,
                &group.target,
            ))?;
        }
    }

    if !options.verbose {
        let summary = group.summary();
        if summary.decision_count() > 0 {
            report_callback(reporter.file_operation_finished(
                group.operation,
                &group.source,
                &group.target,
                &summary,
                options.dry_run,
            ))?;
        }
    }

    Ok(progress_action_count)
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
    options: FileApplyOptions,
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
    let ignore_context = TreeIgnoreContext {
        source_root_path: &source_path,
        rules: ignore_rules.as_ref(),
    };
    plan_tree_entry(
        plan,
        operation,
        ignore_context,
        CopyEntry {
            source_path: &source_path,
            target_path: operation.target_path(),
            source: operation.source(),
            target: operation.target(),
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

fn plan_tree_entry(
    plan: &ActionPlan,
    operation: &PlannedFileOperation,
    ignore_context: TreeIgnoreContext<'_>,
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
        return plan_tree_directory(
            plan,
            operation,
            ignore_context.source_root_path,
            entry,
            mode,
            ignore_context.rules,
            actions,
        );
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
    source_root_path: &Path,
    entry: CopyEntry<'_>,
    mode: TreePlanMode,
    ignore_rules: Option<&PathIgnoreRules>,
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
            if let TreePlanMode::Copy { options } = mode {
                if options.strict {
                    return conflict(
                        operation.operation(),
                        entry.target_path.to_path_buf(),
                        "target directory exists",
                    );
                }

                if !options.force {
                    actions.push(FileAction::Skip {
                        operation: operation.operation(),
                        target: entry.target.to_path_buf(),
                        reason: "target directory exists".to_owned(),
                    });
                }
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

    plan_tree_directory_children(
        plan,
        operation,
        source_root_path,
        entry,
        mode,
        ignore_rules,
        actions,
    )?;

    if let Some(action) = directory_metadata {
        actions.push(action);
    }

    Ok(())
}

fn plan_ignored_tree_directory(
    plan: &ActionPlan,
    operation: &PlannedFileOperation,
    source_root_path: &Path,
    entry: CopyEntry<'_>,
    mode: TreePlanMode,
    ignore_rules: Option<&PathIgnoreRules>,
    actions: &mut Vec<FileAction>,
) -> Result<()> {
    plan_tree_directory_children(
        plan,
        operation,
        source_root_path,
        entry,
        mode,
        ignore_rules,
        actions,
    )
}

fn plan_tree_directory_children(
    plan: &ActionPlan,
    operation: &PlannedFileOperation,
    source_root_path: &Path,
    entry: CopyEntry<'_>,
    mode: TreePlanMode,
    ignore_rules: Option<&PathIgnoreRules>,
    actions: &mut Vec<FileAction>,
) -> Result<()> {
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

        if ignored_source_entry(
            source_root_path,
            &child_source_path,
            &child_metadata,
            ignore_rules,
        ) {
            if child_metadata.is_dir()
                && ignore_rules
                    .map(PathIgnoreRules::has_negation)
                    .unwrap_or(false)
            {
                plan_ignored_tree_directory(
                    plan,
                    operation,
                    source_root_path,
                    CopyEntry {
                        source_path: &child_source_path,
                        target_path: &child_target_path,
                        source: &child_source,
                        target: &child_target,
                    },
                    mode,
                    ignore_rules,
                    actions,
                )?;
            }
            continue;
        }

        plan_tree_entry(
            plan,
            operation,
            TreeIgnoreContext {
                source_root_path,
                rules: ignore_rules,
            },
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

    if matches!(mode, TreePlanMode::Sync) && operation.delete().unwrap_or(false) {
        let _ = plan_sync_deletes(operation, entry, ignore_rules, actions)?;
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
    options: FileApplyOptions,
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
    options: FileApplyOptions,
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

fn file_sync_changed(
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
fn reader_contents_changed(
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
fn read_full_chunk(
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

fn add_symlink_warnings(groups: &mut [PlannedFileOperationActions]) {
    let created_paths = groups
        .iter()
        .flat_map(|group| group.actions.iter())
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
    for group in groups {
        let warnings = group
            .actions
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

        group.actions.extend(warnings);
    }
}

fn summarize_actions(actions: &[FileAction], expanded: bool) -> FileOperationSummary {
    let mut summary = FileOperationSummary {
        expanded,
        ..FileOperationSummary::default()
    };

    for action in actions {
        match action {
            FileAction::CreateDirectory { .. }
            | FileAction::CopyFile { .. }
            | FileAction::CreateSymlink { .. } => summary.changed += 1,
            FileAction::RepairMetadata { report: true, .. } => {
                summary.changed += 1;
                summary.metadata_changed += 1;
            }
            FileAction::RepairMetadata { report: false, .. } => {}
            FileAction::Delete { .. } => summary.deleted += 1,
            FileAction::Skip { reason, .. } => {
                summary.skipped += 1;
                if summary.skip_reason.is_none() {
                    summary.skip_reason = Some(reason.clone());
                }
            }
            FileAction::Warning { .. } => summary.warnings += 1,
        }
    }

    summary
}

fn report_dry_run(action: &FileAction, reporter: &mut dyn Reporter, detailed: bool) -> Result<()> {
    if !detailed && !matches!(action, FileAction::Warning { .. }) {
        return Ok(());
    }

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

fn apply_action(
    plan: &ActionPlan,
    action: &FileAction,
    reporter: &mut dyn Reporter,
    detailed: bool,
) -> Result<()> {
    match action {
        FileAction::CreateDirectory {
            operation,
            source,
            target,
            target_path,
        } => {
            with_writable_parent(
                *operation,
                target_path,
                &plan.context().worktree_path,
                || create_target_dir(*operation, target_path, &plan.context().worktree_path),
            )?;
            if detailed {
                report_applied(reporter, *operation, source, target)?;
            }
            Ok(())
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
            with_writable_parent(
                *operation,
                target_path,
                &plan.context().worktree_path,
                || {
                    create_parent_dir(*operation, target_path, &plan.context().worktree_path)?;
                    if *replace {
                        remove_file_checked(
                            *operation,
                            target_path,
                            &plan.context().worktree_path,
                        )?;
                    }
                    copy_file_with_metadata_with_policy(
                        *operation,
                        source_path,
                        target_path,
                        &plan.context().root_path,
                        &plan.context().worktree_path,
                        *metadata_policy,
                        Some(reporter),
                    )
                },
            )?;
            if detailed {
                report_applied(reporter, *operation, source, target)?;
            }
            Ok(())
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
            if detailed && *should_report {
                report(
                    reporter,
                    OutputEvent::FileMetadataApplied {
                        source: source.clone(),
                        target: target.clone(),
                    },
                )?;
            }
            Ok(())
        }
        FileAction::CreateSymlink {
            operation,
            source,
            target,
            target_path,
            preserved_source_path,
            link_target,
            final_target,
            target_is_dir,
            replace,
        } => {
            if let Some(source_path) = preserved_source_path {
                ensure_preserved_source_symlink_safe(
                    plan,
                    *operation,
                    source_path,
                    target_path,
                    link_target,
                    final_target,
                    *target_is_dir,
                )?;
            }
            with_writable_parent(
                *operation,
                target_path,
                &plan.context().worktree_path,
                || {
                    create_parent_dir(*operation, target_path, &plan.context().worktree_path)?;
                    if *replace {
                        remove_file_checked(
                            *operation,
                            target_path,
                            &plan.context().worktree_path,
                        )?;
                    }
                    create_symlink(
                        *operation,
                        link_target,
                        *target_is_dir,
                        target_path,
                        &plan.context().worktree_path,
                    )
                },
            )?;
            if detailed {
                report_applied(reporter, *operation, source, target)?;
            }
            Ok(())
        }
        FileAction::Delete {
            target,
            target_path,
        } => {
            with_writable_parent(
                FileOperationKind::Sync,
                target_path,
                &plan.context().worktree_path,
                || {
                    remove_any(
                        FileOperationKind::Sync,
                        target_path,
                        &plan.context().worktree_path,
                    )
                },
            )?;
            if detailed {
                report(
                    reporter,
                    OutputEvent::FileDeleted {
                        path: target.clone(),
                    },
                )?;
            }
            Ok(())
        }
        FileAction::Skip {
            operation,
            target,
            reason,
        } => {
            if detailed {
                report(
                    reporter,
                    OutputEvent::FileSkipped {
                        operation: *operation,
                        target: target.clone(),
                        reason: reason.clone(),
                    },
                )?;
            }
            Ok(())
        }
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

fn with_writable_parent<F>(
    operation: FileOperationKind,
    target_path: &Path,
    worktree_path: &Path,
    action: F,
) -> Result<()>
where
    F: FnOnce() -> Result<()>,
{
    let restore = prepare_parent_for_writes(operation, target_path, worktree_path)?;
    let result = action();
    if let Some((path, permissions)) = restore {
        let restore_result =
            fs::set_permissions(&path, permissions).map_err(|source| Error::FileOperationIo {
                operation: operation.as_str(),
                path,
                source,
            });
        if result.is_ok() {
            restore_result?;
        }
    }
    result
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

fn ensure_preserved_source_symlink_safe(
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
        .strip_prefix(&plan.context().root_path)
        .map_or(resolved_target.clone(), |relative| {
            plan.context().worktree_path.join(relative)
        });
    let target_parent = target_path.parent().unwrap_or_else(|| Path::new("."));
    let link_target =
        relative_path(target_parent, &final_target).unwrap_or_else(|| raw_target.clone());

    Ok((link_target, final_target, target_is_dir))
}

fn raw_source_path(plan: &ActionPlan, operation: &PlannedFileOperation) -> PathBuf {
    if operation.source().is_absolute() {
        operation.source().to_path_buf()
    } else {
        normalize_lexical(&plan.context().root_path.join(operation.source()))
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

fn report_callback(result: std::io::Result<()>) -> Result<()> {
    result.map_err(|source| Error::Output { source })
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
