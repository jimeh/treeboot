use predicates::prelude::*;

use crate::cases::support::{
    assert_context_shape, assert_json_object_keys, canonical_path, git_worktree, parse_json,
    treeboot, write_file,
};

pub(crate) fn status_should_report_worktree_root_and_config_without_parsing() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(&config, "invalid toml = [\n");
    let expected_worktree = canonical_path(repo.worktree_path());
    let expected_root = canonical_path(repo.root_path());
    let expected_config = canonical_path(&config);

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
        .stdout(predicate::str::contains(format!(
            "config: {}",
            expected_config.display()
        )));
}

pub(crate) fn status_should_support_json_yaml_and_text_formats() {
    let repo = git_worktree();
    let expected_worktree = canonical_path(repo.worktree_path());

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
    assert_json_object_keys(&json, &["config", "context"]);
    assert_context_shape(&json["context"]);
    assert_eq!(
        json["context"]["worktree_path"],
        expected_worktree.display().to_string()
    );

    treeboot()
        .args(["status", "--yaml"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("context:"))
        .stdout(predicate::str::contains("config:"));

    treeboot()
        .args(["status", "--format", "text"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("treeboot: status"));
}

pub(crate) fn info_alias_should_report_status() {
    let repo = git_worktree();

    treeboot()
        .arg("info")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("treeboot: status"));
}

pub(crate) fn status_no_init_script_should_be_usage_error() {
    let repo = git_worktree();

    treeboot()
        .args(["status", "--no-init-script"])
        .current_dir(repo.worktree_path())
        .assert()
        .code(2)
        .stderr(predicate::str::is_empty().not());
}

pub(crate) fn status_should_report_default_branch_from_environment() {
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

pub(crate) fn status_output_shortcuts_should_conflict_with_format() {
    let repo = git_worktree();

    treeboot()
        .args(["status", "--json", "--format", "yaml"])
        .current_dir(repo.worktree_path())
        .assert()
        .code(2)
        .stderr(predicate::str::is_empty().not());

    treeboot()
        .args(["status", "--json", "--yaml"])
        .current_dir(repo.worktree_path())
        .assert()
        .code(2)
        .stderr(predicate::str::is_empty().not());
}

pub(crate) fn status_should_fail_outside_git_worktree() {
    let dir = tempfile::TempDir::new().expect("tempdir should be created");

    treeboot()
        .arg("status")
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("not inside a Git worktree"));
}

pub(crate) fn status_config_option_should_report_requested_config() {
    let repo = git_worktree();
    let config = repo.worktree_path().join("custom.treeboot.toml");
    write_file(&config, "invalid toml = [\n");
    let expected_config = canonical_path(&config);

    treeboot()
        .args(["status", "--config", "custom.treeboot.toml"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains(format!(
            "config: {}",
            expected_config.display()
        )));
}

#[cfg(unix)]
pub(crate) fn status_json_should_ignore_legacy_script_file() {
    let repo = git_worktree();
    let script = repo.worktree_path().join(".treeboot.sh");
    write_file(&script, "#!/bin/sh\n");

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
    assert_json_object_keys(&json, &["config", "context"]);
}
