use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

use crate::config::{
    Config, FileOperationSettings, FileOperationSettingsInput, RawMetadataField,
    effective_ignore_patterns, normalize_file_operation_settings,
};
use crate::context;
use crate::glob;
use crate::ignore_rules::PathIgnoreRules;
use crate::paths::{self, UnsupportedPath};
use crate::{
    ActionPlan, EnvironmentInput, Error, ExecuteOptions, Executor, FileOperation,
    FileOperationKind, MetadataField, OutputEvent, PlanOrigin, Reporter, Result, RuntimePolicy,
    SourceSpan, SymlinkMode, SyncCompare, Worktree, WorktreeOptions,
};

/// Options for building manual file operation specs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManualFileOperationOptions {
    /// File operation kind to build.
    pub operation: FileOperationKind,
    /// Source paths resolved from the root checkout.
    pub sources: Vec<PathBuf>,
    /// Optional target path resolved from the current worktree.
    pub target: Option<PathBuf>,
    /// Fails when a source is missing.
    pub required: bool,
    /// Expands glob source patterns into matched sources.
    pub glob: bool,
    /// Allows glob source pattern bases outside the root checkout.
    ///
    /// Expansion never walks directories outside `TREEBOOT_ROOT_PATH` unless
    /// this is enabled. Expanded operations are still subject to per-source
    /// boundary validation when the plan is built.
    pub dangerously_allow_sources_outside_root: bool,
    /// How copy and sync should treat source symlinks.
    pub symlinks: Option<SymlinkMode>,
    /// Sync comparison mode.
    pub compare: Option<SyncCompare>,
    /// Whether sync should delete target-only files.
    pub delete: Option<bool>,
    /// Source-relative path patterns ignored by copy and sync.
    pub ignore: Vec<String>,
    /// Metadata fields ignored by copy and sync.
    pub ignore_metadata: Vec<MetadataField>,
}

impl Default for ManualFileOperationOptions {
    fn default() -> Self {
        Self {
            operation: FileOperationKind::Copy,
            sources: Vec::new(),
            target: None,
            required: false,
            glob: true,
            dangerously_allow_sources_outside_root: false,
            symlinks: None,
            compare: None,
            delete: None,
            ignore: Vec::new(),
            ignore_metadata: Vec::new(),
        }
    }
}

impl ManualFileOperationOptions {
    /// Creates manual copy operation options for the given sources.
    #[must_use]
    pub fn copy(sources: Vec<PathBuf>) -> Self {
        Self::new(FileOperationKind::Copy, sources)
    }

    /// Creates manual symlink operation options for the given sources.
    #[must_use]
    pub fn symlink(sources: Vec<PathBuf>) -> Self {
        Self::new(FileOperationKind::Symlink, sources)
    }

    /// Creates manual sync operation options for the given sources.
    #[must_use]
    pub fn sync(sources: Vec<PathBuf>) -> Self {
        Self::new(FileOperationKind::Sync, sources)
    }

    fn new(operation: FileOperationKind, sources: Vec<PathBuf>) -> Self {
        Self {
            operation,
            sources,
            ..Self::default()
        }
    }
}

impl FileOperation {
    /// Builds normalized manual file operation specs for an action plan.
    ///
    /// This applies the same target derivation and option validation used by
    /// `treeboot copy`, `treeboot symlink`, and `treeboot sync`.
    ///
    /// # Errors
    ///
    /// Returns an error when the manual operation has no sources, when an
    /// option is not valid for the selected operation kind, or when a target
    /// cannot be derived for an absolute source.
    pub fn from_manual_options(
        context: &Worktree,
        options: ManualFileOperationOptions,
    ) -> Result<Vec<Self>> {
        let settings = validate_manual_options(
            options.operation,
            &options.sources,
            options.symlinks,
            options.compare,
            options.delete,
            &options.ignore,
            &options.ignore_metadata,
        )?;
        manual_operations(options, context, settings)
    }
}

/// Options for running one manual file operation command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileOperationOptions {
    /// Directory from which the operation starts. Defaults to the process cwd.
    pub cwd: Option<PathBuf>,
    /// Overrides the root checkout used as the source base.
    pub root: Option<PathBuf>,
    /// Explicit environment input used for compatibility discovery and options.
    pub environment: EnvironmentInput,
    /// File operation kind to run.
    pub operation: FileOperationKind,
    /// Source paths resolved from the root checkout.
    pub sources: Vec<PathBuf>,
    /// Optional target path resolved from the current worktree.
    pub target: Option<PathBuf>,
    /// Fails when a source is missing.
    pub required: bool,
    /// Expands glob source patterns into matched sources.
    pub glob: bool,
    /// How copy and sync should treat source symlinks.
    pub symlinks: Option<SymlinkMode>,
    /// Sync comparison mode.
    pub compare: Option<SyncCompare>,
    /// Whether sync should delete target-only files.
    pub delete: Option<bool>,
    /// Source-relative path patterns ignored by copy and sync.
    pub ignore: Vec<String>,
    /// Metadata fields ignored by copy and sync.
    pub ignore_metadata: Vec<MetadataField>,
    /// Fails on stricter file-operation conflicts.
    pub strict: bool,
    /// Replaces existing file-operation targets where supported.
    pub force: bool,
    /// Prints planned work without changing files.
    pub dry_run: bool,
    /// Prints detailed file-operation actions instead of compact summaries.
    pub verbose: bool,
}

impl Default for FileOperationOptions {
    fn default() -> Self {
        Self {
            cwd: None,
            root: None,
            environment: EnvironmentInput::empty(),
            operation: FileOperationKind::Copy,
            sources: Vec::new(),
            target: None,
            required: false,
            glob: true,
            symlinks: None,
            compare: None,
            delete: None,
            ignore: Vec::new(),
            ignore_metadata: Vec::new(),
            strict: false,
            force: false,
            dry_run: false,
            verbose: false,
        }
    }
}

impl FileOperationOptions {
    /// Creates copy command options for the given sources.
    #[must_use]
    pub fn copy(sources: Vec<PathBuf>) -> Self {
        Self::new(FileOperationKind::Copy, sources)
    }

    /// Creates symlink command options for the given sources.
    #[must_use]
    pub fn symlink(sources: Vec<PathBuf>) -> Self {
        Self::new(FileOperationKind::Symlink, sources)
    }

    /// Creates sync command options for the given sources.
    #[must_use]
    pub fn sync(sources: Vec<PathBuf>) -> Self {
        Self::new(FileOperationKind::Sync, sources)
    }

    fn new(operation: FileOperationKind, sources: Vec<PathBuf>) -> Self {
        Self {
            operation,
            sources,
            ..Self::default()
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
    pub context: Worktree,
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
    /// Explicit environment input used for compatibility discovery.
    pub environment: EnvironmentInput,
    /// Current partial value being completed.
    pub current: PathBuf,
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
        environment,
        operation,
        sources,
        target,
        required,
        glob,
        symlinks,
        compare,
        delete,
        ignore,
        ignore_metadata,
        strict,
        force,
        dry_run,
        verbose,
    } = options;
    let mut manual_options = ManualFileOperationOptions {
        operation,
        sources,
        target,
        required,
        glob,
        dangerously_allow_sources_outside_root: false,
        symlinks,
        compare,
        delete,
        ignore,
        ignore_metadata,
    };

    let runtime_policy = RuntimePolicy::from_environment(&environment, strict)?;
    let pre_config_strict = runtime_policy.pre_config_strict();
    let context = context::resolve(&WorktreeOptions {
        cwd,
        root,
        environment,
    })?;

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

    let config_options = Config::load_discovered(&context, None)?
        .map(|loaded| loaded.config.options)
        .unwrap_or_default();
    let plan_options = runtime_policy.resolve(&config_options);
    manual_options.ignore = effective_ignore_patterns(
        operation,
        plan_options.default_ignore(),
        manual_options.ignore,
    );
    manual_options.dangerously_allow_sources_outside_root = plan_options
        .action_plan_options()
        .dangerously_allow_sources_outside_root;
    let strict = plan_options.strict();
    let operations = FileOperation::from_manual_options(&context, manual_options)?;
    let plan = ActionPlan::from_file_operations(
        &context,
        PlanOrigin::Manual { operation },
        &operations,
        plan_options.into_action_plan_options(),
    )?;
    let report = Executor::new(ExecuteOptions {
        strict,
        force,
        dry_run,
        verbose,
        skip_commands: true,
    })
    .execute_files(&plan, reporter)?;
    let context = plan.context().clone();

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
        environment: options.environment,
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
    ignore: &[String],
    ignore_metadata: &[MetadataField],
) -> Result<FileOperationSettings> {
    if sources.is_empty() {
        return invalid_manual(operation, "at least one source is required");
    }

    let ignore_metadata = ignore_metadata
        .iter()
        .copied()
        .map(RawMetadataField::from)
        .collect();
    normalize_file_operation_settings(
        operation,
        FileOperationSettingsInput {
            compare,
            delete,
            symlinks,
            ignore: ignore.to_vec(),
            ignore_metadata,
        },
    )
    .map_err(|field| Error::FileOperationInvalid {
        operation: operation.as_str(),
        message: format!(
            "`{}` is only valid for {}",
            field.name(),
            field.allowed_operations()
        ),
    })
}

fn manual_operations(
    options: ManualFileOperationOptions,
    context: &Worktree,
    settings: FileOperationSettings,
) -> Result<Vec<FileOperation>> {
    let ManualFileOperationOptions {
        operation,
        sources,
        target,
        required,
        glob,
        dangerously_allow_sources_outside_root,
        ignore,
        ..
    } = options;
    let sources = expand_manual_sources(
        operation,
        sources,
        context,
        &ManualGlobPolicy {
            required,
            glob,
            dangerously_allow_sources_outside_root,
        },
        &ignore,
    )?;
    let multiple_sources = sources.len() > 1;
    let target_explicit = target.is_some();
    sources
        .into_iter()
        .map(|(source, ignore_prefix)| {
            let target = manual_target(operation, &source, target.as_deref(), multiple_sources)?;
            let source_path = resolve_path(
                operation,
                &context.root_path,
                &source,
                &source,
                &target,
                ManualPathRole::Source,
            )?;
            let target_path = resolve_path(
                operation,
                &context.worktree_path,
                &target,
                &source,
                &target,
                ManualPathRole::Target,
            )?;
            Ok(FileOperation {
                operation,
                source_path,
                target_path,
                source,
                target,
                required,
                glob: false,
                target_explicit,
                compare: settings.compare,
                delete: settings.delete,
                symlinks: settings.symlinks,
                ignore: ignore.clone(),
                ignore_metadata: settings.ignore_metadata.clone(),
                ignore_prefix,
                declaration: manual_span(),
            })
        })
        .collect()
}

/// Expands manual glob source arguments into matched source values, exactly
/// as if the shell had expanded the pattern into multiple source arguments.
///
/// Each returned entry pairs a source value with the pattern-base-relative
/// ignore prefix, which is empty for literal sources. Optional patterns with
/// no matches are kept as literal source values so they follow normal
/// missing-source skip semantics.
#[derive(Debug, Clone, Copy)]
struct ManualGlobPolicy {
    required: bool,
    glob: bool,
    dangerously_allow_sources_outside_root: bool,
}

fn expand_manual_sources(
    operation: FileOperationKind,
    sources: Vec<PathBuf>,
    context: &Worktree,
    policy: &ManualGlobPolicy,
    ignore: &[String],
) -> Result<Vec<(PathBuf, PathBuf)>> {
    let mut expanded = Vec::with_capacity(sources.len());

    for source in sources {
        if !policy.glob || !glob::is_glob_source(&source) {
            expanded.push((source, PathBuf::new()));
            continue;
        }

        let split = glob::split_glob_source(&source).map_err(|error| {
            manual_error(
                operation,
                format!(
                    "invalid glob source pattern `{}`: {error}",
                    source.display()
                ),
            )
        })?;
        let base_path = paths::resolve_path(&context.root_path, &split.base)
            .map_err(|error| {
                manual_error(
                    operation,
                    format!(
                        "unsupported glob source pattern `{}`: {}",
                        source.display(),
                        error.reason()
                    ),
                )
            })
            .and_then(|resolved| {
                paths::normalize_maybe_existing(&resolved).map_err(|error| {
                    manual_error(
                        operation,
                        format!(
                            "failed to resolve glob source pattern `{}`: {error}",
                            source.display()
                        ),
                    )
                })
            })?;
        validate_manual_glob_base(operation, context, policy, &source, &base_path)?;
        let ignore_rules = manual_glob_ignore_rules(operation, &source, &base_path, ignore)?;
        let matches = glob::expand_glob_source(&base_path, &split, ignore_rules.as_ref()).map_err(
            |error| {
                manual_error(
                    operation,
                    format!(
                        "failed to expand glob source pattern `{}`: {error}",
                        source.display()
                    ),
                )
            },
        )?;

        if matches.is_empty() {
            if policy.required {
                return invalid_manual(
                    operation,
                    format!(
                        "no sources match required glob source pattern `{}`",
                        source.display()
                    ),
                );
            }

            expanded.push((source, PathBuf::new()));
            continue;
        }

        for entry in matches {
            expanded.push((split.base.join(&entry.relative), entry.relative));
        }
    }

    Ok(expanded)
}

/// Rejects glob pattern bases outside the root checkout before expansion
/// walks them, unless sources outside the root are explicitly allowed.
fn validate_manual_glob_base(
    operation: FileOperationKind,
    context: &Worktree,
    policy: &ManualGlobPolicy,
    source: &Path,
    base_path: &Path,
) -> Result<()> {
    if policy.dangerously_allow_sources_outside_root {
        return Ok(());
    }

    let root_path = paths::normalize_maybe_existing(&context.root_path).map_err(|error| {
        manual_error(operation, format!("failed to resolve root path: {error}"))
    })?;
    if base_path != root_path && !base_path.starts_with(&root_path) {
        return invalid_manual(
            operation,
            format!(
                "source resolves outside root for glob source pattern `{}`",
                source.display()
            ),
        );
    }

    Ok(())
}

fn manual_glob_ignore_rules(
    operation: FileOperationKind,
    source: &Path,
    base_path: &Path,
    ignore: &[String],
) -> Result<Option<PathIgnoreRules>> {
    if !matches!(operation, FileOperationKind::Copy | FileOperationKind::Sync) || ignore.is_empty()
    {
        return Ok(None);
    }

    PathIgnoreRules::new(base_path, ignore)
        .map(Some)
        .map_err(|error| {
            manual_error(
                operation,
                format!(
                    "invalid ignore pattern for glob source pattern `{}`: {error}",
                    source.display()
                ),
            )
        })
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

impl From<MetadataField> for RawMetadataField {
    fn from(value: MetadataField) -> Self {
        match value {
            MetadataField::Permissions => Self::Permissions,
            MetadataField::Owner => Self::Owner,
            MetadataField::Group => Self::Group,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum ManualPathRole {
    Source,
    Target,
}

impl ManualPathRole {
    const fn label(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Target => "target",
        }
    }
}

fn resolve_path(
    operation: FileOperationKind,
    base: &Path,
    path: &Path,
    source_path: &Path,
    target_path: &Path,
    role: ManualPathRole,
) -> Result<PathBuf> {
    paths::resolve_path(base, path).map_err(|source| Error::FileOperationInvalid {
        operation: operation.as_str(),
        message: unsupported_path_message(path, source_path, target_path, role, source),
    })
}

fn unsupported_path_message(
    path: &Path,
    source_path: &Path,
    target_path: &Path,
    role: ManualPathRole,
    source: UnsupportedPath,
) -> String {
    format!(
        "unsupported {} path `{}` for source `{}` target `{}`: {}",
        role.label(),
        path.display(),
        source_path.display(),
        target_path.display(),
        source.reason()
    )
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
    Err(manual_error(operation, message))
}

fn manual_error(operation: FileOperationKind, message: impl Into<String>) -> Error {
    Error::FileOperationInvalid {
        operation: operation.as_str(),
        message: message.into(),
    }
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
    use crate::ActionPlanOptions;

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

    fn options(operation: FileOperationKind, sources: &[&str]) -> ManualFileOperationOptions {
        ManualFileOperationOptions {
            operation,
            sources: sources.iter().map(PathBuf::from).collect(),
            ..ManualFileOperationOptions::default()
        }
    }

    #[test]
    fn manual_operations_should_map_single_source_to_same_target() {
        let (root, worktree) = temp_workspace("single-default-target");
        let context = context(&root, &worktree);
        let options = options(FileOperationKind::Copy, &[".env"]);
        let operations = FileOperation::from_manual_options(&context, options)
            .expect("operation should normalize");

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

        let operations = FileOperation::from_manual_options(&context, options)
            .expect("operation should normalize");

        assert_eq!(operations[0].target, PathBuf::from("local/.env"));
        assert_eq!(operations[0].target_path, worktree.join("local/.env"));
    }

    #[test]
    fn manual_operations_should_map_multiple_sources_to_default_targets() {
        let (root, worktree) = temp_workspace("multi-default-target");
        let context = context(&root, &worktree);
        let options = options(FileOperationKind::Copy, &[".env", ".npmrc"]);
        let operations = FileOperation::from_manual_options(&context, options)
            .expect("operation should normalize");

        assert_eq!(operations[0].target_path, worktree.join(".env"));
        assert_eq!(operations[1].target_path, worktree.join(".npmrc"));
    }

    #[test]
    fn manual_operations_should_map_multiple_sources_under_target_prefix() {
        let (root, worktree) = temp_workspace("multi-target-prefix");
        let context = context(&root, &worktree);
        let mut options = options(FileOperationKind::Copy, &["a", "nested/c"]);
        options.target = Some(PathBuf::from("local"));

        let operations = FileOperation::from_manual_options(&context, options)
            .expect("operation should normalize");

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
        let mut options = ManualFileOperationOptions {
            operation: FileOperationKind::Copy,
            sources: vec![source.clone()],
            ..ManualFileOperationOptions::default()
        };
        options.sources.push(root.join("b"));
        options.target = Some(PathBuf::from("local"));

        let operations = FileOperation::from_manual_options(&context, options)
            .expect("operation should normalize");

        assert_eq!(operations[0].source_path, source);
        assert_eq!(operations[0].target_path, worktree.join("local/a"));
        assert_eq!(operations[1].target_path, worktree.join("local/b"));
    }

    #[cfg(windows)]
    #[test]
    fn manual_operations_should_reject_drive_relative_windows_sources() {
        let (root, worktree) = temp_workspace("drive-relative-source");
        let context = context(&root, &worktree);
        let options = options(FileOperationKind::Copy, &[r"C:shared\.env"]);

        let error = FileOperation::from_manual_options(&context, options)
            .expect_err("drive-relative source should fail");

        assert!(
            error
                .to_string()
                .contains("drive-relative paths are not supported")
        );
        assert!(error.to_string().contains("source `C:shared\\.env`"));
        assert!(error.to_string().contains("target `C:shared\\.env`"));
    }

    #[cfg(windows)]
    #[test]
    fn manual_operations_should_reject_drive_relative_windows_targets_with_context() {
        let (root, worktree) = temp_workspace("drive-relative-target");
        let context = context(&root, &worktree);
        let mut options = options(FileOperationKind::Copy, &[".env"]);
        options.target = Some(PathBuf::from(r"C:local\.env"));

        let error = FileOperation::from_manual_options(&context, options)
            .expect_err("drive-relative target should fail");

        let message = error.to_string();
        assert!(message.contains("drive-relative paths are not supported"));
        assert!(message.contains("source `.env`"));
        assert!(message.contains("target `C:local\\.env`"));
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
            &options.ignore,
            &options.ignore_metadata,
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
            &options.ignore,
            &options.ignore_metadata,
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
            &options.ignore,
            &options.ignore_metadata,
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
            &options.ignore,
            &options.ignore_metadata,
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
            &options.ignore,
            &options.ignore_metadata,
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
            &options.ignore,
            &options.ignore_metadata,
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
        options.ignore = vec!["**/vendor/**".to_owned(), "!**/vendor/keep/**".to_owned()];
        options.ignore_metadata = vec![MetadataField::Owner, MetadataField::Group];

        let operations = FileOperation::from_manual_options(&context, options)
            .expect("operation should normalize");

        assert_eq!(operations[0].compare, Some(SyncCompare::Checksum));
        assert_eq!(operations[0].delete, Some(true));
        assert_eq!(operations[0].symlinks, Some(SymlinkMode::Preserve));
        assert_eq!(
            operations[0].ignore,
            vec!["**/vendor/**", "!**/vendor/keep/**"]
        );
        assert_eq!(
            operations[0].ignore_metadata,
            vec![MetadataField::Owner, MetadataField::Group]
        );
    }

    #[test]
    fn validate_manual_options_should_reject_ignore_for_symlink() {
        let mut options = options(FileOperationKind::Symlink, &["file"]);
        options.ignore = vec!["**/tmp/**".to_owned()];

        let error = validate_manual_options(
            options.operation,
            &options.sources,
            options.symlinks,
            options.compare,
            options.delete,
            &options.ignore,
            &options.ignore_metadata,
        )
        .expect_err("ignore should fail");

        assert!(
            error
                .to_string()
                .contains("`ignore` is only valid for copy and sync")
        );
    }

    #[test]
    fn validate_manual_options_should_reject_ignored_metadata_for_symlink() {
        let mut options = options(FileOperationKind::Symlink, &["file"]);
        options.ignore_metadata = vec![MetadataField::Permissions];

        let error = validate_manual_options(
            options.operation,
            &options.sources,
            options.symlinks,
            options.compare,
            options.delete,
            &options.ignore,
            &options.ignore_metadata,
        )
        .expect_err("ignore_metadata should fail");

        assert!(
            error
                .to_string()
                .contains("`ignore_metadata` is only valid for copy and sync")
        );
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
                environment: EnvironmentInput::empty(),
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
                glob: false,
                target_explicit: true,
                compare: None,
                delete: None,
                symlinks: Some(SymlinkMode::Preserve),
                ignore: Vec::new(),
                ignore_metadata: Vec::new(),
                ignore_prefix: PathBuf::new(),
                declaration: manual_span(),
            }],
            ActionPlanOptions::default(),
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
                glob: false,
                target_explicit: true,
                compare: Some(SyncCompare::Metadata),
                delete: Some(false),
                symlinks: Some(SymlinkMode::Preserve),
                ignore: Vec::new(),
                ignore_metadata: Vec::new(),
                ignore_prefix: PathBuf::new(),
                declaration: manual_span(),
            }],
            ActionPlanOptions {
                strict: true,
                ..ActionPlanOptions::default()
            },
        )
        .expect_err("strict sync should fail");

        assert!(error.to_string().contains("cannot be used with sync"));
        assert!(!worktree.join("shared").exists());
    }

    #[test]
    fn manual_operations_should_expand_glob_sources_like_shell_expansion() {
        let (root, worktree) = temp_workspace("glob-shell-parity");
        std::fs::create_dir_all(root.join("certs")).expect("dirs should be created");
        std::fs::write(root.join("certs/a.pem"), "a").expect("file should be written");
        std::fs::write(root.join("certs/b.pem"), "b").expect("file should be written");
        let context = context(&root, &worktree);
        let mut options = options(FileOperationKind::Copy, &["certs/*.pem"]);
        options.target = Some(PathBuf::from("local"));

        let operations = FileOperation::from_manual_options(&context, options)
            .expect("glob sources should normalize");

        let targets = operations
            .iter()
            .map(|operation| operation.target.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            targets,
            vec![
                PathBuf::from("local").join("certs/a.pem"),
                PathBuf::from("local").join("certs/b.pem"),
            ]
        );
        assert_eq!(operations[0].ignore_prefix, PathBuf::from("a.pem"));
    }

    #[test]
    fn manual_operations_should_use_exact_target_for_single_glob_match() {
        let (root, worktree) = temp_workspace("glob-single-match");
        std::fs::create_dir_all(root.join("certs")).expect("dirs should be created");
        std::fs::write(root.join("certs/only.pem"), "o").expect("file should be written");
        let context = context(&root, &worktree);
        let mut options = options(FileOperationKind::Copy, &["certs/*.pem"]);
        options.target = Some(PathBuf::from("local/cert.pem"));

        let operations = FileOperation::from_manual_options(&context, options)
            .expect("glob source should normalize");

        assert_eq!(operations.len(), 1);
        assert_eq!(
            operations[0].source,
            PathBuf::from("certs").join("only.pem")
        );
        assert_eq!(operations[0].target, PathBuf::from("local/cert.pem"));
    }

    #[test]
    fn manual_operations_should_keep_optional_zero_match_patterns_literal() {
        let (root, worktree) = temp_workspace("glob-zero-match-literal");
        let context = context(&root, &worktree);
        let options = options(FileOperationKind::Copy, &["missing/*.pem"]);

        let operations = FileOperation::from_manual_options(&context, options)
            .expect("zero matches should normalize");

        assert_eq!(operations.len(), 1);
        assert_eq!(operations[0].source, PathBuf::from("missing/*.pem"));
    }

    #[test]
    fn manual_operations_should_fail_required_zero_match_patterns() {
        let (root, worktree) = temp_workspace("glob-zero-match-required");
        let context = context(&root, &worktree);
        let mut options = options(FileOperationKind::Copy, &["missing/*.pem"]);
        options.required = true;

        let error = FileOperation::from_manual_options(&context, options)
            .expect_err("required zero matches should fail");

        assert!(
            error
                .to_string()
                .contains("no sources match required glob source pattern"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn manual_operations_should_treat_sources_literally_without_glob() {
        let (root, worktree) = temp_workspace("glob-disabled");
        std::fs::create_dir_all(root.join("certs")).expect("dirs should be created");
        std::fs::write(root.join("certs/a.pem"), "a").expect("file should be written");
        let context = context(&root, &worktree);
        let mut options = options(FileOperationKind::Copy, &["certs/*.pem"]);
        options.glob = false;

        let operations = FileOperation::from_manual_options(&context, options)
            .expect("literal pattern should normalize");

        assert_eq!(operations.len(), 1);
        assert_eq!(operations[0].source, PathBuf::from("certs/*.pem"));
    }

    #[test]
    fn manual_operations_should_reject_glob_bases_outside_root_before_expansion() {
        let (root, worktree) = temp_workspace("glob-outside-base");
        let base = root.parent().expect("root should have parent");
        std::fs::create_dir_all(base.join("outside")).expect("dirs should be created");
        std::fs::write(base.join("outside/a.pem"), "a").expect("file should be written");
        let context = context(&root, &worktree);
        let options = options(FileOperationKind::Copy, &["../outside/*.pem"]);

        let error = FileOperation::from_manual_options(&context, options)
            .expect_err("outside glob base should fail");

        assert!(
            error
                .to_string()
                .contains("source resolves outside root for glob source pattern"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn manual_operations_should_expand_outside_glob_bases_with_dangerous_override() {
        let (root, worktree) = temp_workspace("glob-outside-base-allowed");
        let base = root.parent().expect("root should have parent");
        std::fs::create_dir_all(base.join("outside")).expect("dirs should be created");
        std::fs::write(base.join("outside/a.pem"), "a").expect("file should be written");
        let context = context(&root, &worktree);
        let mut options = options(FileOperationKind::Copy, &["../outside/*.pem"]);
        options.dangerously_allow_sources_outside_root = true;
        options.target = Some(PathBuf::from("out.pem"));

        let operations = FileOperation::from_manual_options(&context, options)
            .expect("outside glob base should expand with the dangerous override");

        assert_eq!(operations.len(), 1);
        assert_eq!(
            operations[0].source,
            PathBuf::from("../outside").join("a.pem")
        );
    }

    #[test]
    fn manual_operations_should_reject_invalid_glob_patterns() {
        let (root, worktree) = temp_workspace("glob-invalid-pattern");
        let context = context(&root, &worktree);
        let options = options(FileOperationKind::Copy, &["certs/[ab"]);

        let error = FileOperation::from_manual_options(&context, options)
            .expect_err("invalid pattern should fail");

        assert!(
            error.to_string().contains("invalid glob pattern"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn manual_operations_should_reject_parent_components_after_glob_patterns() {
        let (root, worktree) = temp_workspace("glob-parent-after-pattern");
        let context = context(&root, &worktree);
        let options = options(FileOperationKind::Copy, &["certs/*/../other"]);

        let error = FileOperation::from_manual_options(&context, options)
            .expect_err("parent component should fail");

        assert!(
            error.to_string().contains("invalid glob source pattern"),
            "unexpected error: {error}"
        );
    }
}
