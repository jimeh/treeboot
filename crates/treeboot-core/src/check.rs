use std::path::PathBuf;

use serde::Serialize;

use crate::context;
use crate::{
    ActionPlan, Config, EnvironmentInput, Error, InitScriptDiscovery, Result, RuntimePolicy,
    Worktree, WorktreeOptions,
};

/// Options for checking treeboot bootstrap behavior.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CheckOptions {
    /// Directory from which the check starts. Defaults to the process cwd.
    pub cwd: Option<PathBuf>,
    /// Overrides the root checkout used as the file-operation source.
    pub root: Option<PathBuf>,
    /// Explicit environment input used for compatibility discovery and options.
    pub environment: EnvironmentInput,
    /// Uses one specific config file and skips init script discovery.
    pub config: Option<PathBuf>,
    /// Skips init script discovery and uses declarative config discovery.
    pub no_init_script: bool,
    /// Fails on missing config and stricter file-operation conflicts.
    pub strict: bool,
}

/// Completed action for a `treeboot check` invocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CheckAction {
    /// No config or executable init script was detected.
    MissingConfig,
    /// The check started from the root checkout and had no work to validate.
    RootWorktreeSkipped,
    /// An init script would take precedence.
    InitScript {
        /// Script path.
        path: PathBuf,
    },
    /// Declarative config was validated.
    Config {
        /// Config file path.
        path: PathBuf,
    },
}

/// Result summary for a `treeboot check` invocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CheckReport {
    /// Runtime context used by the check.
    pub context: WorktreeSnapshot,
    /// Action that was validated.
    pub action: CheckAction,
}

/// Serializable worktree context snapshot for reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorktreeSnapshot {
    /// Source checkout used for file operations.
    pub root_path: PathBuf,
    /// Current worktree root where targets and commands are anchored.
    pub worktree_path: PathBuf,
    /// Best-effort default branch name.
    pub default_branch: String,
}

impl From<&Worktree> for WorktreeSnapshot {
    fn from(context: &Worktree) -> Self {
        Self {
            root_path: context.root_path.clone(),
            worktree_path: context.worktree_path.clone(),
            default_branch: context.default_branch.clone(),
        }
    }
}

/// Checks treeboot bootstrap behavior without side effects.
///
/// # Errors
///
/// Returns an error when context discovery fails, strict mode treats the
/// current state as invalid, config loading fails, or declarative validation
/// fails.
pub fn check(options: CheckOptions) -> Result<CheckReport> {
    let runtime_policy = RuntimePolicy::from_environment(&options.environment, options.strict)?;
    let pre_config_strict = runtime_policy.pre_config_strict();
    let context = context::resolve(&WorktreeOptions {
        cwd: options.cwd.clone(),
        root: options.root.clone(),
        environment: options.environment.clone(),
    })?;

    if context.root_path == context.worktree_path {
        if pre_config_strict {
            return Err(Error::RootWorktreeStrict);
        }

        return Ok(CheckReport {
            context: WorktreeSnapshot::from(&context),
            action: CheckAction::RootWorktreeSkipped,
        });
    }

    if options.config.is_none() && !options.no_init_script {
        let scripts = InitScriptDiscovery::discover(&context);

        if let Some(path) = scripts.executable {
            return Ok(CheckReport {
                context: WorktreeSnapshot::from(&context),
                action: CheckAction::InitScript { path },
            });
        }
    }

    match Config::discover_path(&context, options.config.as_deref())? {
        Some(path) => {
            let config = Config::load(&path, &context)?;
            let plan_options = runtime_policy.resolve(&config.options);
            ActionPlan::from_manifest(
                &path,
                &config,
                &context,
                plan_options.action_plan_options(),
            )?;

            Ok(CheckReport {
                context: WorktreeSnapshot::from(&context),
                action: CheckAction::Config { path },
            })
        }
        None => {
            if pre_config_strict {
                Err(Error::NoConfigDetectedStrict)
            } else {
                Ok(CheckReport {
                    context: WorktreeSnapshot::from(&context),
                    action: CheckAction::MissingConfig,
                })
            }
        }
    }
}
