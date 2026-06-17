use std::path::PathBuf;

use crate::context::resolve_worktree_path;
use crate::{Error, OutputEvent, Reporter, Result};

const DEFAULT_CONFIG_PATH: &str = ".treeboot.toml";
const DEFAULT_SCRIPT_PATH: &str = ".treeboot.sh";

const STARTER_CONFIG: &str = r#"strict = false
dangerously_allow_sources_outside_root = false
dangerously_allow_targets_outside_worktree = false

copy = [
  ".env",
]

symlink = [
]

commands = [
]
"#;

const STARTER_SCRIPT: &str = r#"#!/usr/bin/env sh
set -eu

root_path="$1"

printf 'treeboot root: %s\n' "$root_path"
"#;

/// Init file type to create.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitKind {
    /// Create a starter TOML config.
    Config,
    /// Create an executable init script.
    Script,
}

/// Options for `treeboot init`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InitOptions {
    /// Directory in which the init target is created.
    pub cwd: Option<PathBuf>,
    /// Init file type to create.
    pub kind: Option<InitKind>,
    /// Output path. Defaults depend on the selected kind.
    pub path: Option<PathBuf>,
    /// Replace an existing output file.
    pub force: bool,
}

/// Result summary for `treeboot init`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitReport {
    /// Init file type that was created.
    pub kind: InitKind,
    /// Created path.
    pub path: PathBuf,
}

/// Creates a starter treeboot config or init script.
///
/// Writes the selected init artifact to the requested path, or to the default
/// path for its kind. Script artifacts are marked executable on Unix.
///
/// # Errors
///
/// Returns an error if the current directory cannot be resolved, no init kind
/// was selected, both init kinds were selected, the target already exists
/// without `force`, or the target directory or file cannot be written.
pub fn init(options: InitOptions, reporter: &mut dyn Reporter) -> Result<InitReport> {
    let cwd = options.cwd.as_ref().map_or_else(
        || std::env::current_dir().map_err(|source| Error::CurrentDir { source }),
        |path| Ok(path.clone()),
    )?;
    let kind = options.kind.ok_or(Error::InitTypeRequired)?;
    let path = options.path.unwrap_or_else(|| default_path(kind));
    let path = resolve_worktree_path(&cwd, &path);

    if path.exists() && !options.force {
        return Err(Error::InitTargetExists(path));
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| Error::InitIo {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let content = match kind {
        InitKind::Config => STARTER_CONFIG,
        InitKind::Script => STARTER_SCRIPT,
    };

    std::fs::write(&path, content).map_err(|source| Error::InitIo {
        path: path.clone(),
        source,
    })?;

    if kind == InitKind::Script {
        make_executable(&path)?;
    }

    reporter
        .report(OutputEvent::InitCreated { path: path.clone() })
        .map_err(|source| Error::Output { source })?;

    Ok(InitReport { kind, path })
}

fn default_path(kind: InitKind) -> PathBuf {
    match kind {
        InitKind::Config => PathBuf::from(DEFAULT_CONFIG_PATH),
        InitKind::Script => PathBuf::from(DEFAULT_SCRIPT_PATH),
    }
}

#[cfg(unix)]
fn make_executable(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = std::fs::metadata(path)
        .map_err(|source| Error::InitIo {
            path: path.to_path_buf(),
            source,
        })?
        .permissions();
    permissions.set_mode(permissions.mode() | 0o111);
    std::fs::set_permissions(path, permissions).map_err(|source| Error::InitIo {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(not(unix))]
fn make_executable(_path: &std::path::Path) -> Result<()> {
    Ok(())
}
