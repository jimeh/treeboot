use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::commands::{CommandExecutionOptions, command_label, execute_commands};
use crate::context;
use crate::file_actions::{FileAction, PlannedFileOperationActions};
#[cfg(test)]
use crate::file_operations::execute_prepared_file_operations_with_warning_injector;
use crate::file_operations::{
    FileApplyOptions, PreparedFileOperations, execute_prepared_file_operations,
    prepare_file_operations,
};
use crate::{
    ActionPlan, Config, EnvironmentInput, Error, FileOperationKind, OutputEvent, Reporter, Result,
    RuntimePolicy, WorktreeOptions, WorktreeSnapshot,
};

/// Options for planning worktree bootstrap without side effects.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct PlanOptions {
    /// Directory from which planning starts. Defaults to the process cwd.
    pub cwd: Option<PathBuf>,
    /// Overrides the root checkout used as the file-operation source.
    pub root: Option<PathBuf>,
    /// Explicit environment input used for compatibility discovery and options.
    pub environment: EnvironmentInput,
    /// Uses one specific config file instead of config discovery.
    pub config: Option<PathBuf>,
    /// Fails on missing config and stricter file-operation conflicts.
    pub strict: bool,
    /// Replaces existing file-operation targets where supported.
    pub force: bool,
    /// Prints detailed file-operation actions instead of compact summaries.
    pub verbose: bool,
    /// Omits configured commands from the plan.
    pub skip_commands: bool,
}

/// Action represented by a bootstrap report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum BootstrapAction {
    /// No config was detected.
    MissingConfig,
    /// Planning started from the root checkout and had no work to do.
    RootWorktreeSkipped,
    /// A declarative config was planned.
    Config {
        /// Config file path.
        path: PathBuf,
    },
}

/// Aggregate file decisions in a bootstrap report.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct BootstrapFileSummary {
    /// Number of created, updated, replaced, or metadata-repaired paths.
    pub changed: usize,
    /// Number of skipped paths.
    pub skipped: usize,
    /// Number of deleted target-only paths.
    pub deleted: usize,
    /// Number of metadata-only repairs, also included in `changed`.
    pub metadata_changed: usize,
    /// Number of concrete file-planning warnings.
    pub file_warnings: usize,
}

/// File decisions for one declared operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct BootstrapFileOperation {
    /// Declared operation kind.
    pub operation: FileOperationKind,
    /// Display source path.
    pub source: PathBuf,
    /// Display target path.
    pub target: PathBuf,
    /// Decisions produced for the operation.
    pub summary: BootstrapFileOperationSummary,
}

/// File decisions for one top-level operation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct BootstrapFileOperationSummary {
    /// Number of created, updated, replaced, or metadata-repaired paths.
    pub changed: usize,
    /// Number of skipped paths.
    pub skipped: usize,
    /// Number of deleted target-only paths.
    pub deleted: usize,
    /// Number of metadata-only repairs, also included in `changed`.
    pub metadata_changed: usize,
    /// Number of concrete file-planning warnings.
    pub file_warnings: usize,
    /// Whether the operation expanded into child actions.
    pub expanded: bool,
    /// Reason for a single unexpanded skip.
    pub skip_reason: Option<String>,
}

/// Configured command represented by a bootstrap report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct BootstrapCommand {
    /// Human-readable command label used by text output.
    pub label: String,
    /// Whether command failure is non-fatal.
    pub allow_failure: bool,
}

/// Non-fatal file warning represented by a bootstrap report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct BootstrapWarning {
    /// Display path associated with the warning.
    pub path: PathBuf,
    /// Human-readable warning reason.
    pub reason: String,
}

/// Complete bootstrap planning and execution report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct BootstrapReport {
    /// Resolved worktree context without the child process environment.
    pub context: WorktreeSnapshot,
    /// Config discovery action represented by the report.
    pub action: BootstrapAction,
    /// Whether the planned files include a change or deletion.
    pub has_file_changes: bool,
    /// Aggregate file decisions.
    pub file_summary: BootstrapFileSummary,
    /// File decisions in config declaration order.
    pub files: Vec<BootstrapFileOperation>,
    /// Whether configured commands were omitted.
    pub commands_skipped: bool,
    /// Configured commands in declaration order.
    pub commands: Vec<BootstrapCommand>,
    /// Ordered non-fatal semantic validation warnings.
    pub validation_warnings: Vec<String>,
    /// Ordered concrete file-planning warnings.
    pub file_warnings: Vec<BootstrapWarning>,
    /// Ordered non-fatal warnings produced while applying files.
    pub execution_warnings: Vec<BootstrapWarning>,
}

#[derive(Debug, Clone)]
pub(crate) struct BootstrapPreparationOptions {
    pub(crate) cwd: Option<PathBuf>,
    pub(crate) root: Option<PathBuf>,
    pub(crate) environment: EnvironmentInput,
    pub(crate) config: Option<PathBuf>,
    pub(crate) strict: bool,
    pub(crate) force: bool,
    pub(crate) verbose: bool,
    pub(crate) skip_commands: bool,
}

pub(crate) struct PreparedBootstrap {
    pub(crate) context: crate::Worktree,
    pub(crate) report: BootstrapReport,
    pub(crate) plan: Option<ActionPlan>,
    pub(crate) files: PreparedFileOperations,
}

impl PreparedBootstrap {
    pub(crate) fn preflight_structured_paths(&self) -> Result<()> {
        preflight_path(&self.report.context.root_path)?;
        preflight_path(&self.report.context.worktree_path)?;
        if let BootstrapAction::Config { path } = &self.report.action {
            preflight_path(path)?;
        }
        for group in &self.files.groups {
            preflight_group_paths(group)?;
        }
        Ok(())
    }

    pub(crate) fn preview(
        &mut self,
        verbose: bool,
        skip_commands: bool,
        reporter: &mut dyn Reporter,
    ) -> Result<()> {
        let Some(plan) = &self.plan else {
            return Ok(());
        };
        execute_prepared_file_operations(
            plan,
            &self.files,
            FileApplyOptions {
                dry_run: true,
                verbose,
                ..FileApplyOptions::default()
            },
            reporter,
        )?;
        if !skip_commands {
            execute_commands(plan, CommandExecutionOptions { dry_run: true }, reporter)?;
        }
        Ok(())
    }

    pub(crate) fn execute(
        &mut self,
        options: FileApplyOptions,
        skip_commands: bool,
        reporter: &mut dyn Reporter,
    ) -> Result<usize> {
        let Some(plan) = &self.plan else {
            return Ok(0);
        };
        let file_report = execute_prepared_file_operations(plan, &self.files, options, reporter)?;
        self.report.execution_warnings = file_report.execution_warnings;
        if !skip_commands {
            execute_commands(
                plan,
                CommandExecutionOptions {
                    dry_run: options.dry_run,
                },
                reporter,
            )?;
        }
        Ok(file_report.action_count)
    }

    #[cfg(test)]
    fn execute_with_warning_injector(
        &mut self,
        options: FileApplyOptions,
        reporter: &mut dyn Reporter,
        warning_injector: crate::file_execution::ExecutionWarningInjector,
    ) -> Result<()> {
        let plan = self
            .plan
            .as_ref()
            .expect("test preparation should have a plan");
        let file_report = execute_prepared_file_operations_with_warning_injector(
            plan,
            &self.files,
            options,
            reporter,
            Some(warning_injector),
        )?;
        self.report.execution_warnings = file_report.execution_warnings;
        Ok(())
    }
}

/// Plans worktree bootstrap without changing files or spawning commands.
///
/// # Errors
///
/// Returns an error if discovery, config loading, validation, filesystem
/// planning, or output reporting fails.
pub fn plan(options: PlanOptions, reporter: &mut dyn Reporter) -> Result<BootstrapReport> {
    let skip_commands = options.skip_commands;
    let verbose = options.verbose;
    let mut prepared = prepare_bootstrap(options.into(), true, reporter)?;
    prepared.preview(verbose, skip_commands, reporter)?;
    Ok(prepared.report)
}

pub(crate) fn prepare_bootstrap(
    options: BootstrapPreparationOptions,
    emit_validation_warnings: bool,
    reporter: &mut dyn Reporter,
) -> Result<PreparedBootstrap> {
    let runtime_policy = RuntimePolicy::from_environment(&options.environment, options.strict)?;
    let pre_config_strict = runtime_policy.pre_config_strict();
    let context = context::resolve(&WorktreeOptions {
        cwd: options.cwd,
        root: options.root,
        environment: options.environment,
    })?;

    if context.is_root() {
        report(reporter, OutputEvent::RootWorktreeDetected)?;
        if pre_config_strict {
            return Err(Error::RootWorktreeStrict);
        }
        return Ok(empty_prepared(
            &context,
            BootstrapAction::RootWorktreeSkipped,
            options.skip_commands,
        ));
    }

    let Some(path) = Config::discover_path(&context, options.config.as_deref())? else {
        report(reporter, OutputEvent::NoConfigDetected)?;
        if pre_config_strict {
            return Err(Error::NoConfigDetectedStrict);
        }
        return Ok(empty_prepared(
            &context,
            BootstrapAction::MissingConfig,
            options.skip_commands,
        ));
    };

    report(reporter, OutputEvent::ConfigDetected { path: path.clone() })?;
    let config = Config::load(&path, &context)?;
    let plan_options = runtime_policy.resolve(&config.options);
    let strict = plan_options.strict();
    let plan = ActionPlan::from_manifest(
        &path,
        &config,
        &context,
        plan_options.into_action_plan_options(),
    )?;
    let validation_warnings = plan
        .warnings()
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if emit_validation_warnings {
        for message in &validation_warnings {
            report(
                reporter,
                OutputEvent::ValidationWarning {
                    message: message.clone(),
                },
            )?;
        }
    }
    let files = prepare_file_operations(
        &plan,
        FileApplyOptions {
            strict,
            force: options.force,
            dry_run: true,
            verbose: options.verbose,
        },
        reporter,
    )?;
    let report = build_report(
        &plan,
        BootstrapAction::Config { path },
        &files,
        options.skip_commands,
        validation_warnings,
    );

    Ok(PreparedBootstrap {
        context: plan.context().clone(),
        report,
        plan: Some(plan),
        files,
    })
}

impl From<PlanOptions> for BootstrapPreparationOptions {
    fn from(options: PlanOptions) -> Self {
        Self {
            cwd: options.cwd,
            root: options.root,
            environment: options.environment,
            config: options.config,
            strict: options.strict,
            force: options.force,
            verbose: options.verbose,
            skip_commands: options.skip_commands,
        }
    }
}

fn empty_prepared(
    context: &crate::Worktree,
    action: BootstrapAction,
    commands_skipped: bool,
) -> PreparedBootstrap {
    PreparedBootstrap {
        context: context.clone(),
        report: BootstrapReport {
            context: WorktreeSnapshot::from(context),
            action,
            has_file_changes: false,
            file_summary: BootstrapFileSummary::default(),
            files: Vec::new(),
            commands_skipped,
            commands: Vec::new(),
            validation_warnings: Vec::new(),
            file_warnings: Vec::new(),
            execution_warnings: Vec::new(),
        },
        plan: None,
        files: PreparedFileOperations::default(),
    }
}

fn build_report(
    plan: &ActionPlan,
    action: BootstrapAction,
    prepared: &PreparedFileOperations,
    commands_skipped: bool,
    validation_warnings: Vec<String>,
) -> BootstrapReport {
    let files = prepared
        .groups
        .iter()
        .map(operation_report)
        .collect::<Vec<_>>();
    let file_summary =
        files
            .iter()
            .fold(BootstrapFileSummary::default(), |mut total, operation| {
                total.changed += operation.summary.changed;
                total.skipped += operation.summary.skipped;
                total.deleted += operation.summary.deleted;
                total.metadata_changed += operation.summary.metadata_changed;
                total.file_warnings += operation.summary.file_warnings;
                total
            });
    let file_warnings = prepared
        .groups
        .iter()
        .flat_map(|group| group.actions.iter())
        .filter_map(|action| match action {
            FileAction::Warning { path, reason } => Some(BootstrapWarning {
                path: path.clone(),
                reason: reason.clone(),
            }),
            _ => None,
        })
        .collect();
    let commands = if commands_skipped {
        Vec::new()
    } else {
        plan.planned_commands()
            .iter()
            .map(|command| BootstrapCommand {
                label: command_label(command),
                allow_failure: command.allow_failure(),
            })
            .collect()
    };

    BootstrapReport {
        context: WorktreeSnapshot::from(plan.context()),
        action,
        has_file_changes: file_summary.changed > 0 || file_summary.deleted > 0,
        file_summary,
        files,
        commands_skipped,
        commands,
        validation_warnings,
        file_warnings,
        execution_warnings: Vec::new(),
    }
}

fn operation_report(group: &PlannedFileOperationActions) -> BootstrapFileOperation {
    let summary = group.summary();
    BootstrapFileOperation {
        operation: group.operation,
        source: group.source.clone(),
        target: group.target.clone(),
        summary: BootstrapFileOperationSummary {
            changed: summary.changed,
            skipped: summary.skipped,
            deleted: summary.deleted,
            metadata_changed: summary.metadata_changed,
            file_warnings: summary.warnings,
            expanded: summary.expanded,
            skip_reason: if summary.expanded {
                None
            } else {
                summary.skip_reason
            },
        },
    }
}

fn preflight_group_paths(group: &PlannedFileOperationActions) -> Result<()> {
    preflight_path(&group.source)?;
    preflight_path(&group.target)?;
    for action in &group.actions {
        match action {
            FileAction::CreateDirectory {
                source,
                target,
                target_path,
                ..
            } => preflight_paths([source, target, target_path])?,
            FileAction::CopyFile {
                source,
                target,
                source_path,
                target_path,
                ..
            }
            | FileAction::RepairMetadata {
                source,
                target,
                source_path,
                target_path,
                ..
            } => preflight_paths([source, target, source_path, target_path])?,
            FileAction::CreateSymlink {
                source,
                target,
                target_path,
                preserved_source_path,
                link_target,
                final_target,
                ..
            } => {
                preflight_paths([source, target, target_path, link_target, final_target])?;
                if let Some(path) = preserved_source_path {
                    preflight_path(path)?;
                }
            }
            FileAction::Delete {
                target,
                target_path,
            } => preflight_paths([target, target_path])?,
            FileAction::Skip { target, .. } => preflight_path(target)?,
            FileAction::Warning { path, .. } => preflight_path(path)?,
        }
    }
    Ok(())
}

fn preflight_paths<'a>(paths: impl IntoIterator<Item = &'a PathBuf>) -> Result<()> {
    for path in paths {
        preflight_path(path)?;
    }
    Ok(())
}

fn preflight_path(path: &Path) -> Result<()> {
    if path.to_str().is_some() {
        Ok(())
    } else {
        Err(Error::Output {
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("path cannot be represented in structured output: {path:?}"),
            ),
        })
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
    #[cfg(unix)]
    use std::ffi::OsString;
    #[cfg(unix)]
    use std::os::unix::ffi::OsStringExt;

    use super::*;

    #[derive(Default)]
    struct SilentReporter;

    impl Reporter for SilentReporter {
        fn report(&mut self, _event: OutputEvent) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[cfg(unix)]
    #[test]
    fn preflight_group_paths_should_reject_non_utf8_action_paths() {
        let non_utf8_path = PathBuf::from(OsString::from_vec(vec![b'f', 0x80]));
        let group = PlannedFileOperationActions {
            operation: crate::FileOperationKind::Copy,
            source: PathBuf::from("source"),
            target: PathBuf::from("target"),
            expanded: true,
            actions: vec![FileAction::Skip {
                operation: crate::FileOperationKind::Copy,
                target: non_utf8_path,
                reason: "test".to_owned(),
            }],
        };

        let error = preflight_group_paths(&group)
            .expect_err("non-UTF-8 action paths should fail structured-output preflight");

        assert!(
            error
                .to_string()
                .contains("path cannot be represented in structured output")
        );
    }

    #[test]
    fn detailed_execute_report_should_include_injected_execution_warning() {
        let context = crate::Worktree::from_parts(
            PathBuf::from("/root"),
            PathBuf::from("/worktree"),
            "main".to_owned(),
            BTreeMap::new(),
        );
        let plan = ActionPlan::from_parts_unchecked(
            context.clone(),
            crate::PlanOrigin::Manifest {
                path: PathBuf::from("/worktree/.treeboot.toml"),
            },
            Some(PathBuf::from("/worktree/.treeboot.toml")),
            Vec::new(),
            Vec::new(),
        );
        let files = PreparedFileOperations {
            groups: vec![PlannedFileOperationActions {
                operation: crate::FileOperationKind::Copy,
                source: PathBuf::from("source"),
                target: PathBuf::from("target"),
                expanded: false,
                actions: vec![FileAction::Skip {
                    operation: crate::FileOperationKind::Copy,
                    target: PathBuf::from("target"),
                    reason: "target exists".to_owned(),
                }],
            }],
        };
        let report = build_report(
            &plan,
            BootstrapAction::Config {
                path: PathBuf::from("/worktree/.treeboot.toml"),
            },
            &files,
            true,
            Vec::new(),
        );
        let mut prepared = PreparedBootstrap {
            context,
            report,
            plan: Some(plan),
            files,
        };
        let mut reporter = SilentReporter;

        prepared
            .execute_with_warning_injector(FileApplyOptions::default(), &mut reporter, |_| {
                Some(BootstrapWarning {
                    path: PathBuf::from("target"),
                    reason: "ownership could not be preserved".to_owned(),
                })
            })
            .expect("injected warning should remain non-fatal");

        assert_eq!(
            prepared.report.execution_warnings,
            [BootstrapWarning {
                path: PathBuf::from("target"),
                reason: "ownership could not be preserved".to_owned(),
            }]
        );
    }
}
