use std::path::PathBuf;
use std::process::Command;

use crate::commands::{CommandExecutionOptions, execute_commands};
use crate::config::{self, RuntimeOptionOverrides};
use crate::context;
use crate::discovery;
use crate::files::{FileApplyOptions, apply_file_operations};
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
    pub skip_commands: bool,
}

/// Completed action for a `treeboot run` invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunAction {
    /// No config or executable init script was detected.
    MissingConfig,
    /// The run started from the root checkout and had no work to do.
    RootWorktreeSkipped,
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
    let env_options = RuntimeOptionOverrides::from_env()?;
    let pre_config_strict = env_options.pre_config_strict(options.strict);
    let context = context::resolve(&options)?;

    if context.root_path == context.worktree_path {
        report(reporter, OutputEvent::RootWorktreeDetected)?;

        if pre_config_strict {
            return Err(Error::RootWorktreeStrict);
        }

        return Ok(RunReport {
            context,
            action: RunAction::RootWorktreeSkipped,
        });
    }

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
            let config = config::load_config(&path, &context)?;
            let plan_options = env_options.resolve(&config.options, options.strict);
            let plan = crate::plan_run_config(&path, &config, &context, plan_options.into())?;

            apply_file_operations(
                &plan,
                FileApplyOptions {
                    strict: plan_options.strict,
                    force: options.force,
                    dry_run: options.dry_run,
                },
                reporter,
            )?;

            if !options.skip_commands {
                execute_commands(
                    &plan,
                    CommandExecutionOptions {
                        dry_run: options.dry_run,
                    },
                    reporter,
                )?;
            }

            Ok(RunReport {
                context,
                action: RunAction::ConfigApplied { path },
            })
        }
        None => {
            report(reporter, OutputEvent::NoConfigDetected)?;

            if pre_config_strict {
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
