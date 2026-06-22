use std::path::PathBuf;

use crate::context;
use crate::{Config, InitScriptDiscovery, Result, Worktree, WorktreeOptions};

/// Options for inspecting treeboot discovery status.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatusOptions {
    /// Directory from which status discovery starts.
    pub cwd: Option<PathBuf>,
    /// Overrides the root checkout used for discovery.
    pub root: Option<PathBuf>,
    /// Uses one specific config file instead of config discovery.
    pub config: Option<PathBuf>,
    /// Skips init script discovery.
    pub no_init_script: bool,
}

/// Init script discovery status for a worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InitScriptStatus {
    /// Init script discovery was skipped by options.
    Skipped,
    /// No executable init script was found.
    Missing {
        /// Existing init script paths ignored because they are not executable.
        ignored: Vec<PathBuf>,
    },
    /// An executable init script was found.
    Found {
        /// Init script path.
        path: PathBuf,
    },
}

/// Result summary for a `treeboot status` invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusReport {
    /// Runtime context discovered for the current worktree.
    pub context: Worktree,
    /// Init script discovery result.
    pub init_script: InitScriptStatus,
    /// Selected config path, when one was requested or discovered.
    pub config: Option<PathBuf>,
}

/// Inspects worktree, root, init script, and config discovery status.
///
/// This function does not execute init scripts, parse config, or run configured
/// commands.
///
/// # Errors
///
/// Returns an error if context discovery fails or a requested config file does
/// not exist.
pub fn inspect_status(options: StatusOptions) -> Result<StatusReport> {
    let context = context::resolve(&WorktreeOptions {
        cwd: options.cwd,
        root: options.root,
    })?;
    let init_script = if options.no_init_script || options.config.is_some() {
        InitScriptStatus::Skipped
    } else {
        inspect_init_script(&context)
    };
    let config = Config::discover_path(&context, options.config.as_deref())?;

    Ok(StatusReport {
        context,
        init_script,
        config,
    })
}

fn inspect_init_script(context: &Worktree) -> InitScriptStatus {
    let scripts = InitScriptDiscovery::discover(context);

    if let Some(path) = scripts.executable {
        InitScriptStatus::Found { path }
    } else {
        InitScriptStatus::Missing {
            ignored: scripts.ignored,
        }
    }
}
