use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::context::resolve_worktree_path;
use crate::{Error, Result, Worktree};

const INIT_SCRIPT_PATHS: &[&str] = &[".treeboot.sh", ".treebootrc", ".config/treeboot/init"];
const IGNORE_REASON_NOT_EXECUTABLE: &str = "not_executable";
const CONFIG_PATHS: &[&str] = &[
    ".treeboot.toml",
    "treeboot.toml",
    ".config/treeboot/config.toml",
];

/// Discovered treeboot init scripts for a worktree.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InitScriptDiscovery {
    /// First executable init script found in treeboot precedence order.
    pub executable: Option<PathBuf>,
    /// Existing init script paths that were ignored.
    pub ignored: Vec<IgnoredInitScript>,
}

impl InitScriptDiscovery {
    /// Discovers executable and ignored init scripts for a worktree.
    #[must_use]
    pub fn discover(context: &Worktree) -> Self {
        discover_scripts(&context.worktree_path)
    }
}

/// Existing init script path that treeboot skipped during discovery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IgnoredInitScript {
    /// Script candidate path.
    pub path: PathBuf,
    /// Stable ignore reason.
    ///
    /// The initial reason is `not_executable`.
    pub reason: &'static str,
}

pub(crate) fn discover_scripts(worktree_path: &Path) -> InitScriptDiscovery {
    let mut ignored = Vec::new();

    for relative in INIT_SCRIPT_PATHS {
        let path = worktree_path.join(relative);

        if !path.is_file() {
            continue;
        }

        if is_executable(&path) {
            return InitScriptDiscovery {
                executable: Some(path),
                ignored,
            };
        }

        ignored.push(IgnoredInitScript {
            path,
            reason: IGNORE_REASON_NOT_EXECUTABLE,
        });
    }

    InitScriptDiscovery {
        executable: None,
        ignored,
    }
}

pub(crate) fn discover_config(
    worktree_path: &Path,
    requested_config: Option<&Path>,
) -> Result<Option<PathBuf>> {
    if let Some(path) = requested_config {
        let path = resolve_worktree_path(worktree_path, path);

        if path.is_file() {
            return Ok(Some(path));
        }

        return Err(Error::ConfigNotFound(path));
    }

    Ok(CONFIG_PATHS
        .iter()
        .map(|relative| worktree_path.join(relative))
        .find(|path| path.is_file()))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    path.metadata()
        .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}
