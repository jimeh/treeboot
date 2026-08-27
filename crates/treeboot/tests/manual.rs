use std::ffi::{OsStr, OsString};
use std::path::Path;

use predicates::prelude::*;

mod common;

use common::{git_worktree, treeboot, write_file};

const COMPLETION_SHELLS: [&str; 5] = ["bash", "zsh", "fish", "powershell", "elvish"];

fn complete_source_candidates<I, S>(
    shell: &str,
    args: I,
    current_dir: &Path,
) -> assert_cmd::assert::Assert
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let args = args
        .into_iter()
        .map(|arg| arg.as_ref().to_os_string())
        .collect::<Vec<OsString>>();
    let index = args.len().saturating_sub(1).to_string();
    let mut command = treeboot();

    command
        .env("COMPLETE", shell)
        .env("_CLAP_COMPLETE_INDEX", index)
        .env("_CLAP_IFS", "\n")
        .arg("--")
        .args(args)
        .current_dir(current_dir);

    command.assert()
}

#[test]
fn dynamic_completion_should_list_root_relative_sources() {
    let repo = git_worktree();
    write_file(&repo.root_path().join(".env"), "TOKEN=1\n");
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        "invalid = [\n",
    );
    std::fs::create_dir_all(repo.root_path().join("shared"))
        .expect("source directory should be created");

    for shell in COMPLETION_SHELLS {
        complete_source_candidates(shell, ["treeboot", "copy", ""], repo.worktree_path())
            .success()
            .stdout(predicate::str::contains(".env"))
            .stdout(predicate::str::contains("shared"));
    }
}

#[test]
fn dynamic_completion_should_use_root_option_for_sources() {
    let repo = git_worktree();
    let root = tempfile::TempDir::new().expect("override root should be created");
    write_file(&root.path().join("override.env"), "TOKEN=1\n");
    write_file(&repo.root_path().join("default.env"), "TOKEN=1\n");

    for shell in COMPLETION_SHELLS {
        complete_source_candidates(
            shell,
            [
                OsStr::new("treeboot"),
                OsStr::new("copy"),
                OsStr::new("--root"),
                root.path().as_os_str(),
                OsStr::new(""),
            ],
            repo.worktree_path(),
        )
        .success()
        .stdout(predicate::str::contains("override.env"))
        .stdout(predicate::str::contains("default.env").not());
    }
}

#[test]
fn dynamic_completion_should_use_root_equals_option_for_sources() {
    let repo = git_worktree();
    let root = tempfile::TempDir::new().expect("override root should be created");
    write_file(&root.path().join("override.env"), "TOKEN=1\n");
    write_file(&repo.root_path().join("default.env"), "TOKEN=1\n");

    for shell in COMPLETION_SHELLS {
        complete_source_candidates(
            shell,
            [
                OsString::from("treeboot"),
                OsString::from("copy"),
                OsString::from(format!("--root={}", root.path().display())),
                OsString::new(),
            ],
            repo.worktree_path(),
        )
        .success()
        .stdout(predicate::str::contains("override.env"))
        .stdout(predicate::str::contains("default.env").not());
    }
}
