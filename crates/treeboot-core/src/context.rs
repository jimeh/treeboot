use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use crate::git::Git;
use crate::paths;
use crate::{Error, Result};

const TREEBOOT_ROOT_PATH: &str = "TREEBOOT_ROOT_PATH";
const CODEX_SOURCE_TREE_PATH: &str = "CODEX_SOURCE_TREE_PATH";
const CONDUCTOR_ROOT_PATH: &str = "CONDUCTOR_ROOT_PATH";
const SUPERSET_ROOT_PATH: &str = "SUPERSET_ROOT_PATH";
const CONDUCTOR_DEFAULT_BRANCH: &str = "CONDUCTOR_DEFAULT_BRANCH";
const TREEBOOT_STRICT: &str = "TREEBOOT_STRICT";
const TREEBOOT_DANGEROUSLY_ALLOW_SOURCES_OUTSIDE_ROOT: &str =
    "TREEBOOT_DANGEROUSLY_ALLOW_SOURCES_OUTSIDE_ROOT";
const TREEBOOT_DANGEROUSLY_ALLOW_TARGETS_OUTSIDE_WORKTREE: &str =
    "TREEBOOT_DANGEROUSLY_ALLOW_TARGETS_OUTSIDE_WORKTREE";

/// Environment variable map built for configured commands.
pub type Environment = BTreeMap<String, OsString>;

/// Explicit environment variable input used while resolving treeboot behavior.
///
/// This type only models the process environment variables that treeboot reads.
/// Unknown process environment variables are intentionally not captured.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnvironmentInput {
    /// Root checkout override from `TREEBOOT_ROOT_PATH`.
    pub treeboot_root_path: Option<OsString>,
    /// Root checkout compatibility override from `CODEX_SOURCE_TREE_PATH`.
    pub codex_source_tree_path: Option<OsString>,
    /// Root checkout compatibility override from `CONDUCTOR_ROOT_PATH`.
    pub conductor_root_path: Option<OsString>,
    /// Root checkout compatibility override from `SUPERSET_ROOT_PATH`.
    pub superset_root_path: Option<OsString>,
    /// Default branch compatibility override from `CONDUCTOR_DEFAULT_BRANCH`.
    pub conductor_default_branch: Option<OsString>,
    /// Runtime strict-mode override from `TREEBOOT_STRICT`.
    pub treeboot_strict: Option<OsString>,
    /// Runtime source-boundary override from
    /// `TREEBOOT_DANGEROUSLY_ALLOW_SOURCES_OUTSIDE_ROOT`.
    pub treeboot_dangerously_allow_sources_outside_root: Option<OsString>,
    /// Runtime target-boundary override from
    /// `TREEBOOT_DANGEROUSLY_ALLOW_TARGETS_OUTSIDE_WORKTREE`.
    pub treeboot_dangerously_allow_targets_outside_worktree: Option<OsString>,
}

impl EnvironmentInput {
    /// Returns an empty environment input.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            treeboot_root_path: None,
            codex_source_tree_path: None,
            conductor_root_path: None,
            superset_root_path: None,
            conductor_default_branch: None,
            treeboot_strict: None,
            treeboot_dangerously_allow_sources_outside_root: None,
            treeboot_dangerously_allow_targets_outside_worktree: None,
        }
    }

    /// Captures treeboot's known environment variables from the process.
    ///
    /// Empty values are captured as-is; lookup helpers ignore empty values to
    /// preserve the CLI compatibility behavior.
    #[must_use]
    pub fn from_process_env() -> Self {
        Self {
            treeboot_root_path: std::env::var_os(TREEBOOT_ROOT_PATH),
            codex_source_tree_path: std::env::var_os(CODEX_SOURCE_TREE_PATH),
            conductor_root_path: std::env::var_os(CONDUCTOR_ROOT_PATH),
            superset_root_path: std::env::var_os(SUPERSET_ROOT_PATH),
            conductor_default_branch: std::env::var_os(CONDUCTOR_DEFAULT_BRANCH),
            treeboot_strict: std::env::var_os(TREEBOOT_STRICT),
            treeboot_dangerously_allow_sources_outside_root: std::env::var_os(
                TREEBOOT_DANGEROUSLY_ALLOW_SOURCES_OUTSIDE_ROOT,
            ),
            treeboot_dangerously_allow_targets_outside_worktree: std::env::var_os(
                TREEBOOT_DANGEROUSLY_ALLOW_TARGETS_OUTSIDE_WORKTREE,
            ),
        }
    }

    /// Returns non-empty root path candidates in treeboot precedence order.
    pub fn root_candidates(&self) -> impl Iterator<Item = (&'static str, &OsStr)> {
        [
            (
                TREEBOOT_ROOT_PATH,
                non_empty_value(&self.treeboot_root_path),
            ),
            (
                CODEX_SOURCE_TREE_PATH,
                non_empty_value(&self.codex_source_tree_path),
            ),
            (
                CONDUCTOR_ROOT_PATH,
                non_empty_value(&self.conductor_root_path),
            ),
            (
                SUPERSET_ROOT_PATH,
                non_empty_value(&self.superset_root_path),
            ),
        ]
        .into_iter()
        .filter_map(|(name, value)| value.map(|value| (name, value)))
    }

    /// Returns the non-empty `CONDUCTOR_DEFAULT_BRANCH` value.
    #[must_use]
    pub fn conductor_default_branch(&self) -> Option<&OsStr> {
        non_empty_value(&self.conductor_default_branch)
    }
}

/// Options for discovering a Git worktree.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorktreeOptions {
    /// Directory from which discovery starts. Defaults to the process cwd.
    pub cwd: Option<PathBuf>,
    /// Overrides the root checkout used as the file-operation source.
    pub root: Option<PathBuf>,
    /// Explicit environment input used for compatibility discovery.
    pub environment: EnvironmentInput,
}

/// Resolved Git worktree metadata used by treeboot operations.
///
/// ```compile_fail
/// # use treeboot_core::Worktree;
/// let _ = Worktree {
///     root_path: "/repo".into(),
///     worktree_path: "/repo/worktree".into(),
///     default_branch: "main".into(),
///     environment: Default::default(),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
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
    /// Creates a resolved worktree context from supplied parts without
    /// performing discovery or filesystem validation.
    #[must_use]
    pub fn from_parts(
        root_path: PathBuf,
        worktree_path: PathBuf,
        default_branch: String,
        environment: Environment,
    ) -> Self {
        Self {
            root_path,
            worktree_path,
            default_branch,
            environment,
        }
    }

    /// Returns whether the selected worktree is the root checkout.
    #[must_use]
    pub fn is_root(&self) -> bool {
        self.root_path == self.worktree_path
    }

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

pub(crate) fn resolve(options: &WorktreeOptions) -> Result<Worktree> {
    let cwd = options.cwd.as_ref().map_or_else(
        || std::env::current_dir().map_err(|source| Error::CurrentDir { source }),
        |path| Ok(path.clone()),
    )?;
    let git = Git::new(&cwd);
    let worktree_path = normalize_existing_path(&git.worktree_path()?)?;
    let root_path = discover_root_path(options, &cwd, &git)?;
    let default_branch = discover_default_branch(options, &git)?;
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

    if let Some((_key, value)) = options.environment.root_candidates().next() {
        return normalize_existing_path(&resolve_input_path(cwd, &PathBuf::from(value)));
    }

    git.main_worktree_path()?
        .map(|path| normalize_existing_path(&path))
        .transpose()?
        .ok_or(Error::RootPathNotFound)
}

fn discover_default_branch(options: &WorktreeOptions, git: &Git) -> Result<String> {
    if let Some(branch) = options.environment.conductor_default_branch() {
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

fn non_empty_value(value: &Option<OsString>) -> Option<&OsStr> {
    value.as_deref().filter(|value| !value.is_empty())
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
    paths::canonicalize(path).map_err(|source| Error::NormalizePath {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_input_root_candidates_should_ignore_empty_values_in_order() {
        let environment = EnvironmentInput {
            treeboot_root_path: Some(OsString::new()),
            codex_source_tree_path: Some(OsString::from("/codex")),
            conductor_root_path: Some(OsString::from("/conductor")),
            superset_root_path: Some(OsString::from("/superset")),
            ..EnvironmentInput::empty()
        };

        let candidates = environment
            .root_candidates()
            .map(|(name, value)| (name, value.to_os_string()))
            .collect::<Vec<_>>();

        assert_eq!(
            candidates,
            vec![
                (CODEX_SOURCE_TREE_PATH, OsString::from("/codex")),
                (CONDUCTOR_ROOT_PATH, OsString::from("/conductor")),
                (SUPERSET_ROOT_PATH, OsString::from("/superset")),
            ]
        );
    }

    #[test]
    fn environment_input_conductor_default_branch_should_ignore_empty_value() {
        let environment = EnvironmentInput {
            conductor_default_branch: Some(OsString::new()),
            ..EnvironmentInput::empty()
        };

        assert_eq!(environment.conductor_default_branch(), None);
    }

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
