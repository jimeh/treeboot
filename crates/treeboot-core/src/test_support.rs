//! Shared cross-platform helpers for unit tests.
//!
//! Symlink creation differs by platform, so tests route through these helpers
//! to keep one test body across platforms instead of duplicating per-OS
//! variants. The CLI integration tests have an equivalent set in
//! `crates/treeboot/tests/common/mod.rs`.

#![allow(dead_code)]

use std::path::Path;

/// Creates a file symlink at `link` pointing to `target`, using the
/// platform-appropriate API. Accepts the same argument shapes as
/// `std::os::unix::fs::symlink` so call sites convert as a plain rename.
pub(crate) fn symlink_file(
    target: impl AsRef<Path>,
    link: impl AsRef<Path>,
) -> std::io::Result<()> {
    let (target, link) = (target.as_ref(), link.as_ref());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_file(target, link)
    }
}

/// Creates a directory symlink at `link` pointing to `target`, using the
/// platform-appropriate API. On Windows a directory symlink must be created
/// with the directory-specific call, so pick this when the target is a
/// directory.
pub(crate) fn symlink_dir(target: impl AsRef<Path>, link: impl AsRef<Path>) -> std::io::Result<()> {
    let (target, link) = (target.as_ref(), link.as_ref());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_dir(target, link)
    }
}

/// Reports whether this process can create symlinks. Windows requires privilege
/// or Developer Mode, so symlink tests skip rather than fail when this returns
/// `false`. The probe runs once and is cached for the test run.
pub(crate) fn symlinks_supported() -> bool {
    static SUPPORTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *SUPPORTED.get_or_init(|| {
        let Ok(dir) = tempfile::TempDir::new() else {
            return false;
        };
        let target = dir.path().join("probe-target");
        if std::fs::write(&target, b"probe").is_err() {
            return false;
        }
        symlink_file(&target, dir.path().join("probe-link")).is_ok()
    })
}

/// Returns `true` when symlinks are unsupported, after printing a skip notice so
/// CI logs distinguish a skipped symlink test from one that ran. Call at the top
/// of a symlink test and early-return when it returns `true`.
pub(crate) fn skip_without_symlinks(test: &str) -> bool {
    if symlinks_supported() {
        return false;
    }
    eprintln!("skipping {test}: platform cannot create symlinks");
    true
}
