use predicates::prelude::*;

mod common;

use common::{
    assert_context_shape, assert_json_object_keys, git_repo, git_worktree, parse_json,
    toml_string_path, treeboot, write_file,
};

#[cfg(unix)]
use common::write_executable_script;

#[test]
fn check_should_validate_config_without_side_effects() {
    let repo = git_worktree();
    write_file(&repo.root_path().join(".env"), "TOKEN=1\n");
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        r#"copy = [".env"]"#,
    );

    treeboot()
        .arg("check")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout("treeboot: check ok\n");

    assert!(!repo.worktree_path().join(".env").exists());

    treeboot()
        .args(["check", "--format", "yaml"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("action:"));
}

#[test]
fn check_should_support_json_yaml_and_text_formats() {
    let repo = git_worktree();
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        r#"commands = [{ run = "printf ok" }]"#,
    );

    let json = treeboot()
        .args(["check", "--json"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let json = parse_json(json, "check");
    assert_json_object_keys(&json, &["action", "context"]);
    assert_context_shape(&json["context"]);
    assert_json_object_keys(&json["action"], &["kind", "path"]);
    assert_eq!(json["action"]["kind"], "config");
    assert!(json["action"]["path"].is_string());

    treeboot()
        .args(["check", "--yaml"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("kind: config"));

    treeboot()
        .args(["check", "--format", "text"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout("treeboot: check ok\n");
}

#[test]
fn check_should_fail_when_run_validation_fails() {
    let repo = git_worktree();
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        r#"copy = [
  { source = ".env", target = ".env" },
  { source = ".env.local", target = ".env" },
]"#,
    );

    treeboot()
        .arg("check")
        .current_dir(repo.worktree_path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("duplicate configured target"));
}

#[test]
fn check_should_fail_for_invalid_ignore_patterns() {
    let repo = git_worktree();
    std::fs::create_dir_all(repo.root_path().join("shared"))
        .expect("source directory should be created");
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        r#"copy = [{ source = "shared", ignore = ["{a,b"] }]"#,
    );

    treeboot()
        .arg("check")
        .current_dir(repo.worktree_path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid ignore pattern"));
}

#[test]
fn check_should_fail_for_invalid_default_ignore_patterns() {
    let repo = git_worktree();
    std::fs::create_dir_all(repo.root_path().join("shared"))
        .expect("source directory should be created");
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        r#"
default_ignore = ["{a,b"]
copy = ["shared"]
"#,
    );

    treeboot()
        .arg("check")
        .current_dir(repo.worktree_path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid ignore pattern"));
}

#[test]
fn check_should_succeed_for_missing_config_unless_strict() {
    let repo = git_worktree();

    let json = treeboot()
        .args(["check", "--json"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let json = parse_json(json, "check");
    assert_json_object_keys(&json, &["action", "context"]);
    assert_context_shape(&json["context"]);
    assert_json_object_keys(&json["action"], &["kind"]);
    assert_eq!(json["action"]["kind"], "missing_config");

    treeboot()
        .args(["check", "--strict"])
        .current_dir(repo.worktree_path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("no config detected"));
}

#[test]
fn check_should_skip_root_checkout_unless_strict() {
    let repo = git_repo();

    let json = treeboot()
        .args(["check", "--json"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let json = parse_json(json, "check");
    assert_json_object_keys(&json, &["action", "context"]);
    assert_context_shape(&json["context"]);
    assert_json_object_keys(&json["action"], &["kind"]);
    assert_eq!(json["action"]["kind"], "root_worktree_skipped");

    treeboot()
        .args(["check", "--strict"])
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("This is not a work tree"));
}

#[test]
fn check_output_shortcuts_should_conflict_with_format() {
    let repo = git_worktree();

    treeboot()
        .args(["check", "--json", "--format", "yaml"])
        .current_dir(repo.worktree_path())
        .assert()
        .code(2)
        .stderr(predicate::str::contains("cannot be used with"));

    treeboot()
        .args(["check", "--json", "--yaml"])
        .current_dir(repo.worktree_path())
        .assert()
        .code(2)
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn check_should_fail_before_side_effects_for_invalid_env_override() {
    let repo = git_worktree();
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        r#"commands = [{ run = "touch marker" }]"#,
    );

    treeboot()
        .arg("check")
        .env("TREEBOOT_STRICT", "not-a-bool")
        .current_dir(repo.worktree_path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid boolean"));

    assert!(!repo.worktree_path().join("marker").exists());
}

#[test]
fn check_should_reject_invalid_source_boundary_env_override() {
    let repo = git_worktree();
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        r#"commands = [{ run = "touch marker" }]"#,
    );

    treeboot()
        .arg("check")
        .env(
            "TREEBOOT_DANGEROUSLY_ALLOW_SOURCES_OUTSIDE_ROOT",
            "not-a-bool",
        )
        .current_dir(repo.worktree_path())
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "TREEBOOT_DANGEROUSLY_ALLOW_SOURCES_OUTSIDE_ROOT",
        ))
        .stderr(predicate::str::contains("invalid boolean"));

    assert!(!repo.worktree_path().join("marker").exists());
}

#[test]
fn check_should_reject_invalid_target_boundary_env_override() {
    let repo = git_worktree();
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        r#"commands = [{ run = "touch marker" }]"#,
    );

    treeboot()
        .arg("check")
        .env(
            "TREEBOOT_DANGEROUSLY_ALLOW_TARGETS_OUTSIDE_WORKTREE",
            "not-a-bool",
        )
        .current_dir(repo.worktree_path())
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "TREEBOOT_DANGEROUSLY_ALLOW_TARGETS_OUTSIDE_WORKTREE",
        ))
        .stderr(predicate::str::contains("invalid boolean"));

    assert!(!repo.worktree_path().join("marker").exists());
}

#[test]
fn check_should_honor_source_boundary_environment_override() {
    let repo = git_worktree();
    let outside = tempfile::TempDir::new().expect("outside dir should be created");
    write_file(&outside.path().join("secret"), "TOKEN=1\n");
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        &format!(
            "copy = [{{ source = \"{}\", target = \"secret\" }}]\n",
            toml_string_path(&outside.path().join("secret"))
        ),
    );

    treeboot()
        .arg("check")
        .current_dir(repo.worktree_path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("outside root"));

    treeboot()
        .arg("check")
        .env("TREEBOOT_DANGEROUSLY_ALLOW_SOURCES_OUTSIDE_ROOT", "true")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout("treeboot: check ok\n");

    assert!(!repo.worktree_path().join("secret").exists());
}

#[test]
fn check_env_target_override_should_beat_config_target_override() {
    let repo = git_worktree();
    let outside = tempfile::TempDir::new().expect("outside target dir should be created");
    let outside_target = outside.path().join("target");
    write_file(&repo.root_path().join(".env"), "TOKEN=1\n");
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        &format!(
            "dangerously_allow_targets_outside_worktree = true\n\
             copy = [{{ source = \".env\", target = \"{}\" }}]\n",
            toml_string_path(&outside_target)
        ),
    );

    treeboot()
        .arg("check")
        .env(
            "TREEBOOT_DANGEROUSLY_ALLOW_TARGETS_OUTSIDE_WORKTREE",
            "false",
        )
        .current_dir(repo.worktree_path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("outside worktree"));
}

#[cfg(unix)]
#[test]
fn check_should_report_init_script_precedence_without_running_it() {
    let repo = git_worktree();
    let script = repo.worktree_path().join(".treeboot.sh");
    let marker = repo.worktree_path().join("script.out");
    write_executable_script(
        &script,
        &format!("#!/bin/sh\nprintf 'ran\\n' > {}\n", marker.display()),
    );
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        "invalid toml = [\n",
    );

    let json = treeboot()
        .args(["check", "--json"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let json = parse_json(json, "check");
    assert_json_object_keys(&json["action"], &["kind", "path"]);
    assert_eq!(json["action"]["kind"], "init_script");
    assert!(json["action"]["path"].is_string());

    assert!(!marker.exists());
}

#[cfg(unix)]
#[test]
fn check_config_option_should_skip_init_script_and_validate_requested_config() {
    let repo = git_worktree();
    let script = repo.worktree_path().join(".treeboot.sh");
    let marker = repo.worktree_path().join("script.out");
    let config = repo.worktree_path().join("custom.treeboot.toml");
    write_executable_script(
        &script,
        &format!("#!/bin/sh\nprintf 'ran\\n' > {}\n", marker.display()),
    );
    write_file(&config, r#"commands = [{ run = "printf ok" }]"#);

    let json = treeboot()
        .args(["check", "--config", "custom.treeboot.toml", "--json"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let json = parse_json(json, "check");
    assert_json_object_keys(&json["action"], &["kind", "path"]);
    assert_eq!(json["action"]["kind"], "config");
    assert!(json["action"]["path"].is_string());

    assert!(!marker.exists());
}
