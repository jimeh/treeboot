use std::path::{Path, PathBuf};

use crate::context::resolve_worktree_path;
use crate::{Error, Result};

const INIT_SCRIPT_PATHS: &[&str] = &[".treeboot.sh", ".treebootrc", ".config/treeboot/init"];
const CONFIG_PATHS: &[&str] = &[
    ".treeboot.toml",
    "treeboot.toml",
    ".config/treeboot/config.toml",
];

pub(crate) struct ScriptDiscovery {
    pub(crate) executable: Option<PathBuf>,
    pub(crate) ignored: Vec<PathBuf>,
}

pub(crate) fn discover_scripts(worktree_path: &Path) -> ScriptDiscovery {
    let mut ignored = Vec::new();

    for relative in INIT_SCRIPT_PATHS {
        let path = worktree_path.join(relative);

        if !path.is_file() {
            continue;
        }

        if is_executable(&path) {
            return ScriptDiscovery {
                executable: Some(path),
                ignored,
            };
        }

        ignored.push(path);
    }

    ScriptDiscovery {
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
