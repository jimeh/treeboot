use predicates::prelude::*;

mod common;

use common::{
    assert_context_shape, assert_json_object_keys, git_worktree, parse_json, treeboot, write_file,
};

#[cfg(unix)]
use common::write_executable_script;

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
fn status_should_support_json_yaml_and_text_formats() {
    let repo = git_worktree();
    let expected_worktree =
        std::fs::canonicalize(repo.worktree_path()).expect("worktree should canonicalize");

    let json = treeboot()
        .args(["status", "--format", "json"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let json = parse_json(json, "status");
    assert_json_object_keys(&json, &["config", "context", "init_script"]);
    assert_context_shape(&json["context"]);
    assert_json_object_keys(&json["init_script"], &["ignored", "status"]);
    assert_eq!(
        json["context"]["worktree_path"],
        expected_worktree.display().to_string()
    );
    assert_eq!(json["init_script"]["status"], "not_found");
    assert_eq!(
        json["init_script"]["ignored"],
        serde_json::Value::Array(vec![])
    );

    treeboot()
        .args(["status", "--yaml"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("context:"))
        .stdout(predicate::str::contains("init_script:"));

    treeboot()
        .args(["status", "--format", "text"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("treeboot: status"));
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
fn status_no_init_script_json_should_report_skipped_init_script() {
    let repo = git_worktree();

    let json = treeboot()
        .args(["status", "--no-init-script", "--json"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let json = parse_json(json, "status");
    assert_json_object_keys(&json["init_script"], &["status"]);
    assert_eq!(json["init_script"]["status"], "skipped");
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

#[test]
fn status_output_shortcuts_should_conflict_with_format() {
    let repo = git_worktree();

    treeboot()
        .args(["status", "--json", "--format", "yaml"])
        .current_dir(repo.worktree_path())
        .assert()
        .code(2)
        .stderr(predicate::str::contains("cannot be used with"));

    treeboot()
        .args(["status", "--json", "--yaml"])
        .current_dir(repo.worktree_path())
        .assert()
        .code(2)
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn status_should_fail_outside_git_worktree() {
    let dir = tempfile::TempDir::new().expect("tempdir should be created");

    treeboot()
        .arg("status")
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("not inside a Git worktree"));
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

    let json = treeboot()
        .args(["status", "--json"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let json = parse_json(json, "status");
    assert_json_object_keys(&json["init_script"], &["ignored", "status"]);
    assert_eq!(json["init_script"]["status"], "not_found");
    assert_json_object_keys(&json["init_script"]["ignored"][0], &["path", "reason"]);
    assert_eq!(
        json["init_script"]["ignored"][0]["path"],
        expected_script.display().to_string()
    );
    assert_eq!(
        json["init_script"]["ignored"][0]["reason"],
        "not_executable"
    );
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

#[cfg(unix)]
#[test]
fn status_json_should_report_executable_init_script_without_running_it() {
    let repo = git_worktree();
    let script = repo.worktree_path().join(".treeboot.sh");
    let marker = repo.worktree_path().join("script.out");
    write_executable_script(
        &script,
        &format!("#!/bin/sh\nprintf 'ran\\n' > {}\n", marker.display()),
    );
    let expected_script = std::fs::canonicalize(&script).expect("script should canonicalize");

    let json = treeboot()
        .args(["status", "--json"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let json = parse_json(json, "status");
    assert_json_object_keys(&json["init_script"], &["path", "status"]);
    assert_eq!(json["init_script"]["status"], "found");
    assert_eq!(
        json["init_script"]["path"],
        expected_script.display().to_string()
    );

    assert!(!marker.exists());
}
