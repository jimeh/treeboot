use std::path::{Path, PathBuf};

use crate::context::resolve_worktree_path;
use crate::{Error, Result};

const CONFIG_PATHS: &[&str] = &[
    ".treeboot.toml",
    "treeboot.toml",
    ".config/treeboot/config.toml",
];

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
