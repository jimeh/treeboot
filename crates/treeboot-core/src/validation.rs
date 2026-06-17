use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use crate::{
    CommandKind, CommandOperation, Config, ConfigRuntimeOptions, Error, FileOperation,
    FileOperationKind, Result, RunContext, SourceSpan, SymlinkMode, SyncCompare,
};

/// Options that affect declarative run planning.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RunPlanOptions {
    /// Rejects sync operations and other strict-mode conflicts.
    pub strict: bool,
    /// Allows file operation sources outside the root checkout.
    pub dangerously_allow_sources_outside_root: bool,
    /// Allows file operation targets outside the current worktree.
    pub dangerously_allow_targets_outside_worktree: bool,
}

impl From<ConfigRuntimeOptions> for RunPlanOptions {
    fn from(options: ConfigRuntimeOptions) -> Self {
        Self {
            strict: options.strict,
            dangerously_allow_sources_outside_root: options.dangerously_allow_sources_outside_root,
            dangerously_allow_targets_outside_worktree: options
                .dangerously_allow_targets_outside_worktree,
        }
    }
}

/// A validated declarative run plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunPlan {
    /// Runtime context used while building the plan.
    pub context: RunContext,
    /// Config file used for this plan.
    pub config_path: PathBuf,
    /// Planned file operations.
    pub files: Vec<PlannedFileOperation>,
    /// Planned command operations.
    pub commands: Vec<PlannedCommand>,
}

/// A validated file operation ready for execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedFileOperation {
    /// File operation kind.
    pub operation: FileOperationKind,
    /// Declared source path.
    pub source: PathBuf,
    /// Declared target path.
    pub target: PathBuf,
    /// Normalized source path.
    pub source_path: PathBuf,
    /// Normalized target path.
    pub target_path: PathBuf,
    /// Whether a missing source should fail validation.
    pub required: bool,
    /// Sync comparison mode.
    pub compare: Option<SyncCompare>,
    /// Whether sync should delete target-only files.
    pub delete_extra: Option<bool>,
    /// How copy and sync should treat source symlinks.
    pub symlinks: Option<SymlinkMode>,
    /// Whether this operation should execute.
    pub status: PlannedFileStatus,
    /// Source location for the operation declaration.
    pub declaration: SourceSpan,
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedCommand {
    /// Optional display name.
    pub name: Option<String>,
    /// Command invocation.
    pub command: CommandKind,
    /// Declared working directory.
    pub cwd: Option<PathBuf>,
    /// Normalized working directory.
    pub cwd_path: PathBuf,
    /// Extra environment variables for this command.
    pub env: BTreeMap<String, String>,
    /// Whether this command can run in an async batch.
    pub async_command: bool,
    /// Whether a non-zero exit status should be non-fatal.
    pub allow_failure: bool,
    /// Source location for the command declaration.
    pub declaration: SourceSpan,
}

/// Builds a validated declarative run plan.
///
/// This does not apply file operations or execute commands. It normalizes paths
/// that may not exist yet, rejects invalid declarative behavior, and marks
/// optional missing-source file operations as skipped.
///
/// # Errors
///
/// Returns an error if declarative validation fails.
pub fn plan_run_config(
    path: &Path,
    config: &Config,
    context: &RunContext,
    options: RunPlanOptions,
) -> Result<RunPlan> {
    let root_path = normalize_existing(&context.root_path).map_err(|source| {
        invalid_config_error(path, None, format!("failed to resolve root path: {source}"))
    })?;
    let worktree_path = normalize_existing(&context.worktree_path).map_err(|source| {
        invalid_config_error(
            path,
            None,
            format!("failed to resolve worktree path: {source}"),
        )
    })?;

    let target_paths = normalize_target_paths(path, &config.files)?;
    validate_duplicate_targets(path, &config.files, &target_paths)?;
    validate_strict_sync(path, &config.files, options.strict)?;

    let files = plan_file_operations(
        path,
        config,
        options,
        &target_paths,
        root_path.as_path(),
        worktree_path.as_path(),
    )?;
    let commands = plan_commands(path, &config.commands, context, worktree_path.as_path())?;

    Ok(RunPlan {
        context: context.clone(),
        config_path: path.to_path_buf(),
        files,
        commands,
    })
}

fn normalize_target_paths(path: &Path, files: &[FileOperation]) -> Result<Vec<PathBuf>> {
    files
        .iter()
        .map(|operation| {
            normalize_maybe_existing(&operation.target_path).map_err(|source| {
                invalid_config_error(
                    path,
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

fn validate_duplicate_targets(
    path: &Path,
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
                format!("{}: {}", target.display(), operation_summary(operation))
            })
        })
        .collect::<Vec<_>>()
        .join("; ");

    Err(Error::ConfigInvalid {
        path: path.to_path_buf(),
        message: format!("duplicate configured target: {details}"),
    })
}

fn validate_strict_sync(path: &Path, files: &[FileOperation], strict: bool) -> Result<()> {
    if !strict {
        return Ok(());
    }

    if let Some(operation) = files
        .iter()
        .find(|operation| operation.operation == FileOperationKind::Sync)
    {
        return invalid_config(
            path,
            Some(operation.declaration),
            format!(
                "`--strict` cannot be used with sync file operation {}",
                operation_summary(operation)
            ),
        );
    }

    Ok(())
}

fn plan_file_operations(
    path: &Path,
    config: &Config,
    options: RunPlanOptions,
    target_paths: &[PathBuf],
    root_path: &Path,
    worktree_path: &Path,
) -> Result<Vec<PlannedFileOperation>> {
    let mut planned = Vec::with_capacity(config.files.len());

    for (operation, target_path) in config.files.iter().zip(target_paths) {
        validate_target_boundary(path, options, operation, target_path, worktree_path)?;

        let source_path = normalize_maybe_existing(&operation.source_path).map_err(|source| {
            invalid_config_error(
                path,
                Some(operation.declaration),
                format!(
                    "failed to resolve source {}: {source}",
                    operation.source.display()
                ),
            )
        })?;
        validate_source_boundary(path, options, operation, &source_path, root_path)?;

        let status = match source_exists(path, operation, source_path.as_path())? {
            true => {
                if matches!(
                    operation.operation,
                    FileOperationKind::Copy | FileOperationKind::Sync
                ) {
                    validate_source_symlinks(path, operation, source_path.as_path(), root_path)?;
                }

                PlannedFileStatus::Ready
            }
            false if operation.required => {
                return invalid_config(
                    path,
                    Some(operation.declaration),
                    format!(
                        "required source does not exist for {}",
                        operation_summary(operation)
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
            delete_extra: operation.delete_extra,
            symlinks: operation.symlinks,
            status,
            declaration: operation.declaration,
        });
    }

    Ok(planned)
}

fn validate_target_boundary(
    path: &Path,
    options: RunPlanOptions,
    operation: &FileOperation,
    target_path: &Path,
    worktree_path: &Path,
) -> Result<()> {
    if options.dangerously_allow_targets_outside_worktree {
        return Ok(());
    }

    if !is_within(target_path, worktree_path) {
        return invalid_config(
            path,
            Some(operation.declaration),
            format!(
                "target resolves outside worktree for {}",
                operation_summary(operation)
            ),
        );
    }

    Ok(())
}

fn validate_source_boundary(
    path: &Path,
    options: RunPlanOptions,
    operation: &FileOperation,
    source_path: &Path,
    root_path: &Path,
) -> Result<()> {
    if options.dangerously_allow_sources_outside_root {
        return Ok(());
    }

    if !is_within(source_path, root_path) {
        return invalid_config(
            path,
            Some(operation.declaration),
            format!(
                "source resolves outside root for {}",
                operation_summary(operation)
            ),
        );
    }

    Ok(())
}

fn plan_commands(
    path: &Path,
    commands: &[CommandOperation],
    context: &RunContext,
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
            async_command: command.async_command,
            allow_failure: command.allow_failure,
            declaration: command.declaration,
        });
    }

    Ok(planned)
}

fn validate_source_symlinks(
    path: &Path,
    operation: &FileOperation,
    source_path: &Path,
    root_path: &Path,
) -> Result<()> {
    validate_source_symlink_path(path, operation, source_path, root_path)
}

fn validate_source_symlink_path(
    config_path: &Path,
    operation: &FileOperation,
    path: &Path,
    root_path: &Path,
) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path).map_err(|source| {
        invalid_config_error(
            config_path,
            Some(operation.declaration),
            format!(
                "failed to inspect source {}: {source}",
                operation.source.display()
            ),
        )
    })?;

    if metadata.file_type().is_symlink() {
        let target = normalize_existing(path).map_err(|source| {
            invalid_config_error(
                config_path,
                Some(operation.declaration),
                format!(
                    "failed to resolve source symlink {}: {source}",
                    path.display()
                ),
            )
        })?;

        if !is_within(&target, root_path) {
            return invalid_config(
                config_path,
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
        invalid_config_error(
            config_path,
            Some(operation.declaration),
            format!(
                "failed to inspect source directory {}: {source}",
                path.display()
            ),
        )
    })? {
        let entry = entry.map_err(|source| {
            invalid_config_error(
                config_path,
                Some(operation.declaration),
                format!(
                    "failed to inspect source directory {}: {source}",
                    path.display()
                ),
            )
        })?;
        validate_source_symlink_path(config_path, operation, &entry.path(), root_path)?;
    }

    Ok(())
}

fn source_exists(path: &Path, operation: &FileOperation, source_path: &Path) -> Result<bool> {
    match std::fs::symlink_metadata(source_path) {
        Ok(_) => Ok(true),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(source) => Err(invalid_config_error(
            path,
            Some(operation.declaration),
            format!(
                "failed to inspect source {}: {source}",
                operation.source.display()
            ),
        )),
    }
}

fn operation_summary(operation: &FileOperation) -> String {
    format!(
        "{} {} -> {} at line {}, column {}",
        operation_name(operation.operation),
        operation.source.display(),
        operation.target.display(),
        operation.declaration.line,
        operation.declaration.column
    )
}

fn operation_name(operation: FileOperationKind) -> &'static str {
    match operation {
        FileOperationKind::Copy => "copy",
        FileOperationKind::Symlink => "symlink",
        FileOperationKind::Sync => "sync",
    }
}

fn invalid_config<T>(
    path: &Path,
    span: Option<SourceSpan>,
    message: impl Into<String>,
) -> Result<T> {
    Err(invalid_config_error(path, span, message))
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

    fn context(root_path: &Path, worktree_path: &Path) -> RunContext {
        RunContext {
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
            delete_extra: match operation {
                FileOperationKind::Sync => Some(true),
                FileOperationKind::Copy | FileOperationKind::Symlink => None,
            },
            symlinks: match operation {
                FileOperationKind::Copy | FileOperationKind::Sync => Some(SymlinkMode::Preserve),
                FileOperationKind::Symlink => None,
            },
            declaration: span(),
        }
    }

    fn plan(config: &Config, root: &Path, worktree: &Path) -> Result<RunPlan> {
        plan_run_config(
            Path::new(".treeboot.toml"),
            config,
            &context(root, worktree),
            RunPlanOptions::default(),
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
    fn plan_run_config_should_mark_optional_missing_sources_skipped() {
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
                delete_extra: None,
                symlinks: Some(SymlinkMode::Preserve),
                declaration: span(),
            }],
            commands: Vec::new(),
        };

        let plan = plan_run_config(
            Path::new(".treeboot.toml"),
            &config,
            &context(&root, &worktree),
            RunPlanOptions::default(),
        )
        .expect("optional missing source should plan");

        assert_eq!(
            plan.files[0].status,
            PlannedFileStatus::SkippedMissingSource
        );
    }

    #[test]
    fn plan_run_config_should_build_ready_file_operation() {
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
    fn plan_run_config_should_build_command_metadata() {
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
                async_command: true,
                allow_failure: true,
                declaration: span(),
            }],
        };

        let plan = plan(&config, &root, &worktree).expect("command should plan");

        assert_eq!(
            plan.commands[0].cwd_path,
            std::fs::canonicalize(app_dir).expect("app dir should canonicalize")
        );
        assert!(plan.commands[0].async_command);
        assert!(plan.commands[0].allow_failure);
    }

    #[test]
    fn plan_run_config_should_allow_explicit_boundary_escapes() {
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
                delete_extra: None,
                symlinks: Some(SymlinkMode::Preserve),
                declaration: span(),
            }],
            commands: Vec::new(),
        };

        let plan = plan_run_config(
            Path::new(".treeboot.toml"),
            &config,
            &context(&root, &worktree),
            RunPlanOptions {
                dangerously_allow_sources_outside_root: true,
                dangerously_allow_targets_outside_worktree: true,
                ..RunPlanOptions::default()
            },
        )
        .expect("escaped paths should plan");

        assert_eq!(plan.files[0].status, PlannedFileStatus::Ready);
    }

    #[test]
    fn plan_run_config_should_reject_missing_root_path() {
        let (_root, worktree) = temp_workspace("missing-root");
        let missing_root = worktree.join("missing-root");
        let error = plan_run_config(
            Path::new(".treeboot.toml"),
            &empty_config(),
            &context(&missing_root, &worktree),
            RunPlanOptions::default(),
        )
        .expect_err("missing root should fail");

        assert!(error.to_string().contains("failed to resolve root path"));
    }

    #[test]
    fn plan_run_config_should_reject_missing_worktree_path() {
        let (root, worktree) = temp_workspace("missing-worktree");
        let missing_worktree = worktree.join("missing-worktree");
        let error = plan_run_config(
            Path::new(".treeboot.toml"),
            &empty_config(),
            &context(&root, &missing_worktree),
            RunPlanOptions::default(),
        )
        .expect_err("missing worktree should fail");

        assert!(
            error
                .to_string()
                .contains("failed to resolve worktree path")
        );
    }

    #[test]
    fn plan_run_config_should_allow_strict_when_no_sync_exists() {
        let (root, worktree) = temp_workspace("strict-no-sync");

        let plan = plan_run_config(
            Path::new(".treeboot.toml"),
            &empty_config(),
            &context(&root, &worktree),
            RunPlanOptions {
                strict: true,
                ..RunPlanOptions::default()
            },
        )
        .expect("strict mode should allow configs without sync");

        assert!(plan.files.is_empty());
    }

    #[test]
    fn plan_run_config_should_walk_source_directories() {
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
    fn plan_run_config_should_preserve_sync_options() {
        let (root, worktree) = temp_workspace("sync-options");
        let source_dir = root.join("shared");
        std::fs::create_dir_all(&source_dir).expect("source dir should be created");
        let config = Config {
            options: Default::default(),
            files: vec![file_operation(
                FileOperationKind::Sync,
                &root,
                &worktree,
                "shared",
                "shared",
            )],
            commands: Vec::new(),
        };

        let plan = plan(&config, &root, &worktree).expect("sync should plan");

        assert_eq!(plan.files[0].compare, Some(SyncCompare::Metadata));
        assert_eq!(plan.files[0].delete_extra, Some(true));
        assert_eq!(plan.files[0].symlinks, Some(SymlinkMode::Preserve));
    }

    #[cfg(unix)]
    #[test]
    fn plan_run_config_should_allow_safe_source_symlink() {
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
    fn plan_run_config_should_reject_broken_source_symlink() {
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
    fn plan_run_config_should_default_command_cwd_to_worktree() {
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
                async_command: false,
                allow_failure: false,
                declaration: span(),
            }],
        };

        let plan = plan_run_config(
            Path::new(".treeboot.toml"),
            &config,
            &context(&root, &worktree),
            RunPlanOptions::default(),
        )
        .expect("command should plan");

        assert_eq!(
            plan.commands[0].cwd_path,
            std::fs::canonicalize(worktree).expect("worktree should canonicalize")
        );
    }
}
