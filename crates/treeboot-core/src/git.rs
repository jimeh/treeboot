use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use crate::{Error, Result};

pub(crate) struct Git {
    cwd: PathBuf,
}

impl Git {
    pub(crate) fn new(cwd: &Path) -> Self {
        Self {
            cwd: cwd.to_path_buf(),
        }
    }

    pub(crate) fn worktree_path(&self) -> Result<PathBuf> {
        let output = self.output(&["rev-parse", "--show-toplevel"])?;

        if !output.status.success() {
            return Err(Error::NotGitWorktree);
        }

        Ok(PathBuf::from(trim_stdout(&output.stdout)))
    }

    pub(crate) fn main_worktree_path(&self) -> Result<Option<PathBuf>> {
        let output = self.output(&["worktree", "list", "--porcelain"])?;

        if !output.status.success() {
            return Err(Error::GitFailed {
                command: command_label(&["worktree", "list", "--porcelain"]),
                stderr: trim_stderr(&output.stderr),
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout
            .lines()
            .find_map(|line| line.strip_prefix("worktree "))
            .map(PathBuf::from))
    }

    pub(crate) fn default_branch(&self) -> Result<String> {
        let output = self.output(&[
            "symbolic-ref",
            "--quiet",
            "--short",
            "refs/remotes/origin/HEAD",
        ])?;

        if !output.status.success() {
            return Ok(String::new());
        }

        let branch = trim_stdout(&output.stdout);
        Ok(branch.strip_prefix("origin/").unwrap_or(&branch).to_owned())
    }

    fn output(&self, args: &[&str]) -> Result<Output> {
        Command::new("git")
            .args(args)
            .current_dir(&self.cwd)
            .output()
            .map_err(|source| Error::GitIo {
                command: command_label(args),
                source,
            })
    }
}

fn command_label(args: &[&str]) -> String {
    format!("git {}", args.join(" "))
}

fn trim_stdout(stdout: &[u8]) -> String {
    String::from_utf8_lossy(stdout).trim().to_owned()
}

fn trim_stderr(stderr: &[u8]) -> String {
    String::from_utf8_lossy(stderr).trim().to_owned()
}
