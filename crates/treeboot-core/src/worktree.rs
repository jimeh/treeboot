use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::context::TREEBOOT_WORKTREE_ID;
use crate::env::resolve_effective_context;
use crate::git::Git;
use crate::paths;
use crate::{EnvOptions, EnvironmentInput, Error, Result, WorktreeOptions};

/// Options for inspecting worktree identifiers in the current repository.
///
/// Construct options through [`WorktreeInspectionOptions::default`] so future
/// additive fields remain source-compatible.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct WorktreeInspectionOptions {
    /// Directory from which repository discovery starts.
    pub cwd: Option<PathBuf>,
    /// Explicit environment input used for compatibility discovery.
    pub environment: EnvironmentInput,
}

/// Result of inspecting the current worktree identifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct WorktreeIdReport {
    /// Complete effective worktree identifier.
    pub id: String,
}

/// Identifier and canonical path of one registered worktree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct WorktreeEntry {
    /// Complete effective worktree identifier.
    pub id: String,
    /// Canonical absolute worktree path.
    pub path: PathBuf,
}

/// Result of resolving a worktree identifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct WorktreePathReport {
    /// Exact identifier that was resolved.
    pub id: String,
    /// Canonical absolute path of the matching worktree.
    pub path: PathBuf,
}

/// Inventory of registered, existing, non-bare worktrees.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct WorktreeListReport {
    /// Worktrees ordered with the main worktree first, then by canonical path.
    pub worktrees: Vec<WorktreeEntry>,
}

/// Inspects the current worktree's config-refined identifier.
///
/// # Errors
///
/// Returns an error if worktree discovery or full config parsing fails.
pub fn inspect_worktree_id(options: WorktreeInspectionOptions) -> Result<WorktreeIdReport> {
    let context = resolve_effective_context(env_options(&options, options.cwd.clone()))?;
    Ok(WorktreeIdReport {
        id: identifier_from_context(&context)?,
    })
}

/// Lists every registered, existing, non-bare worktree and its effective ID.
///
/// # Errors
///
/// Returns an error if repository discovery fails or any existing candidate
/// cannot be canonicalized, discovered, or fully parsed.
pub fn inspect_worktree_list(options: WorktreeInspectionOptions) -> Result<WorktreeListReport> {
    let cwd = options.cwd.clone().map_or_else(
        || std::env::current_dir().map_err(|source| Error::CurrentDir { source }),
        Ok,
    )?;
    // Resolve only Git context here. Config is loaded below for each candidate
    // so every failure carries candidate attribution, including the caller.
    crate::context::resolve(&WorktreeOptions {
        cwd: Some(cwd.clone()),
        root: None,
        environment: EnvironmentInput::empty(),
    })?;
    let candidates = Git::new(&cwd).worktrees()?;
    let mut main = None;
    let mut linked = Vec::new();

    for candidate in candidates.into_iter().filter(|candidate| !candidate.bare) {
        let path = match paths::canonicalize(&candidate.path) {
            Ok(path) => path,
            Err(source) if source.kind() == ErrorKind::NotFound => continue,
            Err(source) => {
                return Err(candidate_error(
                    &candidate.path,
                    Error::NormalizePath {
                        path: candidate.path.clone(),
                        source,
                    },
                ));
            }
        };
        let context = match resolve_effective_context(env_options(&options, Some(path.clone()))) {
            Ok(context) => context,
            Err(error) if candidate_disappeared(&path, &error) => continue,
            Err(error) => return Err(candidate_error(&path, error)),
        };
        let id =
            identifier_from_context(&context).map_err(|error| candidate_error(&path, error))?;
        let entry = WorktreeEntry { id, path };

        if candidate.main {
            main = Some(entry);
        } else {
            linked.push(entry);
        }
    }

    linked.sort_by(|left, right| left.path.cmp(&right.path));
    let mut worktrees = Vec::with_capacity(linked.len() + usize::from(main.is_some()));
    if let Some(main) = main {
        worktrees.push(main);
    }
    worktrees.extend(linked);

    Ok(WorktreeListReport { worktrees })
}

/// Resolves an exact effective identifier to one registered worktree path.
///
/// Every candidate is inspected before the result is selected so identifier
/// collisions cannot be hidden by enumeration order.
///
/// # Errors
///
/// Returns a typed error when no worktree matches, multiple worktrees match, or
/// repository or candidate inspection fails.
pub fn inspect_worktree_path(
    id: &str,
    options: WorktreeInspectionOptions,
) -> Result<WorktreePathReport> {
    let report = inspect_worktree_list(options)?;
    let paths = report
        .worktrees
        .into_iter()
        .filter(|entry| entry.id == id)
        .map(|entry| entry.path)
        .collect::<Vec<_>>();

    match paths.as_slice() {
        [] => Err(Error::WorktreeIdNotFound { id: id.to_owned() }),
        [path] => Ok(WorktreePathReport {
            id: id.to_owned(),
            path: path.clone(),
        }),
        _ => Err(Error::WorktreeIdAmbiguous {
            id: id.to_owned(),
            paths,
        }),
    }
}

fn env_options(options: &WorktreeInspectionOptions, cwd: Option<PathBuf>) -> EnvOptions {
    EnvOptions {
        cwd,
        environment: options.environment.clone(),
        ..EnvOptions::default()
    }
}

fn identifier_from_context(context: &crate::Worktree) -> Result<String> {
    context
        .environment
        .get(TREEBOOT_WORKTREE_ID)
        .map(|id| id.to_string_lossy().into_owned())
        .ok_or(Error::WorktreeIdMissing)
}

fn candidate_error(path: &Path, source: Error) -> Error {
    Error::WorktreeInspection {
        path: path.to_path_buf(),
        source: Box::new(source),
    }
}

fn is_not_found(error: &Error) -> bool {
    match error {
        Error::NormalizePath { source, .. }
        | Error::ConfigIo { source, .. }
        | Error::CurrentDir { source }
        | Error::GitIo { source, .. } => source.kind() == ErrorKind::NotFound,
        Error::WorktreeInspection { source, .. } => is_not_found(source),
        _ => false,
    }
}

fn candidate_disappeared(path: &Path, error: &Error) -> bool {
    if !is_not_found(error) {
        return false;
    }

    matches!(path.try_exists(), Ok(false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_found_classification_should_only_skip_not_found_io_errors() {
        assert!(is_not_found(&Error::NormalizePath {
            path: "/missing".into(),
            source: std::io::Error::from(ErrorKind::NotFound),
        }));
        assert!(!is_not_found(&Error::NormalizePath {
            path: "/denied".into(),
            source: std::io::Error::from(ErrorKind::PermissionDenied),
        }));
        assert!(is_not_found(&Error::ConfigIo {
            path: "/missing/.treeboot.toml".into(),
            source: std::io::Error::from(ErrorKind::NotFound),
        }));
        assert!(is_not_found(&Error::CurrentDir {
            source: std::io::Error::from(ErrorKind::NotFound),
        }));
        assert!(is_not_found(&Error::GitIo {
            command: "git status".to_owned(),
            source: std::io::Error::from(ErrorKind::NotFound),
        }));
        assert!(is_not_found(&Error::WorktreeInspection {
            path: "/missing".into(),
            source: Box::new(Error::NormalizePath {
                path: "/missing".into(),
                source: std::io::Error::from(ErrorKind::NotFound),
            }),
        }));
        assert!(!is_not_found(&Error::NotGitWorktree));
    }

    #[test]
    fn candidate_disappearance_should_require_a_missing_candidate_path() {
        let temp = tempfile::TempDir::new().expect("tempdir should be created");
        let existing = temp.path().join("existing");
        std::fs::create_dir(&existing).expect("existing candidate should be created");
        let missing = temp.path().join("missing");
        let error = Error::NormalizePath {
            path: missing.clone(),
            source: std::io::Error::from(ErrorKind::NotFound),
        };

        assert!(!candidate_disappeared(&existing, &error));
        assert!(candidate_disappeared(&missing, &error));
        assert!(!candidate_disappeared(&missing, &Error::NotGitWorktree));
    }

    #[test]
    fn identifier_resolution_should_error_when_managed_value_is_missing() {
        let context = crate::Worktree::from_parts(
            "/repo".into(),
            "/repo/linked".into(),
            "main".to_owned(),
            crate::Environment::new(),
        );

        let error =
            identifier_from_context(&context).expect_err("missing managed ID should be an error");

        assert!(matches!(error, Error::WorktreeIdMissing));
    }
}
