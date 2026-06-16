use std::path::PathBuf;
use std::process::Command;

use crate::context;
use crate::discovery;
use crate::{Error, OutputEvent, Reporter, Result, RunContext};

/// Options for running worktree bootstrap.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunOptions {
    /// Directory from which the run starts. Defaults to the process cwd.
    pub cwd: Option<PathBuf>,
    /// Overrides the root checkout used as the file-operation source.
    pub root: Option<PathBuf>,
    /// Uses one specific config file and skips init script discovery.
    pub config: Option<PathBuf>,
    /// Fails on missing config and stricter file-operation conflicts.
    pub strict: bool,
    /// Replaces existing file-operation targets where supported.
    pub force: bool,
    /// Prints planned work without changing files or running commands.
    pub dry_run: bool,
    /// Runs file operations only.
    pub no_commands: bool,
}

/// Completed action for a `treeboot run` invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunAction {
    /// No config or executable init script was detected.
    MissingConfig,
    /// An init script would run in dry-run mode.
    WouldRunInitScript {
        /// Script path.
        path: PathBuf,
    },
    /// An init script was executed.
    RanInitScript {
        /// Script path.
        path: PathBuf,
    },
    /// A declarative config was detected.
    ConfigDetected {
        /// Config file path.
        path: PathBuf,
    },
}

/// Result summary for a `treeboot run` invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunReport {
    /// Runtime context used by the run.
    pub context: RunContext,
    /// Action taken by the run flow.
    pub action: RunAction,
}

/// Runs worktree bootstrap according to the provided options.
///
/// Resolves the worktree context, discovers executable init scripts and
/// declarative config files, reports the selected action, and executes an init
/// script when one should run.
///
/// # Errors
///
/// Returns an error if context discovery fails, output reporting fails, an init
/// script cannot be started or exits unsuccessfully, a configured file cannot
/// be read, or strict mode treats a missing config as a failure.
pub fn run(options: RunOptions, reporter: &mut dyn Reporter) -> Result<RunReport> {
    let context = context::resolve(&options)?;

    if options.config.is_none() {
        let scripts = discovery::discover_scripts(&context.worktree_path);

        for path in scripts.ignored {
            report(reporter, OutputEvent::IgnoredInitScript { path })?;
        }

        if let Some(path) = scripts.executable {
            return run_init_script(path, context, &options, reporter);
        }
    }

    match discovery::discover_config(&context.worktree_path, options.config.as_deref())? {
        Some(path) => {
            report(reporter, OutputEvent::ConfigDetected { path: path.clone() })?;

            Err(Error::ConfigExecutionNotImplemented(path))
        }
        None => {
            report(reporter, OutputEvent::NoConfigDetected)?;

            if options.strict {
                Err(Error::NoConfigDetectedStrict)
            } else {
                Ok(RunReport {
                    context,
                    action: RunAction::MissingConfig,
                })
            }
        }
    }
}

fn run_init_script(
    path: PathBuf,
    context: RunContext,
    options: &RunOptions,
    reporter: &mut dyn Reporter,
) -> Result<RunReport> {
    if options.dry_run {
        report(
            reporter,
            OutputEvent::WouldRunInitScript {
                path: path.clone(),
                root_path: context.root_path.clone(),
            },
        )?;

        return Ok(RunReport {
            context,
            action: RunAction::WouldRunInitScript { path },
        });
    }

    report(reporter, OutputEvent::RunInitScript { path: path.clone() })?;

    let status = Command::new(&path)
        .arg(&context.root_path)
        .current_dir(&context.worktree_path)
        .envs(&context.environment)
        .status()
        .map_err(|source| Error::ScriptIo {
            path: path.clone(),
            source,
        })?;

    if !status.success() {
        return Err(Error::ScriptFailed { path, status });
    }

    Ok(RunReport {
        context,
        action: RunAction::RanInitScript { path },
    })
}

fn report(reporter: &mut dyn Reporter, event: OutputEvent) -> Result<()> {
    reporter
        .report(event)
        .map_err(|source| Error::Output { source })
}
