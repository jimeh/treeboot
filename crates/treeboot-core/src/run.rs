use std::path::PathBuf;

use crate::file_operations::FileApplyOptions;
use crate::plan::{BootstrapPreparationOptions, prepare_bootstrap};
use crate::{BootstrapAction, BootstrapReport, EnvironmentInput, Reporter, Result, Worktree};

/// Options for running worktree bootstrap.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunOptions {
    /// Directory from which the run starts. Defaults to the process cwd.
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
    /// Prints planned work without changing files or running commands.
    pub dry_run: bool,
    /// Prints detailed file-operation actions instead of compact summaries.
    pub verbose: bool,
    /// Runs file operations only.
    pub skip_commands: bool,
}

/// Completed action for a `treeboot run` invocation.
///
/// New run outcomes may be added in future releases. Downstream matches must
/// include a wildcard arm so they remain forward compatible.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RunAction {
    /// No config was detected.
    MissingConfig,
    /// The run started from the root checkout and had no work to do.
    RootWorktreeSkipped,
    /// A declarative config was detected.
    ConfigDetected {
        /// Config file path.
        path: PathBuf,
    },
    /// Declarative config file operations were applied.
    ConfigApplied {
        /// Config file path.
        path: PathBuf,
    },
}

/// Result summary for a `treeboot run` invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunReport {
    /// Runtime context used by the run.
    pub context: Worktree,
    /// Action taken by the run flow.
    pub action: RunAction,
}

/// Runs worktree bootstrap according to the provided options.
///
/// Resolves the worktree context, discovers declarative config files, reports
/// the selected action, and executes the resulting action plan.
///
/// # Errors
///
/// Returns an error if context discovery, config loading, validation, file or
/// command execution, or output reporting fails.
pub fn run(options: RunOptions, reporter: &mut dyn Reporter) -> Result<RunReport> {
    let dry_run = options.dry_run;
    let verbose = options.verbose;
    let skip_commands = options.skip_commands;
    let force = options.force;
    let strict = options.strict;
    let mut prepared = prepare_bootstrap(options.into(), false, reporter)?;
    prepared.execute(
        FileApplyOptions {
            strict,
            force,
            dry_run,
            verbose,
        },
        skip_commands,
        reporter,
    )?;

    Ok(RunReport {
        context: prepared.context,
        action: run_action(&prepared.report.action),
    })
}

/// Runs worktree bootstrap and returns the complete decision report.
///
/// This facade preflights every prepared action path for structured
/// serialization before it applies any file changes. Existing text callers
/// should continue to use [`run`].
///
/// Direct callers that set neither [`RunOptions::dry_run`] nor
/// [`RunOptions::skip_commands`] may spawn configured commands with inherited
/// stdio. The CLI rejects that combination for structured output.
///
/// # Errors
///
/// Returns an error if discovery, planning, structured path preflight, file or
/// command execution, or output reporting fails.
pub fn run_detailed(options: RunOptions, reporter: &mut dyn Reporter) -> Result<BootstrapReport> {
    let dry_run = options.dry_run;
    let verbose = options.verbose;
    let skip_commands = options.skip_commands;
    let force = options.force;
    let strict = options.strict;
    let mut prepared = prepare_bootstrap(options.into(), false, reporter)?;
    prepared.preflight_structured_paths()?;
    prepared.execute(
        FileApplyOptions {
            strict,
            force,
            dry_run,
            verbose,
        },
        skip_commands,
        reporter,
    )?;
    Ok(prepared.report)
}

impl From<RunOptions> for BootstrapPreparationOptions {
    fn from(options: RunOptions) -> Self {
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

fn run_action(action: &BootstrapAction) -> RunAction {
    match action {
        BootstrapAction::MissingConfig => RunAction::MissingConfig,
        BootstrapAction::RootWorktreeSkipped => RunAction::RootWorktreeSkipped,
        BootstrapAction::Config { path } => RunAction::ConfigApplied { path: path.clone() },
    }
}
