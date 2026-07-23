use predicates::prelude::*;

mod common;

use common::{
    assert_context_shape, assert_json_object_keys, git_repo, git_worktree, parse_json, treeboot,
    write_file,
};

#[cfg(unix)]
use common::write_executable_script;

fn assert_doctor_report_shape(json: &serde_json::Value) {
    assert_json_object_keys(json, &["context", "diagnostics", "fatal"]);
    assert!(json["fatal"].is_boolean());

    if !json["context"].is_null() {
        assert_context_shape(&json["context"]);
    }

    let diagnostics = json["diagnostics"]
        .as_array()
        .expect("diagnostics should be an array");
    assert!(!diagnostics.is_empty());

    for diagnostic in diagnostics {
        assert_json_object_keys(diagnostic, &["message", "name", "status"]);
        assert!(diagnostic["name"].is_string());
        assert!(diagnostic["message"].is_string());
        assert!(
            matches!(
                diagnostic["status"].as_str(),
                Some("ok" | "warning" | "error")
            ),
            "diagnostic status should be ok, warning, or error"
        );
    }
}

fn has_diagnostic(json: &serde_json::Value, name: &str, status: &str, message: &str) -> bool {
    json["diagnostics"]
        .as_array()
        .expect("diagnostics should be an array")
        .iter()
        .any(|diagnostic| {
            diagnostic["name"] == name
                && diagnostic["status"] == status
                && diagnostic["message"]
                    .as_str()
                    .is_some_and(|actual| actual.contains(message))
        })
}

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
    let json = parse_json(json, "doctor");
    assert_doctor_report_shape(&json);
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
fn doctor_strict_should_treat_missing_config_as_fatal() {
    let repo = git_worktree();

    let json = treeboot()
        .args(["doctor", "--strict", "--json"])
        .current_dir(repo.worktree_path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("doctor found fatal issues"))
        .get_output()
        .stdout
        .clone();

    let json = parse_json(json, "doctor");
    assert_doctor_report_shape(&json);
    assert_eq!(json["fatal"], true);
    assert!(has_diagnostic(
        &json,
        "config",
        "error",
        "no config detected under strict mode"
    ));
}

#[test]
fn doctor_should_apply_environment_strict_to_missing_config() {
    let repo = git_worktree();

    let json = treeboot()
        .args(["doctor", "--json"])
        .env("TREEBOOT_STRICT", "true")
        .current_dir(repo.worktree_path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("doctor found fatal issues"))
        .get_output()
        .stdout
        .clone();

    let json = parse_json(json, "doctor");
    assert_doctor_report_shape(&json);
    assert_eq!(json["fatal"], true);
    assert!(has_diagnostic(
        &json,
        "config",
        "error",
        "no config detected under strict mode"
    ));
}

#[test]
fn doctor_strict_should_report_root_checkout_as_fatal_diagnostic() {
    let repo = git_repo();

    let json = treeboot()
        .args(["doctor", "--strict", "--json"])
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("doctor found fatal issues"))
        .get_output()
        .stdout
        .clone();

    let json = parse_json(json, "doctor");
    assert_doctor_report_shape(&json);
    assert_eq!(json["fatal"], true);
    assert!(has_diagnostic(
        &json,
        "root_worktree",
        "error",
        "root checkout is not a worktree under strict mode"
    ));
}

#[test]
fn doctor_should_apply_environment_strict_to_root_checkout() {
    let repo = git_repo();

    let json = treeboot()
        .args(["doctor", "--json"])
        .env("TREEBOOT_STRICT", "true")
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("doctor found fatal issues"))
        .get_output()
        .stdout
        .clone();

    let json = parse_json(json, "doctor");
    assert_doctor_report_shape(&json);
    assert_eq!(json["fatal"], true);
    assert!(has_diagnostic(
        &json,
        "root_worktree",
        "error",
        "root checkout is not a worktree under strict mode"
    ));
}

#[test]
fn doctor_strict_should_validate_config_with_strict_policy() {
    let repo = git_worktree();
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        r#"sync = ["shared"]"#,
    );

    let json = treeboot()
        .args(["doctor", "--strict", "--json"])
        .current_dir(repo.worktree_path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("doctor found fatal issues"))
        .get_output()
        .stdout
        .clone();

    let json = parse_json(json, "doctor");
    assert_doctor_report_shape(&json);
    assert_eq!(json["fatal"], true);
    assert!(has_diagnostic(
        &json,
        "config_validation",
        "error",
        "strict mode cannot be used with sync"
    ));
}

#[test]
fn doctor_strict_should_report_missing_requested_config() {
    let repo = git_worktree();

    let json = treeboot()
        .args([
            "doctor",
            "--strict",
            "--config",
            "missing.treeboot.toml",
            "--json",
        ])
        .current_dir(repo.worktree_path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("doctor found fatal issues"))
        .get_output()
        .stdout
        .clone();

    let json = parse_json(json, "doctor");
    assert_doctor_report_shape(&json);
    assert_eq!(json["fatal"], true);
    assert!(has_diagnostic(
        &json,
        "config",
        "error",
        "config file not found"
    ));
}

#[test]
fn doctor_should_apply_environment_strict_to_config_validation() {
    let repo = git_worktree();
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        r#"sync = ["shared"]"#,
    );

    let json = treeboot()
        .args(["doctor", "--json"])
        .env("TREEBOOT_STRICT", "true")
        .current_dir(repo.worktree_path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("doctor found fatal issues"))
        .get_output()
        .stdout
        .clone();

    let json = parse_json(json, "doctor");
    assert_doctor_report_shape(&json);
    assert_eq!(json["fatal"], true);
    assert!(has_diagnostic(
        &json,
        "config_validation",
        "error",
        "strict mode cannot be used with sync"
    ));
}

#[test]
fn doctor_should_report_teardown_validation_errors_separately() {
    let repo = git_worktree();
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        r#"teardown_commands = [{ run = "echo teardown", cwd = ".." }]"#,
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

    let json = parse_json(json, "doctor");
    assert!(has_diagnostic(&json, "config", "ok", "config is valid:"));
    assert!(has_diagnostic(
        &json,
        "teardown_validation",
        "error",
        "command cwd resolves outside worktree"
    ));
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
fn doctor_should_warn_when_default_branch_is_unknown() {
    let repo = git_repo();

    let json = treeboot()
        .args(["doctor", "--json"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();

    let json = parse_json(json, "doctor");
    assert_doctor_report_shape(&json);
    assert_eq!(json["fatal"], false);
    assert_eq!(json["context"]["default_branch"], "");
    assert!(
        json["diagnostics"]
            .as_array()
            .expect("diagnostics should be an array")
            .iter()
            .any(|diagnostic| {
                diagnostic["name"] == "default_branch"
                    && diagnostic["status"] == "warning"
                    && diagnostic["message"] == "default branch unknown"
            })
    );

    treeboot()
        .arg("doctor")
        .current_dir(repo.path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains(
            "warning: default_branch: default branch unknown",
        ));
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
    let json = parse_json(json, "doctor");
    assert_doctor_report_shape(&json);
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
    let json = parse_json(json, "doctor");
    assert_doctor_report_shape(&json);
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
    let json = parse_json(json, "doctor");
    assert_doctor_report_shape(&json);
    assert_eq!(json["fatal"], true);
    assert_eq!(json["context"], serde_json::Value::Null);
}

#[cfg(unix)]
#[test]
fn doctor_should_ignore_legacy_script_and_omit_script_diagnostic() {
    let repo = git_worktree();
    let script = repo.worktree_path().join(".treeboot.sh");
    let marker = repo.worktree_path().join("script.out");
    write_executable_script(
        &script,
        &format!("#!/bin/sh\nprintf 'ran\\n' > {}\n", marker.display()),
    );
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        "commands = []\n",
    );

    let json = treeboot()
        .args(["doctor", "--json"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let json = parse_json(json, "doctor");
    let has_init_script_diagnostic = json["diagnostics"]
        .as_array()
        .expect("diagnostics should be an array")
        .iter()
        .any(|diagnostic| diagnostic["name"] == "init_script");

    assert!(!marker.exists());
    assert!(!has_init_script_diagnostic);
}

#[cfg(unix)]
#[test]
fn doctor_strict_should_fail_for_legacy_script_only_repo() {
    let repo = git_worktree();
    let script = repo.worktree_path().join(".treeboot.sh");
    write_executable_script(&script, "#!/bin/sh\n");

    let json = treeboot()
        .args(["doctor", "--strict", "--json"])
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("doctor found fatal issues"))
        .get_output()
        .stdout
        .clone();

    let json = parse_json(json, "doctor");
    assert_doctor_report_shape(&json);
    assert_eq!(json["fatal"], true);
    assert!(has_diagnostic(
        &json,
        "config",
        "error",
        "no config detected under strict mode"
    ));
}

#[test]
fn doctor_no_init_script_flag_should_be_usage_error() {
    let repo = git_worktree();

    treeboot()
        .args(["doctor", "--no-init-script"])
        .current_dir(repo.worktree_path())
        .assert()
        .code(2)
        .stderr(predicate::str::contains("unexpected argument"));
}

#[cfg(unix)]
#[test]
fn doctor_config_option_should_ignore_legacy_script_and_validate_requested_config() {
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
        .stdout(predicate::str::contains("ok: config: config is valid:"))
        .stdout(predicate::str::contains(
            "ok: teardown_validation: teardown config is valid",
        ));

    assert!(!marker.exists());
}
