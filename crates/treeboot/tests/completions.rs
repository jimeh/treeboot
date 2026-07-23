use predicates::prelude::*;
use tempfile::TempDir;

mod common;

use common::{treeboot, write_file};

#[test]
fn completions_supported_shells_should_emit_scripts() {
    for shell in ["bash", "zsh", "fish", "powershell", "elvish"] {
        treeboot()
            .args(["completions", shell])
            .assert()
            .success()
            .stderr(predicate::str::is_empty())
            .stdout(predicate::str::contains("treeboot"))
            .stdout(predicate::str::contains("COMPLETE"));
    }
}

#[test]
fn completions_should_include_current_subcommands_and_flags() {
    treeboot()
        .env("COMPLETE", "fish")
        .args(["--", "treeboot", ""])
        .assert()
        .success()
        .stdout(predicate::str::contains("copy"))
        .stdout(predicate::str::contains("symlink"))
        .stdout(predicate::str::contains("sync"))
        .stdout(predicate::str::contains("run"))
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("config"))
        .stdout(predicate::str::contains("init"))
        .stdout(predicate::str::contains("version"))
        .stdout(predicate::str::contains("check"))
        .stdout(predicate::str::contains("schema"))
        .stdout(predicate::str::contains("doctor"))
        .stdout(predicate::str::contains("env"))
        .stdout(predicate::str::contains("teardown"))
        .stdout(predicate::str::contains("--root"))
        .stdout(predicate::str::contains("--config"))
        .stdout(predicate::str::contains("--no-init-script").not())
        .stdout(predicate::str::contains("--dry-run"))
        .stdout(predicate::str::contains("--verbose"));
}

#[test]
fn dynamic_completions_should_include_teardown_flags() {
    treeboot()
        .env("COMPLETE", "fish")
        .args(["--", "treeboot", "teardown", "--"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--worktree"))
        .stdout(predicate::str::contains("--root"))
        .stdout(predicate::str::contains("--config"))
        .stdout(predicate::str::contains("--dry-run"))
        .stdout(predicate::str::contains("--yes"));
}

#[test]
fn dynamic_completions_should_include_manual_command_flags() {
    treeboot()
        .env("COMPLETE", "fish")
        .args(["--", "treeboot", "sync", "--"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--target"))
        .stdout(predicate::str::contains("--required"))
        .stdout(predicate::str::contains("--compare"))
        .stdout(predicate::str::contains("--delete"))
        .stdout(predicate::str::contains("--no-delete"))
        .stdout(predicate::str::contains("--symlinks"))
        .stdout(predicate::str::contains("--verbose"));
}

#[test]
fn completions_should_omit_removed_init_script_flag() {
    treeboot()
        .env("COMPLETE", "fish")
        .args(["--", "treeboot", "init", "--"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--config"))
        .stdout(predicate::str::contains("--script").not());
}

#[test]
fn completions_unsupported_shell_should_exit_with_usage_error() {
    treeboot()
        .args(["completions", "nu"])
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("invalid value"))
        .stderr(predicate::str::contains("possible values"));
}

#[test]
fn completions_should_not_require_git_or_config_discovery() {
    let dir = TempDir::new().expect("tempdir should be created");
    write_file(&dir.path().join(".treeboot.toml"), "invalid toml = [\n");

    treeboot()
        .args(["completions", "fish"])
        .env("TREEBOOT_STRICT", "not-a-bool")
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("treeboot"));
}
