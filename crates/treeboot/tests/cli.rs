use predicates::prelude::*;
use tempfile::TempDir;

mod common;

use common::{git_worktree, treeboot, write_file};

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
        .stdout(predicate::str::contains("--root"))
        .stdout(predicate::str::contains("--config"))
        .stdout(predicate::str::contains("--no-init-script"))
        .stdout(predicate::str::contains("--dry-run"));
}

#[test]
fn status_should_report_worktree_root_and_config_without_parsing() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(&config, "invalid toml = [\n");
    let expected_worktree =
        std::fs::canonicalize(repo.worktree_path()).expect("worktree should canonicalize");
    let expected_root = std::fs::canonicalize(repo.root_path()).expect("root should canonicalize");
    let expected_config = std::fs::canonicalize(&config).expect("config should canonicalize");

    treeboot()
        .arg("status")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("treeboot: status"))
        .stdout(predicate::str::contains(format!(
            "worktree: {}",
            expected_worktree.display()
        )))
        .stdout(predicate::str::contains(format!(
            "root: {}",
            expected_root.display()
        )))
        .stdout(predicate::str::contains("init_script: (none)"))
        .stdout(predicate::str::contains(format!(
            "config: {}",
            expected_config.display()
        )));
}

#[test]
fn info_alias_should_report_status() {
    let repo = git_worktree();

    treeboot()
        .arg("info")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("treeboot: status"));
}

#[test]
fn status_no_init_script_should_report_skipped_init_script() {
    let repo = git_worktree();

    treeboot()
        .args(["status", "--no-init-script"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("init_script: (skipped)"));
}

#[test]
fn status_should_report_default_branch_from_environment() {
    let repo = git_worktree();

    treeboot()
        .arg("status")
        .env("CONDUCTOR_DEFAULT_BRANCH", "trunk")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("default_branch: trunk"));
}

#[cfg(unix)]
#[test]
fn status_should_report_ignored_non_executable_init_script() {
    let repo = git_worktree();
    let script = repo.worktree_path().join(".treeboot.sh");
    write_file(&script, "#!/bin/sh\n");
    let expected_script = std::fs::canonicalize(&script).expect("script should canonicalize");

    treeboot()
        .arg("status")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("init_script: (none)"))
        .stdout(predicate::str::contains(format!(
            "ignored_init_script: {}",
            expected_script.display()
        )));
}

#[cfg(unix)]
#[test]
fn status_config_option_should_skip_init_script_and_report_requested_config() {
    let repo = git_worktree();
    let script = repo.worktree_path().join(".treeboot.sh");
    let marker = repo.worktree_path().join("script.out");
    let config = repo.worktree_path().join("custom.treeboot.toml");
    write_executable_script(
        &script,
        &format!("#!/bin/sh\nprintf 'ran\\n' > {}\n", marker.display()),
    );
    write_file(&config, "invalid toml = [\n");
    let expected_config = std::fs::canonicalize(&config).expect("config should canonicalize");

    treeboot()
        .args(["status", "--config", "custom.treeboot.toml"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("init_script: (skipped)"))
        .stdout(predicate::str::contains(format!(
            "config: {}",
            expected_config.display()
        )));

    assert!(!marker.exists());
}

#[cfg(unix)]
#[test]
fn status_should_report_executable_init_script_without_running_it() {
    let repo = git_worktree();
    let script = repo.worktree_path().join(".treeboot.sh");
    let marker = repo.worktree_path().join("script.out");
    write_executable_script(
        &script,
        &format!("#!/bin/sh\nprintf 'ran\\n' > {}\n", marker.display()),
    );
    let expected_script = std::fs::canonicalize(&script).expect("script should canonicalize");

    treeboot()
        .arg("status")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains(format!(
            "init_script: {}",
            expected_script.display()
        )));

    assert!(!marker.exists());
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
        .stdout(predicate::str::contains("--symlinks"));
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
