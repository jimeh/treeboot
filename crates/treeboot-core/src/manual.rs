use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

use crate::config::{
    self, FileOperationSettingsInput, RuntimeOptionOverrides, normalize_file_operation_settings,
};
use crate::context;
use crate::discovery;
use crate::{
    ActionPlan, Error, ExecuteOptions, Executor, FileOperation, FileOperationKind, OutputEvent,
    PlanOrigin, Reporter, Result, RunContext, RunPlanOptions, SourceSpan, SymlinkMode, SyncCompare,
    WorktreeOptions,
};

/// Options for running one manual file operation command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileOperationOptions {
    /// Directory from which the operation starts. Defaults to the process cwd.
    pub cwd: Option<PathBuf>,
    /// Overrides the root checkout used as the source base.
    pub root: Option<PathBuf>,
    /// File operation kind to run.
    pub operation: FileOperationKind,
    /// Source paths resolved from the root checkout.
    pub sources: Vec<PathBuf>,
    /// Optional target path resolved from the current worktree.
    pub target: Option<PathBuf>,
    /// Fails when a source is missing.
    pub required: bool,
    /// How copy and sync should treat source symlinks.
    pub symlinks: Option<SymlinkMode>,
    /// Sync comparison mode.
    pub compare: Option<SyncCompare>,
    /// Whether sync should delete target-only files.
    pub delete: Option<bool>,
    /// Fails on stricter file-operation conflicts.
    pub strict: bool,
    /// Replaces existing file-operation targets where supported.
    pub force: bool,
    /// Prints planned work without changing files.
    pub dry_run: bool,
}

impl Default for FileOperationOptions {
    fn default() -> Self {
        Self {
            cwd: None,
            root: None,
            operation: FileOperationKind::Copy,
            sources: Vec::new(),
            target: None,
            required: false,
            symlinks: None,
            compare: None,
            delete: None,
            strict: false,
            force: false,
            dry_run: false,
        }
    }
}

/// Completed action for a manual file operation invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileOperationAction {
    /// The command started from the root checkout and had no work to do.
    RootWorktreeSkipped,
    /// File operations were planned and applied.
    Applied,
}

/// Result summary for a manual file operation invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileOperationReport {
    /// Runtime context used by the operation.
    pub context: RunContext,
    /// File operation kind that ran.
    pub operation: FileOperationKind,
    /// Completed action.
    pub action: FileOperationAction,
    /// Number of file actions that were applied or reported.
    pub action_count: usize,
}

/// Options for root-relative source completion.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileOperationCompletionOptions {
    /// Directory from which completion starts. Defaults to the process cwd.
    pub cwd: Option<PathBuf>,
    /// Overrides the root checkout used as the completion base.
    pub root: Option<PathBuf>,
    /// Current partial value being completed.
    pub current: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ManualOperationRequest {
    operation: FileOperationKind,
    sources: Vec<PathBuf>,
    target: Option<PathBuf>,
    required: bool,
    symlinks: Option<SymlinkMode>,
    compare: Option<SyncCompare>,
    delete: Option<bool>,
}

/// Runs a manual copy, symlink, or sync file operation.
///
/// # Errors
///
/// Returns an error if context discovery fails, options are invalid,
/// validation rejects the operation, output reporting fails, or applying the
/// file operation fails.
pub fn run_file_operation(
    options: FileOperationOptions,
    reporter: &mut dyn Reporter,
) -> Result<FileOperationReport> {
    let FileOperationOptions {
        cwd,
        root,
        operation,
        sources,
        target,
        required,
        symlinks,
        compare,
        delete,
        strict,
        force,
        dry_run,
    } = options;
    validate_manual_options(operation, &sources, symlinks, compare, delete)?;
    let request = ManualOperationRequest {
        operation,
        sources,
        target,
        required,
        symlinks,
        compare,
        delete,
    };

    let env_options = RuntimeOptionOverrides::from_env()?;
    let pre_config_strict = env_options.pre_config_strict(strict);
    let context = context::resolve(&WorktreeOptions { cwd, root })?;

    if context.root_path == context.worktree_path {
        report(reporter, OutputEvent::RootWorktreeDetected)?;

        if pre_config_strict {
            return Err(Error::RootWorktreeStrict);
        }

        return Ok(FileOperationReport {
            context,
            operation,
            action: FileOperationAction::RootWorktreeSkipped,
            action_count: 0,
        });
    }

    let config_options = match discovery::discover_config(&context.worktree_path, None)? {
        Some(path) => config::load_config(&path, &context)?.options,
        None => Default::default(),
    };
    let plan_options = env_options.resolve(&config_options, strict);
    let operations = manual_operations(request, &context)?;
    let plan = ActionPlan::from_file_operations(
        &context,
        PlanOrigin::Manual { operation },
        &operations,
        RunPlanOptions::from(plan_options),
    )?;
    let report = Executor::new(ExecuteOptions {
        strict: plan_options.strict,
        force,
        dry_run,
        skip_commands: true,
    })
    .execute_files(&plan, reporter)?;
    let context = plan.context;

    Ok(FileOperationReport {
        context,
        operation,
        action: FileOperationAction::Applied,
        action_count: report.file_action_count,
    })
}

/// Returns source completion candidates relative to the resolved root checkout.
///
/// Errors during context resolution or directory scanning are intentionally
/// quiet, making this suitable for shell completion hooks.
#[must_use]
pub fn file_operation_source_candidates(options: FileOperationCompletionOptions) -> Vec<String> {
    let Ok(context) = context::resolve(&WorktreeOptions {
        cwd: options.cwd,
        root: options.root,
    }) else {
        return Vec::new();
    };

    source_candidates(&context.root_path, &options.current)
}

fn validate_manual_options(
    operation: FileOperationKind,
    sources: &[PathBuf],
    symlinks: Option<SymlinkMode>,
    compare: Option<SyncCompare>,
    delete: Option<bool>,
) -> Result<()> {
    if sources.is_empty() {
        return invalid_manual(operation, "at least one source is required");
    }

    normalize_file_operation_settings(
        operation,
        FileOperationSettingsInput {
            compare,
            delete,
            symlinks,
        },
    )
    .map_err(|field| Error::FileOperationInvalid {
        operation: operation.as_str(),
        message: format!(
            "`{}` is only valid for {}",
            field.name(),
            field.allowed_operations()
        ),
    })?;

    Ok(())
}

fn manual_operations(
    request: ManualOperationRequest,
    context: &RunContext,
) -> Result<Vec<FileOperation>> {
    let ManualOperationRequest {
        operation,
        sources,
        target,
        required,
        symlinks,
        compare,
        delete,
    } = request;
    let multiple_sources = sources.len() > 1;
    let settings = normalize_file_operation_settings(
        operation,
        FileOperationSettingsInput {
            compare,
            delete,
            symlinks,
        },
    )
    .map_err(|field| Error::FileOperationInvalid {
        operation: operation.as_str(),
        message: format!(
            "`{}` is only valid for {}",
            field.name(),
            field.allowed_operations()
        ),
    })?;
    sources
        .into_iter()
        .map(|source| {
            let target = manual_target(operation, &source, target.as_deref(), multiple_sources)?;
            Ok(FileOperation {
                operation,
                source_path: resolve_path(&context.root_path, &source),
                target_path: resolve_path(&context.worktree_path, &target),
                source,
                target,
                required,
                compare: settings.compare,
                delete: settings.delete,
                symlinks: settings.symlinks,
                declaration: manual_span(),
            })
        })
        .collect()
}

fn manual_target(
    operation: FileOperationKind,
    source: &Path,
    target: Option<&Path>,
    multiple_sources: bool,
) -> Result<PathBuf> {
    match (target, multiple_sources) {
        (None, _) => Ok(source.to_path_buf()),
        (Some(target), false) => Ok(target.to_path_buf()),
        (Some(target), true) => {
            if source.is_absolute() {
                let Some(name) = source.file_name() else {
                    return invalid_manual(
                        operation,
                        format!("cannot derive target for source {}", source.display()),
                    );
                };
                return Ok(target.join(name));
            }

            Ok(target.join(source))
        }
    }
}

fn source_candidates(root: &Path, current: &Path) -> Vec<String> {
    if current.is_absolute()
        || current.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Vec::new();
    }

    let (search_prefix, needle) = split_candidate(current);
    let search_root = root.join(search_prefix);
    let Ok(entries) = std::fs::read_dir(search_root) else {
        return Vec::new();
    };
    let needle = needle.to_string_lossy();
    let mut candidates = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name();
            let name_lossy = name.to_string_lossy();
            if !name_lossy.starts_with(needle.as_ref()) {
                return None;
            }

            let mut candidate = search_prefix.to_path_buf();
            candidate.push(&name);
            let mut value = candidate.to_string_lossy().into_owned();
            if entry.file_type().ok()?.is_dir() {
                value.push(std::path::MAIN_SEPARATOR);
            }
            Some(value)
        })
        .collect::<Vec<_>>();

    candidates.sort();
    candidates
}

fn split_candidate(path: &Path) -> (&Path, &OsStr) {
    if path.as_os_str().is_empty() {
        return (Path::new(""), OsStr::new(""));
    }

    if has_trailing_separator(path) {
        return (path, OsStr::new(""));
    }

    (
        path.parent().unwrap_or_else(|| Path::new("")),
        path.file_name().unwrap_or_else(|| OsStr::new("")),
    )
}

fn has_trailing_separator(path: &Path) -> bool {
    path.as_os_str().to_string_lossy().ends_with(['/', '\\'])
}

fn resolve_path(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

const fn manual_span() -> SourceSpan {
    SourceSpan {
        start: 0,
        end: 0,
        line: 0,
        column: 0,
    }
}

fn invalid_manual<T>(operation: FileOperationKind, message: impl Into<String>) -> Result<T> {
    Err(Error::FileOperationInvalid {
        operation: operation.as_str(),
        message: message.into(),
    })
}

fn report(reporter: &mut dyn Reporter, event: OutputEvent) -> Result<()> {
    reporter
        .report(event)
        .map_err(|source| Error::Output { source })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::ffi::OsString;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn temp_workspace(name: &str) -> (PathBuf, PathBuf) {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after Unix epoch")
            .as_nanos();
        let base = std::env::temp_dir().join(format!("treeboot-manual-{name}-{id}"));
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

    fn options(operation: FileOperationKind, sources: &[&str]) -> FileOperationOptions {
        FileOperationOptions {
            operation,
            sources: sources.iter().map(PathBuf::from).collect(),
            ..FileOperationOptions::default()
        }
    }

    fn request(options: FileOperationOptions) -> ManualOperationRequest {
        ManualOperationRequest {
            operation: options.operation,
            sources: options.sources,
            target: options.target,
            required: options.required,
            symlinks: options.symlinks,
            compare: options.compare,
            delete: options.delete,
        }
    }

    #[test]
    fn manual_operations_should_map_single_source_to_same_target() {
        let (root, worktree) = temp_workspace("single-default-target");
        let context = context(&root, &worktree);
        let options = options(FileOperationKind::Copy, &[".env"]);
        let request = request(options);
        let operations = manual_operations(request, &context).expect("operation should normalize");

        assert_eq!(operations[0].source, PathBuf::from(".env"));
        assert_eq!(operations[0].target, PathBuf::from(".env"));
        assert_eq!(operations[0].source_path, root.join(".env"));
        assert_eq!(operations[0].target_path, worktree.join(".env"));
    }

    #[test]
    fn manual_operations_should_map_single_source_to_exact_target() {
        let (root, worktree) = temp_workspace("single-exact-target");
        let context = context(&root, &worktree);
        let mut options = options(FileOperationKind::Copy, &[".env"]);
        options.target = Some(PathBuf::from("local/.env"));
        let request = request(options);

        let operations = manual_operations(request, &context).expect("operation should normalize");

        assert_eq!(operations[0].target, PathBuf::from("local/.env"));
        assert_eq!(operations[0].target_path, worktree.join("local/.env"));
    }

    #[test]
    fn manual_operations_should_map_multiple_sources_to_default_targets() {
        let (root, worktree) = temp_workspace("multi-default-target");
        let context = context(&root, &worktree);
        let options = options(FileOperationKind::Copy, &[".env", ".npmrc"]);
        let request = request(options);
        let operations = manual_operations(request, &context).expect("operation should normalize");

        assert_eq!(operations[0].target_path, worktree.join(".env"));
        assert_eq!(operations[1].target_path, worktree.join(".npmrc"));
    }

    #[test]
    fn manual_operations_should_map_multiple_sources_under_target_prefix() {
        let (root, worktree) = temp_workspace("multi-target-prefix");
        let context = context(&root, &worktree);
        let mut options = options(FileOperationKind::Copy, &["a", "nested/c"]);
        options.target = Some(PathBuf::from("local"));
        let request = request(options);

        let operations = manual_operations(request, &context).expect("operation should normalize");

        assert_eq!(operations[0].source_path, root.join("a"));
        assert_eq!(operations[0].target_path, worktree.join("local/a"));
        assert_eq!(operations[1].source_path, root.join("nested/c"));
        assert_eq!(operations[1].target_path, worktree.join("local/nested/c"));
    }

    #[test]
    fn manual_operations_should_map_multiple_absolute_sources_by_name() {
        let (root, worktree) = temp_workspace("multi-absolute-target-prefix");
        let context = context(&root, &worktree);
        let source = root.join("a");
        let mut options = FileOperationOptions {
            operation: FileOperationKind::Copy,
            sources: vec![source.clone()],
            ..FileOperationOptions::default()
        };
        options.sources.push(root.join("b"));
        options.target = Some(PathBuf::from("local"));
        let request = request(options);

        let operations = manual_operations(request, &context).expect("operation should normalize");

        assert_eq!(operations[0].source_path, source);
        assert_eq!(operations[0].target_path, worktree.join("local/a"));
        assert_eq!(operations[1].target_path, worktree.join("local/b"));
    }

    #[test]
    fn manual_target_should_reject_absolute_source_without_file_name() {
        let temp_dir = std::env::temp_dir();
        let root_source = temp_dir
            .ancestors()
            .last()
            .expect("temp dir should have a filesystem root");
        assert!(root_source.is_absolute());
        assert!(root_source.file_name().is_none());

        let error = manual_target(
            FileOperationKind::Copy,
            root_source,
            Some(Path::new("local")),
            true,
        )
        .expect_err("root path should not have a file name");

        assert!(error.to_string().contains("cannot derive target"));
    }

    #[test]
    fn validate_manual_options_should_reject_symlink_mode_for_symlink() {
        let mut options = options(FileOperationKind::Symlink, &["link"]);
        options.symlinks = Some(SymlinkMode::Preserve);

        let error = validate_manual_options(
            options.operation,
            &options.sources,
            options.symlinks,
            options.compare,
            options.delete,
        )
        .expect_err("symlinks should fail");

        assert!(error.to_string().contains("invalid symlink file operation"));
        assert!(error.to_string().contains("only valid for copy and sync"));
    }

    #[test]
    fn validate_manual_options_should_reject_compare_for_copy() {
        let mut options = options(FileOperationKind::Copy, &["file"]);
        options.compare = Some(SyncCompare::Checksum);

        let error = validate_manual_options(
            options.operation,
            &options.sources,
            options.symlinks,
            options.compare,
            options.delete,
        )
        .expect_err("compare should fail");

        assert!(
            error
                .to_string()
                .contains("`compare` is only valid for sync")
        );
    }

    #[test]
    fn validate_manual_options_should_reject_delete_for_copy() {
        let mut options = options(FileOperationKind::Copy, &["file"]);
        options.delete = Some(true);

        let error = validate_manual_options(
            options.operation,
            &options.sources,
            options.symlinks,
            options.compare,
            options.delete,
        )
        .expect_err("delete should fail");

        assert!(
            error
                .to_string()
                .contains("`delete` is only valid for sync")
        );
    }

    #[test]
    fn validate_manual_options_should_reject_compare_for_symlink() {
        let mut options = options(FileOperationKind::Symlink, &["file"]);
        options.compare = Some(SyncCompare::Metadata);

        let error = validate_manual_options(
            options.operation,
            &options.sources,
            options.symlinks,
            options.compare,
            options.delete,
        )
        .expect_err("compare should fail");

        assert!(
            error
                .to_string()
                .contains("`compare` is only valid for sync")
        );
    }

    #[test]
    fn validate_manual_options_should_reject_delete_for_symlink() {
        let mut options = options(FileOperationKind::Symlink, &["file"]);
        options.delete = Some(false);

        let error = validate_manual_options(
            options.operation,
            &options.sources,
            options.symlinks,
            options.compare,
            options.delete,
        )
        .expect_err("delete should fail");

        assert!(
            error
                .to_string()
                .contains("`delete` is only valid for sync")
        );
    }

    #[test]
    fn validate_manual_options_should_reject_empty_sources() {
        let options = options(FileOperationKind::Copy, &[]);
        let error = validate_manual_options(
            options.operation,
            &options.sources,
            options.symlinks,
            options.compare,
            options.delete,
        )
        .expect_err("empty sources should fail");

        assert!(
            error
                .to_string()
                .contains("at least one source is required")
        );
    }

    #[test]
    fn manual_operations_should_preserve_explicit_sync_options() {
        let (root, worktree) = temp_workspace("sync-options");
        let context = context(&root, &worktree);
        let mut options = options(FileOperationKind::Sync, &["shared"]);
        options.compare = Some(SyncCompare::Checksum);
        options.delete = Some(true);
        options.symlinks = Some(SymlinkMode::Preserve);
        let request = request(options);

        let operations = manual_operations(request, &context).expect("operation should normalize");

        assert_eq!(operations[0].compare, Some(SyncCompare::Checksum));
        assert_eq!(operations[0].delete, Some(true));
        assert_eq!(operations[0].symlinks, Some(SymlinkMode::Preserve));
    }

    #[test]
    fn source_candidates_should_list_root_relative_files_and_dirs() {
        let (root, _worktree) = temp_workspace("source-candidates");
        std::fs::write(root.join(".env"), "TOKEN=1\n").expect("file should be written");
        std::fs::create_dir_all(root.join("shared/nested")).expect("dir should be created");

        assert_eq!(
            source_candidates(&root, Path::new("")),
            vec![
                ".env".to_owned(),
                format!("shared{}", std::path::MAIN_SEPARATOR)
            ]
        );
        assert_eq!(
            source_candidates(&root, Path::new("shared/")),
            vec![format!("shared/nested{}", std::path::MAIN_SEPARATOR)]
        );
    }

    #[test]
    fn source_candidates_should_fail_quietly_for_missing_prefix() {
        let (root, _worktree) = temp_workspace("source-candidates-missing");

        assert!(source_candidates(&root, Path::new("missing/")).is_empty());
    }

    #[test]
    fn source_candidates_should_fail_quietly_for_absolute_current_value() {
        let (root, _worktree) = temp_workspace("source-candidates-absolute");

        assert!(source_candidates(&root, Path::new("/tmp")).is_empty());
    }

    #[test]
    fn source_candidates_should_not_escape_root_with_parent_segments() {
        let (root, _worktree) = temp_workspace("source-candidates-parent");
        std::fs::write(root.join("inside"), "ok\n").expect("file should be written");

        assert!(source_candidates(&root, Path::new("../")).is_empty());
        assert!(source_candidates(&root, Path::new("nested/../../")).is_empty());
    }

    #[test]
    fn file_operation_source_candidates_should_fail_quietly_outside_git() {
        let (root, _worktree) = temp_workspace("completion-outside-git");

        assert!(
            file_operation_source_candidates(FileOperationCompletionOptions {
                cwd: Some(root),
                root: None,
                current: PathBuf::new(),
            })
            .is_empty()
        );
    }

    #[test]
    fn manual_validation_error_should_not_look_like_config_error() {
        let (root, worktree) = temp_workspace("manual-error-origin");
        let error = ActionPlan::from_file_operations(
            &context(&root, &worktree),
            PlanOrigin::Manual {
                operation: FileOperationKind::Copy,
            },
            &[FileOperation {
                operation: FileOperationKind::Copy,
                source: PathBuf::from("../outside"),
                target: PathBuf::from("outside"),
                source_path: root.join("../outside"),
                target_path: worktree.join("outside"),
                required: false,
                compare: None,
                delete: None,
                symlinks: Some(SymlinkMode::Preserve),
                declaration: manual_span(),
            }],
            RunPlanOptions::default(),
        )
        .expect_err("outside source should fail");

        assert!(error.to_string().contains("invalid copy file operation"));
        assert!(!error.to_string().contains("invalid config"));
        assert!(!error.to_string().contains("line"));
        assert!(!error.to_string().contains(".treeboot.toml"));
    }

    #[test]
    fn strict_manual_sync_should_fail_before_side_effects() {
        let (root, worktree) = temp_workspace("strict-sync");
        std::fs::create_dir_all(root.join("shared")).expect("source should be created");
        let error = ActionPlan::from_file_operations(
            &context(&root, &worktree),
            PlanOrigin::Manual {
                operation: FileOperationKind::Sync,
            },
            &[FileOperation {
                operation: FileOperationKind::Sync,
                source: PathBuf::from("shared"),
                target: PathBuf::from("shared"),
                source_path: root.join("shared"),
                target_path: worktree.join("shared"),
                required: false,
                compare: Some(SyncCompare::Metadata),
                delete: Some(false),
                symlinks: Some(SymlinkMode::Preserve),
                declaration: manual_span(),
            }],
            RunPlanOptions {
                strict: true,
                ..RunPlanOptions::default()
            },
        )
        .expect_err("strict sync should fail");

        assert!(error.to_string().contains("cannot be used with sync"));
        assert!(!worktree.join("shared").exists());
    }
}
