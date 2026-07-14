use std::path::PathBuf;

use serde::Serialize;

use crate::check::WorktreeSnapshot;
use crate::context;
use crate::{Config, EnvironmentInput, Result, Worktree, WorktreeOptions};

/// Options for inspecting treeboot discovery status.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatusOptions {
    /// Directory from which status discovery starts.
    pub cwd: Option<PathBuf>,
    /// Overrides the root checkout used for discovery.
    pub root: Option<PathBuf>,
    /// Explicit environment input used for compatibility discovery.
    pub environment: EnvironmentInput,
    /// Uses one specific config file instead of config discovery.
    pub config: Option<PathBuf>,
}

/// Result summary for a `treeboot status` invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusReport {
    /// Runtime context discovered for the current worktree.
    pub context: Worktree,
    /// Selected config path, when one was requested or discovered.
    pub config: Option<PathBuf>,
}

/// Serializable result summary for a `treeboot status` invocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatusSnapshotReport {
    /// Runtime context snapshot discovered for the current worktree.
    pub context: WorktreeSnapshot,
    /// Selected config path, when one was requested or discovered.
    pub config: Option<PathBuf>,
}

impl From<&StatusReport> for StatusSnapshotReport {
    fn from(report: &StatusReport) -> Self {
        Self {
            context: WorktreeSnapshot::from(&report.context),
            config: report.config.clone(),
        }
    }
}

/// Inspects worktree, root, and config discovery status.
///
/// This function does not parse config or run configured commands.
///
/// # Errors
///
/// Returns an error if context discovery fails or a requested config file does
/// not exist.
pub fn inspect_status(options: StatusOptions) -> Result<StatusReport> {
    let context = context::resolve(&WorktreeOptions {
        cwd: options.cwd,
        root: options.root,
        environment: options.environment,
    })?;
    let config = Config::discover_path(&context, options.config.as_deref())?;

    Ok(StatusReport { context, config })
}

/// Inspects worktree, root, and config discovery status as a serializable
/// snapshot.
///
/// This function does not parse config or run configured commands.
///
/// # Errors
///
/// Returns an error if context discovery fails or a requested config file does
/// not exist.
pub fn inspect_status_snapshot(options: StatusOptions) -> Result<StatusSnapshotReport> {
    inspect_status(options).map(|report| StatusSnapshotReport::from(&report))
}
