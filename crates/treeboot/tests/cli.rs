use predicates::prelude::*;
use tempfile::TempDir;

mod common;

use common::{treeboot, write_file};

#[cfg(unix)]
use common::write_executable_script;

#[test]
fn help_should_print_usage() {
    treeboot()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage: treeboot"));
}

#[test]
fn version_should_print_package_version() {
    treeboot()
        .arg("--version")
        .assert()
        .success()
        .stdout(format!("treeboot {}\n", env!("CARGO_PKG_VERSION")));
}

#[test]
fn unknown_option_should_exit_with_usage_error() {
    treeboot()
        .arg("--unknown")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("unexpected argument"));
}

#[test]
fn legacy_no_commands_option_should_exit_with_usage_error() {
    treeboot()
        .arg("--no-commands")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("unexpected argument"));
}

#[test]
fn completions_supported_shells_should_emit_scripts() {
    for shell in ["bash", "zsh", "fish", "powershell", "elvish"] {
        treeboot()
            .args(["completions", shell])
            .assert()
            .success()
            .stderr(predicate::str::is_empty())
            .stdout(predicate::str::contains("treeboot").and(predicate::str::contains("config")));
    }
}

#[test]
fn completions_should_include_current_subcommands_and_flags() {
    treeboot()
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("run"))
        .stdout(predicate::str::contains("config"))
        .stdout(predicate::str::contains("init"))
        .stdout(predicate::str::contains("--root"))
        .stdout(predicate::str::contains("--config"))
        .stdout(predicate::str::contains("--dry-run"));
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

#[cfg(unix)]
#[test]
fn completions_should_not_run_init_scripts() {
    let dir = TempDir::new().expect("tempdir should be created");
    let script = dir.path().join(".treeboot.sh");
    let marker = dir.path().join("script.out");
    write_executable_script(
        &script,
        &format!("#!/bin/sh\nprintf 'ran\\n' > {}\n", marker.display()),
    );

    treeboot()
        .args(["completions", "zsh"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("treeboot"));

    assert!(!marker.exists());
}
