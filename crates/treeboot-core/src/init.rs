use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::context::resolve_worktree_path;
use crate::{Error, OutputEvent, Reporter, Result};

const DEFAULT_CONFIG_PATH: &str = ".treeboot.toml";

const STARTER_CONFIG: &str = r#"#:schema https://github.com/jimeh/treeboot/releases/latest/download/config.schema.json

copy = [
  ".env.local",
]

symlink = [
]

commands = [
]
"#;

/// Options for `treeboot init`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InitOptions {
    /// Directory in which the init target is created.
    pub cwd: Option<PathBuf>,
    /// Output path. Defaults to `.treeboot.toml`.
    pub path: Option<PathBuf>,
}

/// Result summary for `treeboot init`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitReport {
    /// Created path.
    pub path: PathBuf,
}

/// Creates a starter treeboot config.
///
/// Writes a starter config to the requested path, or to `.treeboot.toml` by
/// default.
///
/// # Errors
///
/// Returns an error if the current directory cannot be resolved, the target
/// already exists, or the target directory or file cannot be written.
pub fn init(options: InitOptions, reporter: &mut dyn Reporter) -> Result<InitReport> {
    let cwd = options.cwd.as_ref().map_or_else(
        || std::env::current_dir().map_err(|source| Error::CurrentDir { source }),
        |path| Ok(path.clone()),
    )?;
    let path = options
        .path
        .unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_PATH));
    let path = resolve_worktree_path(&cwd, &path);

    if target_exists(&path)? {
        return Err(Error::InitTargetExists(path));
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| Error::InitIo {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    std::fs::write(&path, STARTER_CONFIG).map_err(|source| Error::InitIo {
        path: path.clone(),
        source,
    })?;

    reporter
        .report(OutputEvent::InitCreated { path: path.clone() })
        .map_err(|source| Error::Output { source })?;

    Ok(InitReport { path })
}

fn target_exists(path: &Path) -> Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(source) if source.kind() == ErrorKind::NotFound => Ok(false),
        Err(source) => Err(Error::InitIo {
            path: path.to_path_buf(),
            source,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::symlink_file;

    use tempfile::TempDir;

    #[derive(Default)]
    struct VecReporter {
        events: Vec<OutputEvent>,
    }

    impl Reporter for VecReporter {
        fn report(&mut self, event: OutputEvent) -> std::io::Result<()> {
            self.events.push(event);
            Ok(())
        }
    }

    #[test]
    fn init_should_refuse_existing_file() {
        let dir = TempDir::new().expect("tempdir should be created");
        let config = dir.path().join(".treeboot.toml");
        std::fs::write(&config, "old\n").expect("config should be written");
        let mut reporter = VecReporter::default();

        let err = init(
            InitOptions {
                cwd: Some(dir.path().to_path_buf()),
                path: None,
            },
            &mut reporter,
        )
        .expect_err("existing target should be rejected");

        match err {
            Error::InitTargetExists(path) => assert_eq!(path, config),
            other => panic!("expected InitTargetExists, got {other:?}"),
        }
        assert_eq!(
            std::fs::read_to_string(config).expect("config should be readable"),
            "old\n"
        );
        assert!(reporter.events.is_empty());
    }

    #[test]
    fn init_should_refuse_existing_symlink_without_writing_through_it() {
        let dir = TempDir::new().expect("tempdir should be created");
        let target = dir.path().join("target.toml");
        let link = dir.path().join(".treeboot.toml");
        std::fs::write(&target, "old\n").expect("target should be written");
        symlink_file(&target, &link).expect("symlink should be created");
        let mut reporter = VecReporter::default();

        let err = init(
            InitOptions {
                cwd: Some(dir.path().to_path_buf()),
                path: None,
            },
            &mut reporter,
        )
        .expect_err("existing symlink should be rejected");

        match err {
            Error::InitTargetExists(path) => assert_eq!(path, link),
            other => panic!("expected InitTargetExists, got {other:?}"),
        }
        assert_eq!(
            std::fs::read_to_string(target).expect("target should be readable"),
            "old\n"
        );
        assert!(
            std::fs::symlink_metadata(link)
                .expect("link metadata should load")
                .file_type()
                .is_symlink()
        );
        assert!(reporter.events.is_empty());
    }
}
