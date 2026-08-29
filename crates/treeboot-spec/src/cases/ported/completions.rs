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

pub(crate) fn completions_unsupported_shell_should_exit_with_usage_error() {
    treeboot()
        .args(["completions", "nu"])
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
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

pub(crate) fn completions_should_expose_plan_and_structured_output_flags() {
    treeboot()
        .env("COMPLETE", "fish")
        .args(["--", "treeboot", ""])
        .assert()
        .success()
        .stdout(predicate::str::contains("plan"));

    treeboot()
        .env("COMPLETE", "fish")
        .args(["--", "treeboot", "plan", "--"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--format"))
        .stdout(predicate::str::contains("--json"))
        .stdout(predicate::str::contains("--yaml"))
        .stdout(predicate::str::contains("--skip-commands"))
        .stdout(predicate::str::contains("--verbose"));
}
