use std::ffi::{OsStr, OsString};
use std::path::Path;

use predicates::prelude::*;

mod common;

use common::{git_repo, git_worktree, treeboot, write_file};

#[cfg(unix)]
use common::write_executable_script;

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
fn manual_commands_should_require_sources() {
    for command in ["copy", "symlink", "sync"] {
        treeboot()
            .arg(command)
            .assert()
            .code(2)
            .stderr(predicate::str::contains("required"));
    }
}

#[test]
fn copy_should_create_files_and_directories_from_root() {
    let repo = git_worktree();
    write_file(&repo.root_path().join(".env"), "TOKEN=1\n");
    std::fs::create_dir_all(repo.root_path().join("shared/nested"))
        .expect("source directory should be created");
    write_file(&repo.root_path().join("shared/nested/config"), "value\n");

    treeboot()
        .args(["copy", ".env", "shared"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: copy .env -> .env"))
        .stdout(predicate::str::contains("treeboot: copy shared -> shared"));

    assert_eq!(
        std::fs::read_to_string(repo.worktree_path().join(".env"))
            .expect("copied file should be readable"),
        "TOKEN=1\n"
    );
    assert_eq!(
        std::fs::read_to_string(repo.worktree_path().join("shared/nested/config"))
            .expect("copied nested file should be readable"),
        "value\n"
    );
}

#[test]
fn copy_should_place_multiple_sources_under_target_prefix() {
    let repo = git_worktree();
    write_file(&repo.root_path().join("a"), "a\n");
    write_file(&repo.root_path().join("c"), "c\n");

    treeboot()
        .args(["copy", "a", "c", "--target", "local"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: copy a -> local/a"))
        .stdout(predicate::str::contains("treeboot: copy c -> local/c"));

    assert_eq!(
        std::fs::read_to_string(repo.worktree_path().join("local/a"))
            .expect("first target should be readable"),
        "a\n"
    );
    assert_eq!(
        std::fs::read_to_string(repo.worktree_path().join("local/c"))
            .expect("second target should be readable"),
        "c\n"
    );
}

#[cfg(unix)]
#[test]
fn symlink_should_create_relative_link() {
    let repo = git_worktree();
    write_file(&repo.root_path().join(".tool-versions"), "rust latest\n");

    treeboot()
        .args([
            "symlink",
            ".tool-versions",
            "--target",
            "config/tool-versions",
        ])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "treeboot: symlink .tool-versions -> config/tool-versions",
        ));

    let link = repo.worktree_path().join("config/tool-versions");
    let target = std::fs::read_link(&link).expect("target should be a symlink");
    assert!(target.is_relative());
    assert_eq!(
        std::fs::canonicalize(link).expect("link should resolve"),
        std::fs::canonicalize(repo.root_path().join(".tool-versions"))
            .expect("source should canonicalize")
    );
}

#[test]
fn sync_should_update_changed_files() {
    let repo = git_worktree();
    std::fs::create_dir_all(repo.root_path().join("shared"))
        .expect("source directory should be created");
    std::fs::create_dir_all(repo.worktree_path().join("shared"))
        .expect("target directory should be created");
    write_file(&repo.root_path().join("shared/config"), "new\n");
    write_file(&repo.worktree_path().join("shared/config"), "old\n");

    treeboot()
        .args(["sync", "shared", "--compare", "checksum"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "treeboot: sync shared/config -> shared/config",
        ));

    assert_eq!(
        std::fs::read_to_string(repo.worktree_path().join("shared/config"))
            .expect("synced file should be readable"),
        "new\n"
    );
}

#[test]
fn sync_delete_should_remove_target_only_files() {
    let repo = git_worktree();
    std::fs::create_dir_all(repo.root_path().join("shared"))
        .expect("source directory should be created");
    std::fs::create_dir_all(repo.worktree_path().join("shared"))
        .expect("target directory should be created");
    write_file(&repo.root_path().join("shared/config"), "value\n");
    write_file(&repo.worktree_path().join("shared/extra"), "remove\n");

    treeboot()
        .args(["sync", "shared", "--delete"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: delete shared/extra"));

    assert!(!repo.worktree_path().join("shared/extra").exists());
}

#[test]
fn sync_no_delete_should_preserve_target_only_files() {
    let repo = git_worktree();
    std::fs::create_dir_all(repo.root_path().join("shared"))
        .expect("source directory should be created");
    std::fs::create_dir_all(repo.worktree_path().join("shared"))
        .expect("target directory should be created");
    write_file(&repo.root_path().join("shared/config"), "value\n");
    write_file(&repo.worktree_path().join("shared/extra"), "keep\n");

    treeboot()
        .args(["sync", "shared", "--no-delete"])
        .current_dir(repo.worktree_path())
        .assert()
        .success();

    assert_eq!(
        std::fs::read_to_string(repo.worktree_path().join("shared/extra"))
            .expect("extra file should remain"),
        "keep\n"
    );
}

#[test]
fn dry_run_should_report_without_mutation() {
    let repo = git_worktree();
    write_file(&repo.root_path().join(".env"), "TOKEN=1\n");

    treeboot()
        .args(["copy", ".env", "--dry-run"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "treeboot: would copy .env -> .env",
        ));

    assert!(!repo.worktree_path().join(".env").exists());
}

#[test]
fn force_should_replace_supported_targets() {
    let repo = git_worktree();
    write_file(&repo.root_path().join(".env"), "new\n");
    write_file(&repo.worktree_path().join(".env"), "old\n");

    treeboot()
        .args(["copy", ".env", "--force"])
        .current_dir(repo.worktree_path())
        .assert()
        .success();

    assert_eq!(
        std::fs::read_to_string(repo.worktree_path().join(".env"))
            .expect("target should be readable"),
        "new\n"
    );
}

#[test]
fn root_checkout_should_skip_manual_file_operations() {
    let repo = git_repo();
    write_file(&repo.path().join(".env"), "TOKEN=1\n");

    treeboot()
        .args(["copy", ".env"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "treeboot: This is not a work tree",
        ))
        .stdout(predicate::str::contains("treeboot: copy").not());
}

#[test]
fn strict_root_checkout_should_fail_manual_file_operations() {
    let repo = git_repo();

    treeboot()
        .args(["copy", ".env", "--strict"])
        .current_dir(repo.path())
        .assert()
        .code(1)
        .stdout(predicate::str::contains(
            "treeboot: This is not a work tree",
        ))
        .stderr(predicate::str::contains(
            "treeboot: This is not a work tree",
        ));
}

#[test]
fn invalid_operation_specific_flags_should_be_usage_errors() {
    treeboot()
        .args(["copy", ".env", "--compare", "checksum"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("unexpected argument"));

    treeboot()
        .args(["symlink", ".env", "--symlinks", "preserve"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("unexpected argument"));
}

#[test]
fn required_missing_source_should_fail_without_config_locations() {
    let repo = git_worktree();

    treeboot()
        .args(["copy", "missing", "--required"])
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("invalid copy file operation"))
        .stderr(predicate::str::contains("required source does not exist"))
        .stderr(predicate::str::contains("invalid config").not())
        .stderr(predicate::str::contains("line").not());
}

#[test]
fn duplicate_manual_targets_should_fail_before_side_effects() {
    let repo = git_worktree();
    write_file(&repo.root_path().join("a"), "value\n");

    treeboot()
        .args(["copy", "a", "./a"])
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("invalid copy file operation"))
        .stderr(predicate::str::contains("duplicate target"));

    assert!(!repo.worktree_path().join("a").exists());
}

#[cfg(unix)]
#[test]
fn unsafe_source_symlink_should_fail_before_side_effects() {
    let repo = git_worktree();
    let outside = repo
        .root_path()
        .parent()
        .expect("root should have parent")
        .join("outside-secret");
    write_file(&outside, "secret\n");
    std::fs::create_dir_all(repo.root_path().join("unsafe"))
        .expect("source directory should be created");
    std::os::unix::fs::symlink(&outside, repo.root_path().join("unsafe/link"))
        .expect("unsafe symlink should be created");

    treeboot()
        .args(["copy", "unsafe"])
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("invalid copy file operation"))
        .stderr(predicate::str::contains("unsafe symlink"));

    assert!(!repo.worktree_path().join("unsafe").exists());
}

#[test]
fn manual_commands_should_ignore_invalid_config() {
    let repo = git_worktree();
    write_file(&repo.root_path().join(".env"), "TOKEN=1\n");
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        "invalid = [\n",
    );

    treeboot()
        .args(["copy", ".env"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::contains("invalid config").not());

    assert!(repo.worktree_path().join(".env").is_file());
}

#[cfg(unix)]
#[test]
fn manual_commands_should_ignore_executable_init_script() {
    let repo = git_worktree();
    let marker = repo.worktree_path().join("script.out");
    write_file(&repo.root_path().join(".env"), "TOKEN=1\n");
    write_executable_script(
        &repo.worktree_path().join(".treeboot.sh"),
        &format!("#!/bin/sh\nprintf 'ran\\n' > {}\n", marker.display()),
    );

    treeboot()
        .args(["copy", ".env"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: run").not());

    assert!(!marker.exists());
    assert!(repo.worktree_path().join(".env").is_file());
}

#[test]
fn dynamic_completion_should_list_root_relative_sources() {
    let repo = git_worktree();
    write_file(&repo.root_path().join(".env"), "TOKEN=1\n");
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
