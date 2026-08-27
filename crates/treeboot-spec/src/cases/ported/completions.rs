#![allow(dead_code)]

use predicates::prelude::*;
use tempfile::TempDir;

use crate::cases::support::{treeboot, write_file};

pub(crate) fn completions_supported_shells_should_emit_scripts() {
    for shell in ["bash", "zsh", "fish", "powershell", "elvish"] {
        treeboot()
            .args(["completions", shell])
            .assert()
            .success()
            .stderr(predicate::str::is_empty())
            .stdout(predicate::str::contains("treeboot"));
    }
}

pub(crate) fn completions_should_include_current_subcommands_and_flags() {
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
        .stdout(predicate::str::contains("worktree"))
        .stdout(predicate::str::contains("--root"))
        .stdout(predicate::str::contains("--config"))
        .stdout(predicate::str::contains("--no-init-script").not())
        .stdout(predicate::str::contains("--dry-run"))
        .stdout(predicate::str::contains("--verbose"));
}

pub(crate) fn dynamic_completions_should_include_nested_worktree_commands_and_formats() {
    treeboot()
        .env("COMPLETE", "fish")
        .args(["--", "treeboot", "worktree", ""])
        .assert()
        .success()
        .stdout(predicate::str::contains("id"))
        .stdout(predicate::str::contains("slug"))
        .stdout(predicate::str::contains("path"))
        .stdout(predicate::str::contains("list"));

    for command in ["id", "slug", "path", "list"] {
        treeboot()
            .env("COMPLETE", "fish")
            .args(["--", "treeboot", "worktree", command, "--"])
            .assert()
            .success()
            .stdout(predicate::str::contains("--format"))
            .stdout(predicate::str::contains("--json"))
            .stdout(predicate::str::contains("--yaml"));
    }
}

pub(crate) fn dynamic_identity_completions_should_suggest_directories() {
    let dir = TempDir::new().expect("tempdir should be created");
    std::fs::create_dir(dir.path().join("target-dir")).expect("target directory should be created");
    write_file(&dir.path().join("target-file"), "file\n");

    for command in ["id", "slug"] {
        treeboot()
            .env("COMPLETE", "fish")
            .args(["--", "treeboot", "worktree", command, "target"])
            .current_dir(dir.path())
            .assert()
            .success()
            .stdout(predicate::str::contains("target-dir"))
            .stdout(predicate::str::contains("target-file").not());
    }
}

pub(crate) fn dynamic_completions_should_include_teardown_flags() {
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

pub(crate) fn dynamic_completions_should_include_manual_command_flags() {
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

pub(crate) fn completions_should_omit_removed_init_script_flag() {
    treeboot()
        .env("COMPLETE", "fish")
        .args(["--", "treeboot", "init", "--"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--config"))
        .stdout(predicate::str::contains("--script").not());
}

pub(crate) fn completions_unsupported_shell_should_exit_with_usage_error() {
    treeboot()
        .args(["completions", "nu"])
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::is_empty().not())
        .stderr(predicate::str::is_empty().not());
}

pub(crate) fn completions_should_not_require_git_or_config_discovery() {
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
