use std::path::{Component, Path, PathBuf};

pub(crate) fn canonicalize(path: &Path) -> std::io::Result<PathBuf> {
    dunce::canonicalize(path)
}

pub(crate) fn normalize_maybe_existing(path: &Path) -> std::io::Result<PathBuf> {
    match canonicalize(path) {
        Ok(path) => return Ok(path),
        Err(source) if !missing_path_error(&source) => {
            return Err(source);
        }
        Err(_) => {}
    }

    let mut ancestor = path;

    loop {
        match ancestor.try_exists() {
            Ok(true) => break,
            Ok(false) => {}
            Err(source) if missing_path_error(&source) => {}
            Err(source) => return Err(source),
        }

        let Some(parent) = ancestor.parent() else {
            return Ok(normalize_lexical(path));
        };
        if parent == ancestor {
            return Ok(normalize_lexical(path));
        }
        ancestor = parent;
    }

    let suffix = path
        .strip_prefix(ancestor)
        .map_err(|source| std::io::Error::new(std::io::ErrorKind::InvalidInput, source))?;
    let mut normalized = canonicalize(ancestor)?;
    normalized.push(suffix);

    Ok(normalize_lexical(&normalized))
}

/// Returns whether an I/O error means the inspected path cannot exist.
///
/// Windows rejects paths containing glob metacharacters such as `*` with
/// `ERROR_INVALID_NAME` instead of a not-found error. Such paths normalize
/// lexically like missing paths.
pub(crate) fn missing_path_error(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::NotFound | std::io::ErrorKind::InvalidFilename
    )
}

pub(crate) fn normalize_lexical(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() && !normalized.has_root() {
                    normalized.push(component.as_os_str());
                }
            }
            Component::Normal(part) => normalized.push(part),
        }
    }

    normalized
}

pub(crate) fn resolve_path(base: &Path, path: &Path) -> Result<PathBuf, UnsupportedPath> {
    reject_unsupported_path(path)?;

    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(base.join(path))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UnsupportedPath {
    reason: &'static str,
}

impl UnsupportedPath {
    pub(crate) const fn reason(self) -> &'static str {
        self.reason
    }
}

fn reject_unsupported_path(path: &Path) -> Result<(), UnsupportedPath> {
    if let Some(reason) = unsupported_windows_path_reason(path) {
        return Err(UnsupportedPath { reason });
    }

    Ok(())
}

#[cfg(windows)]
fn unsupported_windows_path_reason(path: &Path) -> Option<&'static str> {
    let mut components = path.components();
    match components.next() {
        Some(Component::Prefix(_)) if !path.is_absolute() => {
            Some("drive-relative paths are not supported")
        }
        Some(Component::RootDir) => {
            Some("root-relative paths without a drive or share are not supported")
        }
        _ => None,
    }
}

#[cfg(not(windows))]
const fn unsupported_windows_path_reason(_path: &Path) -> Option<&'static str> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_lexical_should_resolve_parent_components() {
        assert_eq!(
            normalize_lexical(Path::new("/repo/worktree/../outside")),
            PathBuf::from("/repo/outside")
        );
    }

    #[test]
    fn normalize_maybe_existing_should_preserve_missing_parent_components() {
        let temp = tempfile::TempDir::new().expect("tempdir should be created");
        let base = temp.path().join("existing");
        std::fs::create_dir_all(&base).expect("existing ancestor should be created");
        let path = base.join("missing").join("..").join("target");

        let normalized =
            normalize_maybe_existing(&path).expect("path should normalize through ancestor");

        assert_eq!(
            normalized,
            canonicalize(&base)
                .expect("base should canonicalize")
                .join("target")
        );
    }

    #[cfg(windows)]
    #[test]
    fn normalize_maybe_existing_should_treat_invalid_names_as_missing() {
        let base = std::env::temp_dir();
        let path = base.join("treeboot-paths-invalid").join("certs/*.pem");

        let normalized =
            normalize_maybe_existing(&path).expect("glob metacharacters should normalize");

        assert!(normalized.ends_with(Path::new("certs/*.pem")));
    }

    #[cfg(windows)]
    #[test]
    fn resolve_path_should_reject_drive_relative_windows_paths() {
        let error = resolve_path(Path::new(r"C:\repo"), Path::new(r"C:relative"))
            .expect_err("drive-relative path should fail");

        assert_eq!(error.reason(), "drive-relative paths are not supported");
    }

    #[cfg(windows)]
    #[test]
    fn resolve_path_should_reject_root_relative_windows_paths() {
        let error = resolve_path(Path::new(r"C:\repo"), Path::new(r"\relative"))
            .expect_err("root-relative path should fail");

        assert_eq!(
            error.reason(),
            "root-relative paths without a drive or share are not supported"
        );
    }
}
