use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use crate::file_system::{TargetAncestorIssue, inspect_target_ancestors};
use crate::ignore_rules::PathIgnoreRules;
use crate::{
    CommandKind, CommandOperation, Config, ConfigRuntimeOptions, Error, FileOperation,
    FileOperationKind, MetadataField, Result, SourceSpan, SymlinkMode, SyncCompare, Worktree,
};

/// Options that affect declarative run planning.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ActionPlanOptions {
    /// Rejects sync operations and other strict-mode conflicts.
    pub strict: bool,
    /// Allows file operation sources outside the root checkout.
    pub dangerously_allow_sources_outside_root: bool,
    /// Allows file operation targets outside the current worktree.
    pub dangerously_allow_targets_outside_worktree: bool,
}

impl From<ConfigRuntimeOptions> for ActionPlanOptions {
    fn from(options: ConfigRuntimeOptions) -> Self {
        Self {
            strict: options.strict,
            dangerously_allow_sources_outside_root: options.dangerously_allow_sources_outside_root,
            dangerously_allow_targets_outside_worktree: options
                .dangerously_allow_targets_outside_worktree,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) enum FilePlanOrigin<'a> {
    Config(&'a Path),
    Manual { operation: FileOperationKind },
}

/// Source of a validated action plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanOrigin {
    /// Plan was built from a treeboot manifest.
    Manifest {
        /// Manifest path.
        path: PathBuf,
    },
    /// Plan was built from a manual file operation command.
    Manual {
        /// Manual operation kind.
        operation: FileOperationKind,
    },
}

/// A validated set of file operations and commands ready for execution.
///
/// Plans can only be built through validation constructors. Callers may inspect
/// plans through accessor methods, but cannot construct or mutate planned work
/// directly.
///
/// ```compile_fail
/// # use treeboot_core::ActionPlan;
/// # fn cannot_construct() {
/// ActionPlan {
///     context: todo!(),
///     origin: todo!(),
///     config_path: None,
///     files: Vec::new(),
///     commands: Vec::new(),
/// };
/// # }
/// ```
///
/// ```compile_fail
/// # use treeboot_core::ActionPlan;
/// # fn cannot_mutate(plan: &mut ActionPlan) {
/// plan.commands = Vec::new();
/// # }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionPlan {
    /// Runtime context used while building the plan.
    context: Worktree,
    /// Origin of this plan.
    origin: PlanOrigin,
    /// Config file used for this plan, when it came from a manifest.
    config_path: Option<PathBuf>,
    /// Planned file operations.
    files: Vec<PlannedFileOperation>,
    /// Planned command operations.
    commands: Vec<PlannedCommand>,
}

impl ActionPlan {
    /// Returns the runtime context used while building the plan.
    #[must_use]
    pub const fn context(&self) -> &Worktree {
        &self.context
    }

    /// Returns the origin of this plan.
    #[must_use]
    pub const fn origin(&self) -> &PlanOrigin {
        &self.origin
    }

    /// Returns the config file used for this plan, when it came from a manifest.
    #[must_use]
    pub fn config_path(&self) -> Option<&Path> {
        self.config_path.as_deref()
    }

    /// Returns the planned file operations.
    #[must_use]
    pub fn files(&self) -> &[PlannedFileOperation] {
        &self.files
    }

    /// Returns the planned command operations.
    #[must_use]
    pub fn commands(&self) -> &[PlannedCommand] {
        &self.commands
    }

    /// Builds a validated action plan from a parsed treeboot manifest.
    ///
    /// This does not apply file operations or execute commands. It normalizes
    /// paths that may not exist yet, rejects invalid declarative behavior, and
    /// marks optional missing-source file operations as skipped.
    ///
    /// # Errors
    ///
    /// Returns an error if manifest validation fails.
    pub fn from_manifest(
        path: &Path,
        manifest: &Config,
        context: &Worktree,
        options: ActionPlanOptions,
    ) -> Result<Self> {
        let worktree_path = normalize_existing(&context.worktree_path).map_err(|source| {
            invalid_config_error(
                path,
                None,
                format!("failed to resolve worktree path: {source}"),
            )
        })?;
        let files = plan_file_operations(
            FilePlanOrigin::Config(path),
            &manifest.files,
            context,
            options,
        )?;
        let commands = plan_commands(path, &manifest.commands, context, worktree_path.as_path())?;

        Ok(Self {
            context: context.clone(),
            origin: PlanOrigin::Manifest {
                path: path.to_path_buf(),
            },
            config_path: Some(path.to_path_buf()),
            files,
            commands,
        })
    }

    /// Builds a validated action plan from explicit file operations.
    ///
    /// This is intended for manual commands and other callers that already
    /// have a discovered worktree context and operation list.
    ///
    /// # Errors
    ///
    /// Returns an error if file operation validation fails.
    pub fn from_file_operations(
        context: &Worktree,
        origin: PlanOrigin,
        files: &[FileOperation],
        options: ActionPlanOptions,
    ) -> Result<Self> {
        let file_origin = match &origin {
            PlanOrigin::Manifest { path } => FilePlanOrigin::Config(path),
            PlanOrigin::Manual { operation } => FilePlanOrigin::Manual {
                operation: *operation,
            },
        };
        let files = plan_file_operations(file_origin, files, context, options)?;
        let config_path = match &origin {
            PlanOrigin::Manifest { path } => Some(path.clone()),
            PlanOrigin::Manual { .. } => None,
        };

        Ok(Self {
            context: context.clone(),
            origin,
            config_path,
            files,
            commands: Vec::new(),
        })
    }

    #[cfg(test)]
    pub(crate) fn from_parts_unchecked(
        context: Worktree,
        origin: PlanOrigin,
        config_path: Option<PathBuf>,
        files: Vec<PlannedFileOperation>,
        commands: Vec<PlannedCommand>,
    ) -> Self {
        Self {
            context,
            origin,
            config_path,
            files,
            commands,
        }
    }
}

/// A validated file operation ready for execution.
///
/// ```compile_fail
/// # use treeboot_core::PlannedFileOperation;
/// # fn cannot_mutate(operation: &mut PlannedFileOperation) {
/// operation.target_path = std::path::PathBuf::from("outside");
/// # }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedFileOperation {
    /// File operation kind.
    operation: FileOperationKind,
    /// Declared source path.
    source: PathBuf,
    /// Declared target path.
    target: PathBuf,
    /// Normalized source path.
    source_path: PathBuf,
    /// Normalized target path.
    target_path: PathBuf,
    /// Whether a missing source should fail validation.
    required: bool,
    /// Sync comparison mode.
    compare: Option<SyncCompare>,
    /// Whether sync should delete target-only files.
    delete: Option<bool>,
    /// How copy and sync should treat source symlinks.
    symlinks: Option<SymlinkMode>,
    /// Source-relative path patterns ignored by copy and sync.
    ignore: Vec<String>,
    /// Metadata fields ignored by copy and sync.
    ignore_metadata: Vec<MetadataField>,
    /// Whether this operation should execute.
    status: PlannedFileStatus,
    /// Source location for the operation declaration.
    declaration: SourceSpan,
}

impl PlannedFileOperation {
    /// Returns the file operation kind.
    #[must_use]
    pub const fn operation(&self) -> FileOperationKind {
        self.operation
    }

    /// Returns the declared source path.
    #[must_use]
    pub fn source(&self) -> &Path {
        &self.source
    }

    /// Returns the declared target path.
    #[must_use]
    pub fn target(&self) -> &Path {
        &self.target
    }

    /// Returns the normalized source path.
    #[must_use]
    pub fn source_path(&self) -> &Path {
        &self.source_path
    }

    /// Returns the normalized target path.
    #[must_use]
    pub fn target_path(&self) -> &Path {
        &self.target_path
    }

    /// Returns whether a missing source should fail validation.
    #[must_use]
    pub const fn required(&self) -> bool {
        self.required
    }

    /// Returns the sync comparison mode.
    #[must_use]
    pub const fn compare(&self) -> Option<SyncCompare> {
        self.compare
    }

    /// Returns whether sync should delete target-only files.
    #[must_use]
    pub const fn delete(&self) -> Option<bool> {
        self.delete
    }

    /// Returns how copy and sync should treat source symlinks.
    #[must_use]
    pub const fn symlinks(&self) -> Option<SymlinkMode> {
        self.symlinks
    }

    /// Returns source-relative path patterns ignored by copy and sync.
    #[must_use]
    pub fn ignore(&self) -> &[String] {
        &self.ignore
    }

    /// Returns metadata fields ignored by copy and sync.
    #[must_use]
    pub fn ignore_metadata(&self) -> &[MetadataField] {
        &self.ignore_metadata
    }

    /// Returns whether this operation should execute.
    #[must_use]
    pub const fn status(&self) -> PlannedFileStatus {
        self.status
    }

    /// Returns the source location for the operation declaration.
    #[must_use]
    pub const fn declaration(&self) -> SourceSpan {
        self.declaration
    }

    #[cfg(test)]
    pub(crate) fn from_raw_parts_unchecked(parts: PlannedFileOperationParts) -> Self {
        Self {
            operation: parts.operation,
            source: parts.source,
            target: parts.target,
            source_path: parts.source_path,
            target_path: parts.target_path,
            required: parts.required,
            compare: parts.compare,
            delete: parts.delete,
            symlinks: parts.symlinks,
            ignore: parts.ignore,
            ignore_metadata: parts.ignore_metadata,
            status: parts.status,
            declaration: parts.declaration,
        }
    }

    #[cfg(test)]
    pub(crate) const fn with_compare(mut self, compare: Option<SyncCompare>) -> Self {
        self.compare = compare;
        self
    }

    #[cfg(test)]
    pub(crate) const fn with_delete(mut self, delete: Option<bool>) -> Self {
        self.delete = delete;
        self
    }

    #[cfg(test)]
    pub(crate) fn with_ignore(mut self, ignore: Vec<String>) -> Self {
        self.ignore = ignore;
        self
    }

    #[cfg(test)]
    pub(crate) fn with_ignore_metadata(mut self, ignore_metadata: Vec<MetadataField>) -> Self {
        self.ignore_metadata = ignore_metadata;
        self
    }
}

#[cfg(test)]
pub(crate) struct PlannedFileOperationParts {
    pub(crate) operation: FileOperationKind,
    pub(crate) source: PathBuf,
    pub(crate) target: PathBuf,
    pub(crate) source_path: PathBuf,
    pub(crate) target_path: PathBuf,
    pub(crate) required: bool,
    pub(crate) compare: Option<SyncCompare>,
    pub(crate) delete: Option<bool>,
    pub(crate) symlinks: Option<SymlinkMode>,
    pub(crate) ignore: Vec<String>,
    pub(crate) ignore_metadata: Vec<MetadataField>,
    pub(crate) status: PlannedFileStatus,
    pub(crate) declaration: SourceSpan,
}

/// Execution status for a planned file operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlannedFileStatus {
    /// The operation has an existing source and should run.
    Ready,
    /// The operation has an optional missing source and should be skipped.
    SkippedMissingSource,
}

/// A validated command operation ready for execution.
///
/// ```compile_fail
/// # use treeboot_core::PlannedCommand;
/// # fn cannot_mutate(command: &mut PlannedCommand) {
/// command.cwd_path = std::path::PathBuf::from("outside");
/// # }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedCommand {
    /// Optional display name.
    name: Option<String>,
    /// Command invocation.
    command: CommandKind,
    /// Declared working directory.
    cwd: Option<PathBuf>,
    /// Normalized working directory.
    cwd_path: PathBuf,
    /// Extra environment variables for this command.
    env: BTreeMap<String, String>,
    /// Whether a non-zero exit status should be non-fatal.
    allow_failure: bool,
    /// Source location for the command declaration.
    declaration: SourceSpan,
}

impl PlannedCommand {
    /// Returns the optional display name.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Returns the command invocation.
    #[must_use]
    pub const fn command(&self) -> &CommandKind {
        &self.command
    }

    /// Returns the declared working directory.
    #[must_use]
    pub fn cwd(&self) -> Option<&Path> {
        self.cwd.as_deref()
    }

    /// Returns the normalized working directory.
    #[must_use]
    pub fn cwd_path(&self) -> &Path {
        &self.cwd_path
    }

    /// Returns extra environment variables for this command.
    #[must_use]
    pub const fn env(&self) -> &BTreeMap<String, String> {
        &self.env
    }

    /// Returns whether a non-zero exit status should be non-fatal.
    #[must_use]
    pub const fn allow_failure(&self) -> bool {
        self.allow_failure
    }

    /// Returns the source location for the command declaration.
    #[must_use]
    pub const fn declaration(&self) -> SourceSpan {
        self.declaration
    }

    #[cfg(test)]
    pub(crate) fn from_raw_parts_unchecked(parts: PlannedCommandParts) -> Self {
        Self {
            name: parts.name,
            command: parts.command,
            cwd: parts.cwd,
            cwd_path: parts.cwd_path,
            env: parts.env,
            allow_failure: parts.allow_failure,
            declaration: parts.declaration,
        }
    }
}

#[cfg(test)]
#[derive(Clone)]
pub(crate) struct PlannedCommandParts {
    pub(crate) name: Option<String>,
    pub(crate) command: CommandKind,
    pub(crate) cwd: Option<PathBuf>,
    pub(crate) cwd_path: PathBuf,
    pub(crate) env: BTreeMap<String, String>,
    pub(crate) allow_failure: bool,
    pub(crate) declaration: SourceSpan,
}

pub(super) fn plan_file_operations(
    origin: FilePlanOrigin<'_>,
    files: &[FileOperation],
    context: &Worktree,
    options: ActionPlanOptions,
) -> Result<Vec<PlannedFileOperation>> {
    let root_path = normalize_existing(&context.root_path).map_err(|source| {
        file_plan_error(
            origin,
            None,
            format!("failed to resolve root path: {source}"),
        )
    })?;
    let worktree_path = normalize_existing(&context.worktree_path).map_err(|source| {
        file_plan_error(
            origin,
            None,
            format!("failed to resolve worktree path: {source}"),
        )
    })?;

    let target_paths = normalize_target_paths(origin, files, worktree_path.as_path())?;
    validate_target_conflicts(origin, files, &target_paths)?;
    validate_strict_sync(origin, files, options.strict)?;

    build_file_operations(
        origin,
        files,
        options,
        &target_paths,
        root_path.as_path(),
        worktree_path.as_path(),
    )
}

fn normalize_target_paths(
    origin: FilePlanOrigin<'_>,
    files: &[FileOperation],
    worktree_path: &Path,
) -> Result<Vec<PathBuf>> {
    files
        .iter()
        .map(|operation| {
            validate_target_parent_components(origin, operation, worktree_path)?;
            normalize_target_path(&operation.target_path).map_err(|source| {
                file_plan_error(
                    origin,
                    Some(operation.declaration),
                    format!(
                        "failed to resolve target {}: {source}",
                        operation.target.display()
                    ),
                )
            })
        })
        .collect()
}

fn validate_target_parent_components(
    origin: FilePlanOrigin<'_>,
    operation: &FileOperation,
    worktree_path: &Path,
) -> Result<()> {
    let parent = operation.target_path.parent().unwrap_or(worktree_path);
    if parent.strip_prefix(worktree_path).is_err() {
        return Ok(());
    };

    match inspect_target_ancestors(parent, worktree_path, false) {
        Ok(()) | Err(TargetAncestorIssue::OutsideWorktree { .. }) => Ok(()),
        Err(TargetAncestorIssue::Symlink { path }) => invalid_file_plan(
            origin,
            Some(operation.declaration),
            format!(
                "cannot create target for {}; target parent {} is a symlink",
                operation_label(operation),
                path.display()
            ),
        ),
        Err(TargetAncestorIssue::NotDirectory { path }) => invalid_file_plan(
            origin,
            Some(operation.declaration),
            format!(
                "cannot create target for {}; target parent {} is not a directory",
                operation_label(operation),
                path.display()
            ),
        ),
        Err(TargetAncestorIssue::Io { path, source }) => Err(file_plan_error(
            origin,
            Some(operation.declaration),
            format!(
                "failed to inspect target parent {}: {source}",
                path.display()
            ),
        )),
    }
}

fn validate_target_conflicts(
    origin: FilePlanOrigin<'_>,
    files: &[FileOperation],
    target_paths: &[PathBuf],
) -> Result<()> {
    validate_duplicate_targets(origin, files, target_paths)?;
    validate_overlapping_targets(origin, files, target_paths)
}

fn validate_duplicate_targets(
    origin: FilePlanOrigin<'_>,
    files: &[FileOperation],
    target_paths: &[PathBuf],
) -> Result<()> {
    let mut targets: BTreeMap<&Path, Vec<&FileOperation>> = BTreeMap::new();

    for (operation, target_path) in files.iter().zip(target_paths) {
        targets
            .entry(target_path.as_path())
            .or_default()
            .push(operation);
    }

    let duplicates = targets
        .into_iter()
        .filter(|(_, operations)| operations.len() > 1)
        .collect::<Vec<_>>();

    if duplicates.is_empty() {
        return Ok(());
    }

    let details = duplicates
        .iter()
        .flat_map(|(target, operations)| {
            operations.iter().map(move |operation| {
                format!(
                    "{}: {}",
                    target.display(),
                    operation_summary(origin, operation)
                )
            })
        })
        .collect::<Vec<_>>()
        .join("; ");

    let message = match origin {
        FilePlanOrigin::Config(_) => format!("duplicate configured target: {details}"),
        FilePlanOrigin::Manual { .. } => format!("duplicate target: {details}"),
    };

    Err(file_plan_error(origin, None, message))
}

fn validate_overlapping_targets(
    origin: FilePlanOrigin<'_>,
    files: &[FileOperation],
    target_paths: &[PathBuf],
) -> Result<()> {
    let mut overlaps = Vec::new();

    for (index, (operation, target_path)) in files.iter().zip(target_paths).enumerate() {
        for (other_operation, other_target_path) in files.iter().zip(target_paths).skip(index + 1) {
            if target_path == other_target_path {
                continue;
            }

            let Some((ancestor_path, ancestor, descendant_path, descendant)) =
                overlapping_targets(target_path, operation, other_target_path, other_operation)
            else {
                continue;
            };

            overlaps.push(format!(
                "{} contains {}: {}; {}",
                ancestor_path.display(),
                descendant_path.display(),
                operation_summary(origin, ancestor),
                operation_summary(origin, descendant)
            ));
        }
    }

    if overlaps.is_empty() {
        return Ok(());
    }

    let message = match origin {
        FilePlanOrigin::Config(_) => {
            format!("overlapping configured targets: {}", overlaps.join("; "))
        }
        FilePlanOrigin::Manual { .. } => format!("overlapping targets: {}", overlaps.join("; ")),
    };

    Err(file_plan_error(origin, None, message))
}

fn overlapping_targets<'a>(
    target_path: &'a Path,
    operation: &'a FileOperation,
    other_target_path: &'a Path,
    other_operation: &'a FileOperation,
) -> Option<(&'a Path, &'a FileOperation, &'a Path, &'a FileOperation)> {
    if other_target_path.starts_with(target_path) {
        return Some((target_path, operation, other_target_path, other_operation));
    }

    if target_path.starts_with(other_target_path) {
        return Some((other_target_path, other_operation, target_path, operation));
    }

    None
}

fn validate_strict_sync(
    origin: FilePlanOrigin<'_>,
    files: &[FileOperation],
    strict: bool,
) -> Result<()> {
    if !strict {
        return Ok(());
    }

    if let Some(operation) = files
        .iter()
        .find(|operation| operation.operation == FileOperationKind::Sync)
    {
        return invalid_file_plan(
            origin,
            Some(operation.declaration),
            format!(
                "strict mode cannot be used with sync file operation {}",
                operation_summary(origin, operation)
            ),
        );
    }

    Ok(())
}

fn build_file_operations(
    origin: FilePlanOrigin<'_>,
    files: &[FileOperation],
    options: ActionPlanOptions,
    target_paths: &[PathBuf],
    root_path: &Path,
    worktree_path: &Path,
) -> Result<Vec<PlannedFileOperation>> {
    let mut planned = Vec::with_capacity(files.len());

    for (operation, target_path) in files.iter().zip(target_paths) {
        validate_target_boundary(origin, options, operation, target_path, worktree_path)?;

        let source_path = normalize_maybe_existing(&operation.source_path).map_err(|source| {
            file_plan_error(
                origin,
                Some(operation.declaration),
                format!(
                    "failed to resolve source {}: {source}",
                    operation.source.display()
                ),
            )
        })?;
        validate_source_boundary(origin, options, operation, &source_path, root_path)?;
        let ignore_rules = operation_ignore_rules(origin, operation, &source_path)?;

        let status = match source_exists(origin, operation, source_path.as_path())? {
            true => {
                if matches!(
                    operation.operation,
                    FileOperationKind::Copy | FileOperationKind::Sync
                ) {
                    validate_source_symlinks(
                        origin,
                        operation,
                        source_path.as_path(),
                        root_path,
                        ignore_rules.as_ref(),
                    )?;
                }

                PlannedFileStatus::Ready
            }
            false if operation.required => {
                return invalid_file_plan(
                    origin,
                    Some(operation.declaration),
                    format!(
                        "required source does not exist for {}",
                        operation_summary(origin, operation)
                    ),
                );
            }
            false => PlannedFileStatus::SkippedMissingSource,
        };

        planned.push(PlannedFileOperation {
            operation: operation.operation,
            source: operation.source.clone(),
            target: operation.target.clone(),
            source_path,
            target_path: target_path.clone(),
            required: operation.required,
            compare: operation.compare,
            delete: operation.delete,
            symlinks: operation.symlinks,
            ignore: operation.ignore.clone(),
            ignore_metadata: operation.ignore_metadata.clone(),
            status,
            declaration: operation.declaration,
        });
    }

    Ok(planned)
}

fn validate_target_boundary(
    origin: FilePlanOrigin<'_>,
    options: ActionPlanOptions,
    operation: &FileOperation,
    target_path: &Path,
    worktree_path: &Path,
) -> Result<()> {
    if options.dangerously_allow_targets_outside_worktree {
        return Ok(());
    }

    if !is_within(target_path, worktree_path) {
        return invalid_file_plan(
            origin,
            Some(operation.declaration),
            format!(
                "target resolves outside worktree for {}",
                operation_summary(origin, operation)
            ),
        );
    }

    Ok(())
}

fn validate_source_boundary(
    origin: FilePlanOrigin<'_>,
    options: ActionPlanOptions,
    operation: &FileOperation,
    source_path: &Path,
    root_path: &Path,
) -> Result<()> {
    if options.dangerously_allow_sources_outside_root {
        return Ok(());
    }

    if !is_within(source_path, root_path) {
        return invalid_file_plan(
            origin,
            Some(operation.declaration),
            format!(
                "source resolves outside root for {}",
                operation_summary(origin, operation)
            ),
        );
    }

    Ok(())
}

fn operation_ignore_rules(
    origin: FilePlanOrigin<'_>,
    operation: &FileOperation,
    source_path: &Path,
) -> Result<Option<PathIgnoreRules>> {
    if !matches!(
        operation.operation,
        FileOperationKind::Copy | FileOperationKind::Sync
    ) || operation.ignore.is_empty()
    {
        return Ok(None);
    }

    PathIgnoreRules::new(source_path, &operation.ignore)
        .map(Some)
        .map_err(|source| {
            file_plan_error(
                origin,
                Some(operation.declaration),
                format!(
                    "invalid ignore pattern for {}: {source}",
                    operation_summary(origin, operation)
                ),
            )
        })
}

fn plan_commands(
    path: &Path,
    commands: &[CommandOperation],
    context: &Worktree,
    worktree_path: &Path,
) -> Result<Vec<PlannedCommand>> {
    let mut planned = Vec::with_capacity(commands.len());

    for command in commands {
        let cwd_path = command
            .cwd_path
            .as_ref()
            .map_or_else(
                || Ok(worktree_path.to_path_buf()),
                |cwd_path| normalize_maybe_existing(cwd_path),
            )
            .map_err(|source| {
                invalid_config_error(
                    path,
                    Some(command.declaration),
                    format!("failed to resolve command cwd: {source}"),
                )
            })?;

        if !is_within(&cwd_path, worktree_path) {
            return invalid_config(
                path,
                Some(command.declaration),
                "command cwd resolves outside worktree",
            );
        }

        for key in command.env.keys() {
            if context.environment.contains_key(key) {
                return invalid_config(
                    path,
                    Some(command.declaration),
                    format!("command env overrides treeboot-owned variable `{key}`"),
                );
            }
        }

        planned.push(PlannedCommand {
            name: command.name.clone(),
            command: command.command.clone(),
            cwd: command.cwd.clone(),
            cwd_path,
            env: command.env.clone(),
            allow_failure: command.allow_failure,
            declaration: command.declaration,
        });
    }

    Ok(planned)
}

fn validate_source_symlinks(
    origin: FilePlanOrigin<'_>,
    operation: &FileOperation,
    source_path: &Path,
    root_path: &Path,
    ignore_rules: Option<&PathIgnoreRules>,
) -> Result<()> {
    validate_source_symlink_path(
        origin,
        operation,
        source_path,
        source_path,
        root_path,
        ignore_rules,
    )
}

fn validate_source_symlink_path(
    origin: FilePlanOrigin<'_>,
    operation: &FileOperation,
    source_root: &Path,
    path: &Path,
    root_path: &Path,
    ignore_rules: Option<&PathIgnoreRules>,
) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path).map_err(|source| {
        file_plan_error(
            origin,
            Some(operation.declaration),
            format!(
                "failed to inspect source {}: {source}",
                operation.source.display()
            ),
        )
    })?;

    if metadata.file_type().is_symlink() {
        let target = normalize_existing(path).map_err(|source| {
            file_plan_error(
                origin,
                Some(operation.declaration),
                format!(
                    "failed to resolve source symlink {}: {source}",
                    path.display()
                ),
            )
        })?;

        if !is_within(&target, root_path) {
            return invalid_file_plan(
                origin,
                Some(operation.declaration),
                format!(
                    "copy or sync source contains unsafe symlink {}",
                    path.display()
                ),
            );
        }

        return Ok(());
    }

    if !metadata.is_dir() {
        return Ok(());
    }

    for entry in std::fs::read_dir(path).map_err(|source| {
        file_plan_error(
            origin,
            Some(operation.declaration),
            format!(
                "failed to inspect source directory {}: {source}",
                path.display()
            ),
        )
    })? {
        let entry = entry.map_err(|source| {
            file_plan_error(
                origin,
                Some(operation.declaration),
                format!(
                    "failed to inspect source directory {}: {source}",
                    path.display()
                ),
            )
        })?;
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path).map_err(|source| {
            file_plan_error(
                origin,
                Some(operation.declaration),
                format!(
                    "failed to inspect source directory {}: {source}",
                    path.display()
                ),
            )
        })?;

        if ignored_source_path(source_root, &path, &metadata, ignore_rules) {
            if metadata.is_dir()
                && ignore_rules
                    .map(PathIgnoreRules::has_negation)
                    .unwrap_or(false)
            {
                validate_source_symlink_path(
                    origin,
                    operation,
                    source_root,
                    &path,
                    root_path,
                    ignore_rules,
                )?;
            }
            continue;
        }

        validate_source_symlink_path(
            origin,
            operation,
            source_root,
            &path,
            root_path,
            ignore_rules,
        )?;
    }

    Ok(())
}

fn ignored_source_path(
    source_root: &Path,
    path: &Path,
    metadata: &std::fs::Metadata,
    ignore_rules: Option<&PathIgnoreRules>,
) -> bool {
    ignore_rules
        .zip(path.strip_prefix(source_root).ok())
        .is_some_and(|(rules, relative)| rules.is_ignored(relative, metadata.is_dir()))
}

fn source_exists(
    origin: FilePlanOrigin<'_>,
    operation: &FileOperation,
    source_path: &Path,
) -> Result<bool> {
    match std::fs::symlink_metadata(source_path) {
        Ok(_) => Ok(true),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(source) => Err(file_plan_error(
            origin,
            Some(operation.declaration),
            format!(
                "failed to inspect source {}: {source}",
                operation.source.display()
            ),
        )),
    }
}

fn operation_summary(origin: FilePlanOrigin<'_>, operation: &FileOperation) -> String {
    let summary = operation_label(operation);

    match origin {
        FilePlanOrigin::Config(_) => format!(
            "{} at line {}, column {}",
            summary, operation.declaration.line, operation.declaration.column
        ),
        FilePlanOrigin::Manual { .. } => summary,
    }
}

fn operation_label(operation: &FileOperation) -> String {
    format!(
        "{} {} -> {}",
        operation.operation,
        operation.source.display(),
        operation.target.display()
    )
}

fn invalid_config<T>(
    path: &Path,
    span: Option<SourceSpan>,
    message: impl Into<String>,
) -> Result<T> {
    Err(invalid_config_error(path, span, message))
}

fn invalid_file_plan<T>(
    origin: FilePlanOrigin<'_>,
    span: Option<SourceSpan>,
    message: impl Into<String>,
) -> Result<T> {
    Err(file_plan_error(origin, span, message))
}

fn invalid_config_error(
    path: &Path,
    span: Option<SourceSpan>,
    message: impl Into<String>,
) -> Error {
    let message = match span {
        Some(span) => format!(
            "{} at line {}, column {}",
            message.into(),
            span.line,
            span.column
        ),
        None => message.into(),
    };

    Error::ConfigInvalid {
        path: path.to_path_buf(),
        message,
    }
}

fn file_plan_error(
    origin: FilePlanOrigin<'_>,
    span: Option<SourceSpan>,
    message: impl Into<String>,
) -> Error {
    match origin {
        FilePlanOrigin::Config(path) => invalid_config_error(path, span, message),
        FilePlanOrigin::Manual { operation } => Error::FileOperationInvalid {
            operation: operation.as_str(),
            message: message.into(),
        },
    }
}

fn normalize_existing(path: &Path) -> std::io::Result<PathBuf> {
    std::fs::canonicalize(path)
}

fn normalize_maybe_existing(path: &Path) -> std::io::Result<PathBuf> {
    match normalize_existing(path) {
        Ok(path) => return Ok(path),
        Err(source) if source.kind() != std::io::ErrorKind::NotFound => {
            return Err(source);
        }
        Err(_) => {}
    }

    let mut missing = Vec::new();
    let mut ancestor = path;

    while !ancestor.exists() {
        if let Some(name) = ancestor.file_name() {
            missing.push(name.to_owned());
        }

        let Some(parent) = ancestor.parent() else {
            break;
        };
        ancestor = parent;
    }

    let mut normalized = if ancestor.exists() {
        normalize_existing(ancestor)?
    } else {
        PathBuf::new()
    };

    for component in missing.iter().rev() {
        normalized.push(component);
    }

    Ok(normalize_lexical(&normalized))
}

fn normalize_target_path(path: &Path) -> std::io::Result<PathBuf> {
    let Some(name) = path.file_name() else {
        return normalize_maybe_existing(path);
    };

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut normalized = normalize_maybe_existing(parent)?;
    normalized.push(name);

    Ok(normalize_lexical(&normalized))
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

fn is_within(path: &Path, boundary: &Path) -> bool {
    path == boundary || path.starts_with(boundary)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::ffi::OsString;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

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
        let base = std::env::temp_dir().join(format!("treeboot-{name}-{id}"));
        let root = base.join("root");
        let worktree = base.join("worktree");

        std::fs::create_dir_all(&root).expect("root should be created");
        std::fs::create_dir_all(&worktree).expect("worktree should be created");

        (root, worktree)
    }

    fn context(root_path: &Path, worktree_path: &Path) -> Worktree {
        Worktree {
            root_path: root_path.to_path_buf(),
            worktree_path: worktree_path.to_path_buf(),
            default_branch: "main".to_owned(),
            environment: BTreeMap::from([(
                "TREEBOOT_ROOT_PATH".to_owned(),
                OsString::from(root_path),
            )]),
        }
    }

    fn empty_config() -> Config {
        Config {
            options: Default::default(),
            files: Vec::new(),
            commands: Vec::new(),
        }
    }

    fn file_operation(
        operation: FileOperationKind,
        root: &Path,
        worktree: &Path,
        source: &str,
        target: &str,
    ) -> FileOperation {
        FileOperation {
            operation,
            source: PathBuf::from(source),
            target: PathBuf::from(target),
            source_path: root.join(source),
            target_path: worktree.join(target),
            required: false,
            compare: match operation {
                FileOperationKind::Sync => Some(SyncCompare::Metadata),
                FileOperationKind::Copy | FileOperationKind::Symlink => None,
            },
            delete: match operation {
                FileOperationKind::Sync => Some(false),
                FileOperationKind::Copy | FileOperationKind::Symlink => None,
            },
            symlinks: match operation {
                FileOperationKind::Copy | FileOperationKind::Sync => Some(SymlinkMode::Preserve),
                FileOperationKind::Symlink => None,
            },
            ignore: Vec::new(),
            ignore_metadata: Vec::new(),
            declaration: span(),
        }
    }

    fn plan(config: &Config, root: &Path, worktree: &Path) -> Result<ActionPlan> {
        ActionPlan::from_manifest(
            Path::new(".treeboot.toml"),
            config,
            &context(root, worktree),
            ActionPlanOptions::default(),
        )
    }

    #[test]
    fn normalize_lexical_should_resolve_parent_components() {
        assert_eq!(
            normalize_lexical(Path::new("/repo/worktree/../outside")),
            PathBuf::from("/repo/outside")
        );
    }

    #[test]
    fn is_within_should_not_match_partial_component_prefixes() {
        assert!(!is_within(
            Path::new("/repo-worktree-other/file"),
            Path::new("/repo-worktree")
        ));
    }

    #[test]
    fn action_plan_from_manifest_should_mark_optional_missing_sources_skipped() {
        let (root, worktree) = temp_workspace("missing-source");
        let config = Config {
            options: Default::default(),
            files: vec![FileOperation {
                operation: FileOperationKind::Copy,
                source: PathBuf::from("missing"),
                target: PathBuf::from("missing"),
                source_path: root.join("missing"),
                target_path: worktree.join("missing"),
                required: false,
                compare: None,
                delete: None,
                symlinks: Some(SymlinkMode::Preserve),
                ignore: Vec::new(),
                ignore_metadata: Vec::new(),
                declaration: span(),
            }],
            commands: Vec::new(),
        };

        let plan = ActionPlan::from_manifest(
            Path::new(".treeboot.toml"),
            &config,
            &context(&root, &worktree),
            ActionPlanOptions::default(),
        )
        .expect("optional missing source should plan");

        assert_eq!(
            plan.files[0].status,
            PlannedFileStatus::SkippedMissingSource
        );
    }

    #[test]
    fn action_plan_from_manifest_should_build_ready_file_operation() {
        let (root, worktree) = temp_workspace("ready-file");
        std::fs::write(root.join(".env"), "TOKEN=1\n").expect("source should be written");
        let config = Config {
            options: Default::default(),
            files: vec![file_operation(
                FileOperationKind::Copy,
                &root,
                &worktree,
                ".env",
                ".env",
            )],
            commands: Vec::new(),
        };

        let plan = plan(&config, &root, &worktree).expect("file should plan");

        assert_eq!(plan.files[0].status, PlannedFileStatus::Ready);
    }

    #[test]
    fn action_plan_from_manifest_should_reject_overlapping_file_targets() {
        let (root, worktree) = temp_workspace("overlapping-targets");
        let mut sync = file_operation(
            FileOperationKind::Sync,
            &root,
            &worktree,
            "shared",
            "shared",
        );
        sync.delete = Some(true);
        let config = Config {
            options: Default::default(),
            files: vec![
                file_operation(
                    FileOperationKind::Copy,
                    &root,
                    &worktree,
                    "child",
                    "shared/child",
                ),
                sync,
            ],
            commands: Vec::new(),
        };

        let error = plan(&config, &root, &worktree).expect_err("overlapping targets should fail");

        assert!(error.to_string().contains("overlapping configured targets"));
        assert!(error.to_string().contains("shared"));
        assert!(error.to_string().contains("shared/child"));
    }

    #[test]
    fn action_plan_from_manual_operations_should_reject_overlapping_targets() {
        let (root, worktree) = temp_workspace("manual-overlapping-targets");
        let mut sync = file_operation(
            FileOperationKind::Sync,
            &root,
            &worktree,
            "shared",
            "shared",
        );
        sync.delete = Some(true);
        let operations = vec![
            sync,
            file_operation(
                FileOperationKind::Sync,
                &root,
                &worktree,
                "shared/nested",
                "shared/nested",
            ),
        ];

        let error = ActionPlan::from_file_operations(
            &context(&root, &worktree),
            PlanOrigin::Manual {
                operation: FileOperationKind::Sync,
            },
            &operations,
            ActionPlanOptions::default(),
        )
        .expect_err("overlapping targets should fail");

        assert!(error.to_string().contains("invalid sync file operation"));
        assert!(error.to_string().contains("overlapping targets"));
    }

    #[test]
    fn action_plan_from_manifest_should_build_command_metadata() {
        let (root, worktree) = temp_workspace("command-metadata");
        let app_dir = worktree.join("app");
        std::fs::create_dir_all(&app_dir).expect("command cwd should be created");
        let config = Config {
            options: Default::default(),
            files: Vec::new(),
            commands: vec![CommandOperation {
                name: Some("Install".to_owned()),
                command: CommandKind::Direct {
                    program: "npm".to_owned(),
                    args: vec!["install".to_owned()],
                },
                cwd: Some(PathBuf::from("app")),
                cwd_path: Some(app_dir.clone()),
                env: BTreeMap::from([("NODE_ENV".to_owned(), "development".to_owned())]),
                allow_failure: true,
                declaration: span(),
            }],
        };

        let plan = plan(&config, &root, &worktree).expect("command should plan");

        assert_eq!(
            plan.commands[0].cwd_path,
            std::fs::canonicalize(app_dir).expect("app dir should canonicalize")
        );
        assert!(plan.commands[0].allow_failure);
    }

    #[test]
    fn action_plan_from_manifest_should_allow_explicit_boundary_escapes() {
        let (root, worktree) = temp_workspace("boundary-escapes");
        let outside_source = root
            .parent()
            .expect("root should have parent")
            .join("outside-source");
        let outside_target = worktree
            .parent()
            .expect("worktree should have parent")
            .join("outside-target");
        std::fs::write(&outside_source, "shared\n").expect("outside source should be written");
        let config = Config {
            options: Default::default(),
            files: vec![FileOperation {
                operation: FileOperationKind::Copy,
                source: outside_source.clone(),
                target: outside_target.clone(),
                source_path: outside_source,
                target_path: outside_target,
                required: false,
                compare: None,
                delete: None,
                symlinks: Some(SymlinkMode::Preserve),
                ignore: Vec::new(),
                ignore_metadata: Vec::new(),
                declaration: span(),
            }],
            commands: Vec::new(),
        };

        let plan = ActionPlan::from_manifest(
            Path::new(".treeboot.toml"),
            &config,
            &context(&root, &worktree),
            ActionPlanOptions {
                dangerously_allow_sources_outside_root: true,
                dangerously_allow_targets_outside_worktree: true,
                ..ActionPlanOptions::default()
            },
        )
        .expect("escaped paths should plan");

        assert_eq!(plan.files[0].status, PlannedFileStatus::Ready);
    }

    #[test]
    fn action_plan_from_manifest_should_allow_missing_target_parents_for_all_file_operations() {
        for operation in [
            FileOperationKind::Copy,
            FileOperationKind::Symlink,
            FileOperationKind::Sync,
        ] {
            let (root, worktree) = temp_workspace(&format!("missing-target-parent-{operation}"));
            std::fs::write(root.join("source"), "value\n").expect("source should be written");
            let config = Config {
                options: Default::default(),
                files: vec![file_operation(
                    operation,
                    &root,
                    &worktree,
                    "source",
                    "nested/config/source",
                )],
                commands: Vec::new(),
            };

            let plan = plan(&config, &root, &worktree)
                .unwrap_or_else(|error| panic!("{operation} should plan: {error}"));

            assert_eq!(plan.files[0].status, PlannedFileStatus::Ready);
        }
    }

    #[cfg(unix)]
    #[test]
    fn action_plan_from_manifest_should_allow_final_symlink_target_to_root_source() {
        let (root, worktree) = temp_workspace("final-symlink-target-to-root");
        let source = root.join("config/master.key");
        let target = worktree.join("config/master.key");
        std::fs::create_dir_all(source.parent().unwrap()).expect("source parent should exist");
        std::fs::create_dir_all(target.parent().unwrap()).expect("target parent should exist");
        std::fs::write(&source, "secret\n").expect("source should be written");
        std::os::unix::fs::symlink(&source, &target).expect("target symlink should be created");
        let config = Config {
            options: Default::default(),
            files: vec![file_operation(
                FileOperationKind::Symlink,
                &root,
                &worktree,
                "config/master.key",
                "config/master.key",
            )],
            commands: Vec::new(),
        };

        let plan = plan(&config, &root, &worktree)
            .expect("final target symlink to root source should plan");

        assert_eq!(plan.files[0].status, PlannedFileStatus::Ready);
        assert_eq!(
            plan.files[0].target_path,
            normalize_lexical(&worktree.join("config/master.key"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn action_plan_from_manifest_should_reject_target_parent_symlink_for_all_file_operations() {
        for operation in [
            FileOperationKind::Copy,
            FileOperationKind::Symlink,
            FileOperationKind::Sync,
        ] {
            let (root, worktree) = temp_workspace(&format!("target-parent-symlink-{operation}"));
            let linked = root.join("config");
            std::fs::create_dir_all(&linked).expect("linked directory should be created");
            std::fs::write(root.join("source"), "value\n").expect("source should be written");
            std::os::unix::fs::symlink(&linked, worktree.join("config"))
                .expect("target parent symlink should be created");
            let config = Config {
                options: Default::default(),
                files: vec![file_operation(
                    operation,
                    &root,
                    &worktree,
                    "source",
                    "config/source",
                )],
                commands: Vec::new(),
            };

            let error = match plan(&config, &root, &worktree) {
                Ok(_) => panic!("{operation} should reject symlink parent"),
                Err(error) => error,
            };
            let message = error.to_string();
            assert!(
                message.contains(&format!("cannot create target for {operation}")),
                "{operation} error should name operation: {message}"
            );
            assert!(
                message.contains("target parent") && message.contains("is a symlink"),
                "{operation} error should describe symlink parent: {message}"
            );
        }
    }

    #[test]
    fn action_plan_from_manifest_should_reject_target_parent_file_for_all_file_operations() {
        for operation in [
            FileOperationKind::Copy,
            FileOperationKind::Symlink,
            FileOperationKind::Sync,
        ] {
            let (root, worktree) = temp_workspace(&format!("target-parent-file-{operation}"));
            std::fs::write(root.join("source"), "value\n").expect("source should be written");
            std::fs::write(worktree.join("config"), "not a directory\n")
                .expect("target parent file should be written");
            let config = Config {
                options: Default::default(),
                files: vec![file_operation(
                    operation,
                    &root,
                    &worktree,
                    "source",
                    "config/source",
                )],
                commands: Vec::new(),
            };

            let error = match plan(&config, &root, &worktree) {
                Ok(_) => panic!("{operation} should reject file parent"),
                Err(error) => error,
            };
            let message = error.to_string();
            assert!(
                message.contains(&format!("cannot create target for {operation}")),
                "{operation} error should name operation: {message}"
            );
            assert!(
                message.contains("target parent") && message.contains("is not a directory"),
                "{operation} error should describe file parent: {message}"
            );
        }
    }

    #[test]
    fn action_plan_from_manifest_should_reject_missing_root_path() {
        let (_root, worktree) = temp_workspace("missing-root");
        let missing_root = worktree.join("missing-root");
        let error = ActionPlan::from_manifest(
            Path::new(".treeboot.toml"),
            &empty_config(),
            &context(&missing_root, &worktree),
            ActionPlanOptions::default(),
        )
        .expect_err("missing root should fail");

        assert!(error.to_string().contains("failed to resolve root path"));
    }

    #[test]
    fn action_plan_from_manifest_should_reject_missing_worktree_path() {
        let (root, worktree) = temp_workspace("missing-worktree");
        let missing_worktree = worktree.join("missing-worktree");
        let error = ActionPlan::from_manifest(
            Path::new(".treeboot.toml"),
            &empty_config(),
            &context(&root, &missing_worktree),
            ActionPlanOptions::default(),
        )
        .expect_err("missing worktree should fail");

        assert!(
            error
                .to_string()
                .contains("failed to resolve worktree path")
        );
    }

    #[test]
    fn action_plan_from_manifest_should_allow_strict_when_no_sync_exists() {
        let (root, worktree) = temp_workspace("strict-no-sync");

        let plan = ActionPlan::from_manifest(
            Path::new(".treeboot.toml"),
            &empty_config(),
            &context(&root, &worktree),
            ActionPlanOptions {
                strict: true,
                ..ActionPlanOptions::default()
            },
        )
        .expect("strict mode should allow configs without sync");

        assert!(plan.files.is_empty());
    }

    #[test]
    fn action_plan_from_manifest_should_walk_source_directories() {
        let (root, worktree) = temp_workspace("source-directory");
        let source_dir = root.join("shared");
        std::fs::create_dir_all(&source_dir).expect("source dir should be created");
        std::fs::write(source_dir.join("config"), "value\n").expect("nested source should exist");
        let config = Config {
            options: Default::default(),
            files: vec![file_operation(
                FileOperationKind::Copy,
                &root,
                &worktree,
                "shared",
                "shared",
            )],
            commands: Vec::new(),
        };

        let plan = plan(&config, &root, &worktree).expect("directory source should plan");

        assert_eq!(plan.files[0].status, PlannedFileStatus::Ready);
    }

    #[test]
    fn action_plan_from_manifest_should_preserve_sync_options() {
        let (root, worktree) = temp_workspace("sync-options");
        let source_dir = root.join("shared");
        std::fs::create_dir_all(&source_dir).expect("source dir should be created");
        let mut operation = file_operation(
            FileOperationKind::Sync,
            &root,
            &worktree,
            "shared",
            "shared",
        );
        operation.delete = Some(true);

        let config = Config {
            options: Default::default(),
            files: vec![operation],
            commands: Vec::new(),
        };

        let plan = plan(&config, &root, &worktree).expect("sync should plan");

        assert_eq!(plan.files[0].compare, Some(SyncCompare::Metadata));
        assert_eq!(plan.files[0].delete, Some(true));
        assert_eq!(plan.files[0].symlinks, Some(SymlinkMode::Preserve));
    }

    #[cfg(unix)]
    #[test]
    fn action_plan_from_manifest_should_allow_safe_source_symlink() {
        let (root, worktree) = temp_workspace("safe-symlink");
        std::fs::write(root.join("source"), "value\n").expect("source should be written");
        std::os::unix::fs::symlink(root.join("source"), root.join("link"))
            .expect("safe source symlink should be created");
        let config = Config {
            options: Default::default(),
            files: vec![file_operation(
                FileOperationKind::Copy,
                &root,
                &worktree,
                "link",
                "link",
            )],
            commands: Vec::new(),
        };

        let plan = plan(&config, &root, &worktree).expect("safe symlink should plan");

        assert_eq!(plan.files[0].status, PlannedFileStatus::Ready);
    }

    #[cfg(unix)]
    #[test]
    fn action_plan_from_manifest_should_reject_broken_source_symlink() {
        let (root, worktree) = temp_workspace("broken-symlink");
        std::os::unix::fs::symlink(root.join("missing"), root.join("link"))
            .expect("broken source symlink should be created");
        let config = Config {
            options: Default::default(),
            files: vec![file_operation(
                FileOperationKind::Copy,
                &root,
                &worktree,
                "link",
                "link",
            )],
            commands: Vec::new(),
        };

        let error = plan(&config, &root, &worktree).expect_err("broken symlink should fail");

        assert!(
            error
                .to_string()
                .contains("failed to resolve source symlink")
        );
    }

    #[test]
    fn action_plan_from_manifest_should_default_command_cwd_to_worktree() {
        let (root, worktree) = temp_workspace("command-cwd");
        let config = Config {
            options: Default::default(),
            files: Vec::new(),
            commands: vec![CommandOperation {
                name: None,
                command: CommandKind::Shell {
                    run: "pwd".to_owned(),
                },
                cwd: None,
                cwd_path: None,
                env: BTreeMap::new(),
                allow_failure: false,
                declaration: span(),
            }],
        };

        let plan = ActionPlan::from_manifest(
            Path::new(".treeboot.toml"),
            &config,
            &context(&root, &worktree),
            ActionPlanOptions::default(),
        )
        .expect("command should plan");

        assert_eq!(
            plan.commands[0].cwd_path,
            std::fs::canonicalize(worktree).expect("worktree should canonicalize")
        );
    }
}
