use predicates::prelude::*;

mod common;

use common::{git_worktree, treeboot, write_file};

#[cfg(unix)]
use common::write_executable_script;

#[test]
fn doctor_should_report_diagnostics_as_text_json_and_yaml() {
    let repo = git_worktree();

    treeboot()
        .arg("doctor")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("treeboot: doctor"))
        .stdout(predicate::str::contains("ok: worktree"))
        .stdout(predicate::str::contains(
            "warning: config: no config detected",
        ));

    let json = treeboot()
        .args(["doctor", "--json"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&json).expect("doctor JSON should parse");
    assert_eq!(json["fatal"], false);

    treeboot()
        .args(["doctor", "--format", "yaml"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("fatal: false"))
        .stdout(predicate::str::contains("diagnostics:"));
}

#[test]
fn doctor_should_support_text_format_and_yaml_shortcut() {
    let repo = git_worktree();

    treeboot()
        .args(["doctor", "--format", "text"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("treeboot: doctor"));

    treeboot()
        .args(["doctor", "--yaml"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("diagnostics:"));
}

#[test]
fn doctor_should_exit_nonzero_for_invalid_config_after_printing_report() {
    let repo = git_worktree();
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        "invalid toml = [\n",
    );

    let json = treeboot()
        .args(["doctor", "--json"])
        .current_dir(repo.worktree_path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("doctor found fatal issues"))
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&json).expect("doctor JSON should parse");
    assert_eq!(json["fatal"], true);
    assert!(
        json["diagnostics"]
            .as_array()
            .expect("diagnostics should be an array")
            .iter()
            .any(|diagnostic| diagnostic["name"] == "config")
    );
}

#[test]
fn doctor_text_should_report_fatal_config_diagnostics() {
    let repo = git_worktree();
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        "invalid toml = [\n",
    );

    treeboot()
        .arg("doctor")
        .current_dir(repo.worktree_path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("doctor found fatal issues"))
        .stdout(predicate::str::contains("treeboot: doctor"))
        .stdout(predicate::str::contains("error: config:"));
}

#[test]
fn doctor_should_report_invalid_env_override_as_fatal() {
    let repo = git_worktree();

    let json = treeboot()
        .args(["doctor", "--json"])
        .env("TREEBOOT_STRICT", "not-a-bool")
        .current_dir(repo.worktree_path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("doctor found fatal issues"))
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&json).expect("doctor JSON should parse");
    assert_eq!(json["fatal"], true);
    assert_eq!(json["context"], serde_json::Value::Null);
    assert_eq!(json["diagnostics"][0]["name"], "environment_options");
}

#[test]
fn doctor_output_shortcuts_should_conflict_with_format() {
    let repo = git_worktree();

    treeboot()
        .args(["doctor", "--json", "--format", "yaml"])
        .current_dir(repo.worktree_path())
        .assert()
        .code(2)
        .stderr(predicate::str::contains("cannot be used with"));

    treeboot()
        .args(["doctor", "--json", "--yaml"])
        .current_dir(repo.worktree_path())
        .assert()
        .code(2)
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn doctor_should_fail_outside_git_worktree() {
    let dir = tempfile::TempDir::new().expect("tempdir should be created");

    let json = treeboot()
        .args(["doctor", "--json"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("doctor found fatal issues"))
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&json).expect("doctor JSON should parse");
    assert_eq!(json["fatal"], true);
    assert_eq!(json["context"], serde_json::Value::Null);
}

#[cfg(unix)]
#[test]
fn doctor_should_report_init_script_without_running_it() {
    let repo = git_worktree();
    let script = repo.worktree_path().join(".treeboot.sh");
    let marker = repo.worktree_path().join("script.out");
    write_executable_script(
        &script,
        &format!("#!/bin/sh\nprintf 'ran\\n' > {}\n", marker.display()),
    );

    treeboot()
        .arg("doctor")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("ok: init_script: executable"));

    assert!(!marker.exists());
}

#[cfg(unix)]
#[test]
fn doctor_should_report_ignored_non_executable_init_script() {
    let repo = git_worktree();
    let script = repo.worktree_path().join(".treeboot.sh");
    write_file(&script, "#!/bin/sh\n");

    treeboot()
        .arg("doctor")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains(
            "warning: init_script: no executable init script found; ignored 1",
        ));
}

#[cfg(unix)]
#[test]
fn doctor_config_option_should_skip_init_script_and_validate_requested_config() {
    let repo = git_worktree();
    let script = repo.worktree_path().join(".treeboot.sh");
    let marker = repo.worktree_path().join("script.out");
    let config = repo.worktree_path().join("custom.treeboot.toml");
    write_executable_script(
        &script,
        &format!("#!/bin/sh\nprintf 'ran\\n' > {}\n", marker.display()),
    );
    write_file(&config, r#"commands = [{ run = "printf ok" }]"#);

    treeboot()
        .args(["doctor", "--config", "custom.treeboot.toml"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains(
            "ok: init_script: init script discovery skipped",
        ))
        .stdout(predicate::str::contains("ok: config: config is valid"));

    assert!(!marker.exists());
}
