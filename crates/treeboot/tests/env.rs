use predicates::prelude::*;

mod common;

use common::{assert_json_object_keys, git_worktree, parse_json, treeboot};

const ENV_KEYS: &[&str] = &[
    "CODEX_SOURCE_TREE_PATH",
    "CODEX_WORKTREE_PATH",
    "CONDUCTOR_DEFAULT_BRANCH",
    "CONDUCTOR_ROOT_PATH",
    "CONDUCTOR_WORKSPACE_PATH",
    "GIT_SOURCE_TREE_PATH",
    "GIT_WORKTREE_PATH",
    "SUPERSET_ROOT_PATH",
    "TREEBOOT_DEFAULT_BRANCH",
    "TREEBOOT_ROOT_PATH",
    "TREEBOOT_WORKTREE_PATH",
];

#[test]
fn env_should_print_child_environment_as_text_json_and_yaml() {
    let repo = git_worktree();
    let expected_worktree =
        std::fs::canonicalize(repo.worktree_path()).expect("worktree should canonicalize");

    treeboot()
        .arg("env")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("TREEBOOT_ROOT_PATH="))
        .stdout(predicate::str::contains("TREEBOOT_WORKTREE_PATH="));

    let json = treeboot()
        .args(["env", "--json"])
        .env("TREEBOOT_STRICT", "true")
        .env("UNRELATED_TREEBOOT_TEST_VALUE", "hidden")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let json = parse_json(json, "env");
    assert_json_object_keys(&json, ENV_KEYS);
    for key in ENV_KEYS {
        assert!(json[key].is_string(), "{key} should be a string");
    }
    assert_eq!(
        json["TREEBOOT_WORKTREE_PATH"],
        expected_worktree.display().to_string()
    );
    assert!(json.get("TREEBOOT_STRICT").is_none());
    assert!(json.get("UNRELATED_TREEBOOT_TEST_VALUE").is_none());

    treeboot()
        .args(["env", "--format", "yaml"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("TREEBOOT_ROOT_PATH:"));
}

#[test]
fn env_should_support_text_format_and_yaml_shortcut() {
    let repo = git_worktree();

    treeboot()
        .args(["env", "--format", "text"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("TREEBOOT_DEFAULT_BRANCH="));

    treeboot()
        .args(["env", "--yaml"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("TREEBOOT_WORKTREE_PATH:"));
}

#[test]
fn env_root_option_should_override_source_checkout() {
    let repo = git_worktree();
    let expected_root = std::fs::canonicalize(repo.root_path()).expect("root should canonicalize");

    let json = treeboot()
        .args(["env", "--root"])
        .arg(repo.root_path())
        .args(["--json"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let json = parse_json(json, "env");
    assert_eq!(
        json["TREEBOOT_ROOT_PATH"],
        expected_root.display().to_string()
    );
}

#[test]
fn env_root_environment_alias_should_override_source_checkout() {
    let repo = git_worktree();
    let expected_root = std::fs::canonicalize(repo.root_path()).expect("root should canonicalize");

    let json = treeboot()
        .args(["env", "--json"])
        .env("CONDUCTOR_ROOT_PATH", repo.root_path())
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let json = parse_json(json, "env");
    assert_eq!(
        json["TREEBOOT_ROOT_PATH"],
        expected_root.display().to_string()
    );
    assert_eq!(
        json["CONDUCTOR_ROOT_PATH"],
        expected_root.display().to_string()
    );
}

#[test]
fn env_root_environment_alias_should_resolve_relative_to_cwd() {
    let repo = git_worktree();
    let expected_root = std::fs::canonicalize(repo.root_path()).expect("root should canonicalize");

    let json = treeboot()
        .args(["env", "--json"])
        .env("TREEBOOT_ROOT_PATH", ".")
        .current_dir(repo.root_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let json = parse_json(json, "env");
    assert_eq!(
        json["TREEBOOT_ROOT_PATH"],
        expected_root.display().to_string()
    );
}

#[test]
fn env_should_fail_when_root_override_does_not_exist() {
    let repo = git_worktree();

    treeboot()
        .args(["env", "--root", "missing-root"])
        .current_dir(repo.worktree_path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("failed to normalize path"))
        .stderr(predicate::str::contains("missing-root"));
}

#[test]
fn env_output_shortcuts_should_conflict_with_format() {
    let repo = git_worktree();

    treeboot()
        .args(["env", "--json", "--format", "yaml"])
        .current_dir(repo.worktree_path())
        .assert()
        .code(2)
        .stderr(predicate::str::contains("cannot be used with"));

    treeboot()
        .args(["env", "--json", "--yaml"])
        .current_dir(repo.worktree_path())
        .assert()
        .code(2)
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn env_should_fail_outside_git_worktree() {
    let dir = tempfile::TempDir::new().expect("tempdir should be created");

    treeboot()
        .arg("env")
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("not inside a Git worktree"));
}
