use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::context::{TREEBOOT_WORKTREE_ID, TREEBOOT_WORKTREE_SLUG};
use crate::env::resolve_effective_context;
use crate::git::Git;
use crate::paths;
use crate::{Config, EnvOptions, EnvironmentInput, Error, Result, WorktreeOptions};

/// Options for inspecting one worktree identity.
///
/// Construct options through [`WorktreeIdentityOptions::default`] so future
/// additive fields remain source-compatible.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct WorktreeIdentityOptions {
    /// Process directory used to resolve a relative explicit path, or the
    /// discovery start when no explicit path is supplied.
    pub cwd: Option<PathBuf>,
    /// Exact target path whose identity should be derived.
    pub path: Option<PathBuf>,
    /// Explicit environment input used by implicit Git discovery. Explicit
    /// path inspection intentionally ignores these compatibility overrides.
    pub environment: EnvironmentInput,
}

/// Options for inspecting worktree identities in the current repository.
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

/// Result of inspecting a worktree ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct WorktreeIdReport {
    /// Complete effective worktree ID.
    pub id: String,
}

/// Result of inspecting a worktree slug.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct WorktreeSlugReport {
    /// Complete effective readable worktree slug.
    pub slug: String,
}

/// Identity and canonical path of one registered worktree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct WorktreeEntry {
    /// Complete effective worktree ID.
    pub id: String,
    /// Complete effective readable worktree slug.
    pub slug: String,
    /// Canonical absolute worktree path.
    pub path: PathBuf,
}

/// Result of resolving a worktree ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct WorktreePathReport {
    /// Exact ID that was resolved.
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

#[derive(Debug)]
struct WorktreeIdentity {
    id: String,
    slug: String,
}

/// Inspects the current or explicit target's config-refined ID.
///
/// # Errors
///
/// Returns an error if implicit worktree discovery, explicit path
/// normalization, or full config parsing fails.
pub fn inspect_worktree_id(options: WorktreeIdentityOptions) -> Result<WorktreeIdReport> {
    let context = resolve_identity_context(options)?;
    Ok(WorktreeIdReport {
        id: identity_from_context(&context)?.id,
    })
}

/// Inspects the current or explicit target's config-refined slug.
///
/// # Errors
///
/// Returns an error if implicit worktree discovery, explicit path
/// normalization, or full config parsing fails.
pub fn inspect_worktree_slug(options: WorktreeIdentityOptions) -> Result<WorktreeSlugReport> {
    let context = resolve_identity_context(options)?;
    Ok(WorktreeSlugReport {
        slug: identity_from_context(&context)?.slug,
    })
}

/// Lists every registered, existing, non-bare worktree and its effective
/// identity.
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
        let identity =
            identity_from_context(&context).map_err(|error| candidate_error(&path, error))?;
        let entry = WorktreeEntry {
            id: identity.id,
            slug: identity.slug,
            path,
        };

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

/// Resolves an exact effective ID to one registered worktree path.
///
/// Every candidate is inspected before the result is selected so ID
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

fn resolve_identity_context(options: WorktreeIdentityOptions) -> Result<crate::Worktree> {
    let Some(input_path) = options.path else {
        return resolve_effective_context(EnvOptions {
            cwd: options.cwd,
            environment: options.environment,
            ..EnvOptions::default()
        });
    };
    let cwd = options.cwd.map_or_else(
        || std::env::current_dir().map_err(|source| Error::CurrentDir { source }),
        Ok,
    )?;
    let input_path = paths::resolve_path(&cwd, &input_path).map_err(|source| {
        Error::WorktreeIdentityUnsupportedPath {
            path: input_path,
            reason: source.reason(),
        }
    })?;
    let path =
        paths::normalize_maybe_existing(&input_path).map_err(|source| Error::NormalizePath {
            path: input_path,
            source,
        })?;
    match std::fs::symlink_metadata(&path) {
        Ok(_) if !path.is_dir() => {
            return Err(Error::WorktreeIdentityPathNotDirectory { path });
        }
        Ok(_) => {}
        Err(source) if source.kind() == ErrorKind::NotFound => {}
        Err(source) => {
            return Err(Error::NormalizePath { path, source });
        }
    }

    let identity_fallback_path = path
        .ancestors()
        .find(|candidate| candidate.is_dir())
        .and_then(|candidate| {
            Git::new(candidate)
                .main_worktree_path()
                .ok()
                .flatten()
                .and_then(|path| paths::canonicalize(&path).ok())
        });
    let context = crate::context::for_identity_path(path, identity_fallback_path);
    Ok(Config::load_discovered(&context, None)?.map_or(context, |loaded| loaded.context))
}

fn identity_from_context(context: &crate::Worktree) -> Result<WorktreeIdentity> {
    let id = context
        .environment
        .get(TREEBOOT_WORKTREE_ID)
        .map(|id| id.to_string_lossy().into_owned())
        .ok_or(Error::WorktreeIdMissing)?;
    let slug = context
        .environment
        .get(TREEBOOT_WORKTREE_SLUG)
        .map(|slug| slug.to_string_lossy().into_owned())
        .ok_or(Error::WorktreeSlugMissing)?;
    Ok(WorktreeIdentity { id, slug })
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
            identity_from_context(&context).expect_err("missing managed ID should be an error");

        assert!(matches!(error, Error::WorktreeIdMissing));
    }

    #[test]
    fn identity_resolution_should_error_when_managed_slug_is_missing() {
        let context = crate::Worktree::from_parts(
            "/repo".into(),
            "/repo/linked".into(),
            "main".to_owned(),
            crate::Environment::from([(
                TREEBOOT_WORKTREE_ID.to_owned(),
                std::ffi::OsString::from("a1b2c3"),
            )]),
        );

        let error =
            identity_from_context(&context).expect_err("missing managed slug should be an error");

        assert!(matches!(error, Error::WorktreeSlugMissing));
    }
}
