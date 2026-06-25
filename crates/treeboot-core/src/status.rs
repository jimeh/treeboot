use std::path::PathBuf;

use serde::Serialize;

use crate::check::WorktreeSnapshot;
use crate::context;
use crate::{Config, IgnoredInitScript, InitScriptDiscovery, Result, Worktree, WorktreeOptions};

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
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum InitScriptStatus {
    /// Init script discovery was skipped by options.
    Skipped,
    /// No executable init script was found.
    NotFound {
        /// Existing init script paths that were ignored.
        ignored: Vec<IgnoredInitScript>,
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

/// Serializable result summary for a `treeboot status` invocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatusSnapshotReport {
    /// Runtime context snapshot discovered for the current worktree.
    pub context: WorktreeSnapshot,
    /// Init script discovery result.
    pub init_script: InitScriptStatus,
    /// Selected config path, when one was requested or discovered.
    pub config: Option<PathBuf>,
}

impl From<&StatusReport> for StatusSnapshotReport {
    fn from(report: &StatusReport) -> Self {
        Self {
            context: WorktreeSnapshot::from(&report.context),
            init_script: report.init_script.clone(),
            config: report.config.clone(),
        }
    }
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

/// Inspects worktree, root, init script, and config discovery status as a
/// serializable snapshot.
///
/// This function does not execute init scripts, parse config, or run configured
/// commands.
///
/// # Errors
///
/// Returns an error if context discovery fails or a requested config file does
/// not exist.
pub fn inspect_status_snapshot(options: StatusOptions) -> Result<StatusSnapshotReport> {
    inspect_status(options).map(|report| StatusSnapshotReport::from(&report))
}

fn inspect_init_script(context: &Worktree) -> InitScriptStatus {
    let scripts = InitScriptDiscovery::discover(context);

    if let Some(path) = scripts.executable {
        InitScriptStatus::Found { path }
    } else {
        InitScriptStatus::NotFound {
            ignored: scripts.ignored,
        }
    }
}
