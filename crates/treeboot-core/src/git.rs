use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[cfg(unix)]
use std::ffi::OsString;
#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;

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

        Ok(path_from_git_bytes(strip_one_trailing_lf(&output.stdout)))
    }

    pub(crate) fn main_worktree_path(&self) -> Result<Option<PathBuf>> {
        let args = ["worktree", "list", "--porcelain", "-z"];
        let output = self.output(&args)?;

        if !output.status.success() {
            return Err(Error::GitFailed {
                command: command_label(&args),
                stderr: trim_stderr(&output.stderr),
            });
        }

        Ok(parse_main_worktree_path(&output.stdout))
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

fn strip_one_trailing_lf(stdout: &[u8]) -> &[u8] {
    stdout.strip_suffix(b"\n").unwrap_or(stdout)
}

fn parse_main_worktree_path(stdout: &[u8]) -> Option<PathBuf> {
    stdout
        .split(|byte| *byte == b'\0')
        .find_map(|field| field.strip_prefix(b"worktree "))
        .map(path_from_git_bytes)
}

#[cfg(unix)]
fn path_from_git_bytes(bytes: &[u8]) -> PathBuf {
    PathBuf::from(OsString::from_vec(bytes.to_vec()))
}

#[cfg(not(unix))]
fn path_from_git_bytes(bytes: &[u8]) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_one_trailing_lf_should_preserve_boundary_whitespace() {
        assert_eq!(strip_one_trailing_lf(b" /repo \n"), b" /repo ");
    }

    #[test]
    fn strip_one_trailing_lf_should_remove_only_one_lf() {
        assert_eq!(strip_one_trailing_lf(b"/repo\n\n"), b"/repo\n");
    }

    #[test]
    fn parse_main_worktree_path_should_preserve_spaces_and_newlines() {
        let output = b"worktree /repo/ main\ncheckout\0HEAD abc123\0branch refs/heads/main\0\0";

        assert_eq!(
            parse_main_worktree_path(output),
            Some(PathBuf::from("/repo/ main\ncheckout"))
        );
    }

    #[test]
    fn parse_main_worktree_path_should_find_first_worktree_field() {
        let output = b"worktree /main\0HEAD abc123\0\0worktree /linked\0HEAD def456\0\0";

        assert_eq!(
            parse_main_worktree_path(output),
            Some(PathBuf::from("/main"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn path_from_git_bytes_should_preserve_non_utf8_bytes() {
        use std::os::unix::ffi::OsStrExt;

        let path = path_from_git_bytes(b"/repo/\xff");

        assert_eq!(path.as_os_str().as_bytes(), b"/repo/\xff");
    }
}
