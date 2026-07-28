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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GitWorktree {
    pub(crate) path: PathBuf,
    pub(crate) bare: bool,
    pub(crate) main: bool,
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
        Ok(self
            .worktrees()?
            .into_iter()
            .find(|worktree| worktree.main)
            .map(|worktree| worktree.path))
    }

    pub(crate) fn worktrees(&self) -> Result<Vec<GitWorktree>> {
        let args = ["worktree", "list", "--porcelain", "-z"];
        let output = self.output(&args)?;

        if !output.status.success() {
            return Err(Error::GitFailed {
                command: command_label(&args),
                stderr: trim_stderr(&output.stderr),
            });
        }

        Ok(parse_worktrees(&output.stdout))
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

fn parse_worktrees(stdout: &[u8]) -> Vec<GitWorktree> {
    let mut worktrees = Vec::new();
    let mut current: Option<GitWorktree> = None;

    for field in stdout.split(|byte| *byte == b'\0') {
        if let Some(path) = field.strip_prefix(b"worktree ") {
            if let Some(worktree) = current.take() {
                worktrees.push(worktree);
            }
            current = Some(GitWorktree {
                path: path_from_git_bytes(path),
                bare: false,
                main: worktrees.is_empty(),
            });
        } else if field == b"bare"
            && let Some(worktree) = &mut current
        {
            worktree.bare = true;
        }
    }

    if let Some(worktree) = current {
        worktrees.push(worktree);
    }

    worktrees
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
    fn parse_worktrees_should_preserve_spaces_newlines_and_unknown_fields() {
        let output = b"worktree /repo/ main\ncheckout\0HEAD abc123\0branch refs/heads/main\0\0";

        assert_eq!(
            parse_worktrees(output),
            vec![GitWorktree {
                path: PathBuf::from("/repo/ main\ncheckout"),
                bare: false,
                main: true,
            }]
        );
    }

    #[test]
    fn parse_worktrees_should_parse_multiple_and_distinguish_bare_records() {
        let output = b"worktree /main\0HEAD abc123\0unknown value\0\0\
                       worktree /linked\0HEAD def456\0\0\
                       worktree /bare\0bare\0\0";

        assert_eq!(
            parse_worktrees(output),
            vec![
                GitWorktree {
                    path: PathBuf::from("/main"),
                    bare: false,
                    main: true,
                },
                GitWorktree {
                    path: PathBuf::from("/linked"),
                    bare: false,
                    main: false,
                },
                GitWorktree {
                    path: PathBuf::from("/bare"),
                    bare: true,
                    main: false,
                },
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn path_from_git_bytes_should_preserve_non_utf8_bytes() {
        use std::os::unix::ffi::OsStrExt;

        let worktrees = parse_worktrees(b"worktree /repo/\xff\0HEAD abc\0\0");

        assert_eq!(worktrees[0].path.as_os_str().as_bytes(), b"/repo/\xff");
    }
}
