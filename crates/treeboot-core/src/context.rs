use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::git::Git;
use crate::{Error, Result};

const ROOT_ENV_KEYS: &[&str] = &[
    "TREEBOOT_ROOT_PATH",
    "CODEX_SOURCE_TREE_PATH",
    "CONDUCTOR_ROOT_PATH",
    "SUPERSET_ROOT_PATH",
];

/// Environment variable map built for scripts and configured commands.
pub type Environment = BTreeMap<String, OsString>;

/// Options for discovering a Git worktree.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorktreeOptions {
    /// Directory from which discovery starts. Defaults to the process cwd.
    pub cwd: Option<PathBuf>,
    /// Overrides the root checkout used as the file-operation source.
    pub root: Option<PathBuf>,
}

/// Resolved Git worktree metadata used by treeboot operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    /// Source checkout used for file operations.
    pub root_path: PathBuf,
    /// Current worktree root where targets and commands are anchored.
    pub worktree_path: PathBuf,
    /// Best-effort default branch name.
    pub default_branch: String,
    /// Canonical treeboot variables and compatibility aliases.
    pub environment: Environment,
}

impl Worktree {
    /// Discovers worktree metadata from the provided options.
    ///
    /// # Errors
    ///
    /// Returns an error if the current directory cannot be read, the directory
    /// is not inside a Git worktree, Git discovery fails, or no root checkout
    /// path can be determined.
    pub fn discover(options: WorktreeOptions) -> Result<Self> {
        resolve(&options)
    }
}

/// Backwards-compatible name for resolved worktree metadata.
pub type RunContext = Worktree;

pub(crate) fn resolve(options: &WorktreeOptions) -> Result<Worktree> {
    let cwd = options.cwd.as_ref().map_or_else(
        || std::env::current_dir().map_err(|source| Error::CurrentDir { source }),
        |path| Ok(path.clone()),
    )?;
    let git = Git::new(&cwd);
    let worktree_path = normalize_existing_path(&git.worktree_path()?)?;
    let root_path = discover_root_path(options, &cwd, &git)?;
    let default_branch = discover_default_branch(&git)?;
    let environment = build_environment(&root_path, &worktree_path, &default_branch);

    Ok(Worktree {
        root_path,
        worktree_path,
        default_branch,
        environment,
    })
}

fn discover_root_path(options: &WorktreeOptions, cwd: &Path, git: &Git) -> Result<PathBuf> {
    if let Some(path) = &options.root {
        return normalize_existing_path(&resolve_input_path(cwd, path));
    }

    for key in ROOT_ENV_KEYS {
        if let Some(value) = non_empty_env(key) {
            return normalize_existing_path(&resolve_input_path(cwd, &PathBuf::from(value)));
        }
    }

    git.main_worktree_path()?
        .map(|path| normalize_existing_path(&path))
        .transpose()?
        .ok_or(Error::RootPathNotFound)
}

fn discover_default_branch(git: &Git) -> Result<String> {
    if let Some(branch) = non_empty_env("CONDUCTOR_DEFAULT_BRANCH") {
        return Ok(branch.to_string_lossy().into_owned());
    }

    git.default_branch()
}

fn build_environment(root_path: &Path, worktree_path: &Path, default_branch: &str) -> Environment {
    let root = root_path.as_os_str().to_os_string();
    let worktree = worktree_path.as_os_str().to_os_string();
    let branch = OsString::from(default_branch);

    let mut env = Environment::new();
    env.insert("TREEBOOT_ROOT_PATH".to_owned(), root.clone());
    env.insert("TREEBOOT_WORKTREE_PATH".to_owned(), worktree.clone());
    env.insert("TREEBOOT_DEFAULT_BRANCH".to_owned(), branch.clone());
    env.insert("GIT_SOURCE_TREE_PATH".to_owned(), root.clone());
    env.insert("GIT_WORKTREE_PATH".to_owned(), worktree.clone());
    env.insert("CODEX_SOURCE_TREE_PATH".to_owned(), root.clone());
    env.insert("CODEX_WORKTREE_PATH".to_owned(), worktree.clone());
    env.insert("CONDUCTOR_ROOT_PATH".to_owned(), root.clone());
    env.insert("CONDUCTOR_WORKSPACE_PATH".to_owned(), worktree);
    env.insert("CONDUCTOR_DEFAULT_BRANCH".to_owned(), branch);
    env.insert("SUPERSET_ROOT_PATH".to_owned(), root);
    env
}

fn non_empty_env(key: &str) -> Option<OsString> {
    std::env::var_os(key).filter(|value| !value.is_empty())
}

pub(crate) fn resolve_worktree_path(worktree_path: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        worktree_path.join(path)
    }
}

fn resolve_input_path(cwd: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

fn normalize_existing_path(path: &Path) -> Result<PathBuf> {
    std::fs::canonicalize(path).map_err(|source| Error::NormalizePath {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_environment_should_set_codex_worktree_to_worktree_path() {
        let root = Path::new("/repo");
        let worktree = Path::new("/repo-worktree");
        let env = build_environment(root, worktree, "main");

        assert_eq!(
            env.get("CODEX_WORKTREE_PATH"),
            Some(&OsString::from("/repo-worktree"))
        );
    }

    #[test]
    fn resolve_worktree_path_should_join_relative_paths() {
        let worktree = Path::new("/repo-worktree");

        assert_eq!(
            resolve_worktree_path(worktree, Path::new(".env")),
            PathBuf::from("/repo-worktree/.env")
        );
    }

    #[test]
    fn resolve_worktree_path_should_keep_absolute_paths() {
        let worktree = Path::new("/repo-worktree");

        assert_eq!(
            resolve_worktree_path(worktree, Path::new("/tmp/.env")),
            PathBuf::from("/tmp/.env")
        );
    }
}
