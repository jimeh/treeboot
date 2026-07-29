use predicates::prelude::*;

mod common;

#[cfg(unix)]
use common::symlink_dir;
use common::{
    assert_json_object_keys, canonical_path, git, git_repo, git_worktree, parse_json, treeboot,
    write_file,
};

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
    "TREEBOOT_WORKTREE_ID",
    "TREEBOOT_WORKTREE_PATH",
    "TREEBOOT_WORKTREE_SLUG",
];

#[test]
fn env_should_print_child_environment_as_text_json_and_yaml() {
    let repo = git_worktree();
    let expected_worktree = canonical_path(repo.worktree_path());

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
    let id = json["TREEBOOT_WORKTREE_ID"]
        .as_str()
        .expect("ID should be a string");
    let slug = json["TREEBOOT_WORKTREE_SLUG"]
        .as_str()
        .expect("slug should be a string");
    assert_eq!(id.len(), 6);
    assert!(slug.starts_with("linked-"));
    assert!(slug.ends_with(id));
    assert!(
        slug.bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    );
    assert!(slug.len() <= 48);
    assert!(json.get("TREEBOOT_STRICT").is_none());
    assert!(json.get("UNRELATED_TREEBOOT_TEST_VALUE").is_none());

    treeboot()
        .args(["env", "--format", "yaml"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("TREEBOOT_ROOT_PATH:"))
        .stdout(predicate::str::contains(format!(
            "TREEBOOT_WORKTREE_ID: {id}"
        )))
        .stdout(predicate::str::contains(format!(
            "TREEBOOT_WORKTREE_SLUG: {slug}"
        )));
}

#[test]
fn env_should_use_discovered_worktree_identifier_config() {
    let repo = git_worktree();
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        "worktree_id = { length = 8 }\n\
         worktree_slug = { max_length = 20, separator = \"_\" }\n",
    );

    let json = treeboot()
        .args(["env", "--json"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let json = parse_json(json, "configured env");
    let id = json["TREEBOOT_WORKTREE_ID"]
        .as_str()
        .expect("ID should be a string");
    let slug = json["TREEBOOT_WORKTREE_SLUG"]
        .as_str()
        .expect("slug should be a string");

    assert_eq!(id.len(), 8);
    assert!(slug.starts_with("linked_"));
    assert!(slug.ends_with(id));
    assert!(slug.len() <= 20);
}

#[test]
fn env_config_option_should_select_requested_identifier_config() {
    let repo = git_worktree();
    write_file(
        &repo.worktree_path().join("custom.toml"),
        "worktree_id = { length = 10 }\nworktree_slug = { separator = \"_\" }\n",
    );

    let json = treeboot()
        .args(["env", "--config", "custom.toml", "--json"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let json = parse_json(json, "explicit configured env");
    let id = json["TREEBOOT_WORKTREE_ID"]
        .as_str()
        .expect("ID should be a string");

    assert_eq!(id.len(), 10);
}

#[test]
fn env_should_fail_for_invalid_discovered_config() {
    let repo = git_worktree();
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        r#"worktree_id = { length = 0 }"#,
    );

    treeboot()
        .arg("env")
        .current_dir(repo.worktree_path())
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "worktree_id.length` must be between 1 and 52",
        ));
}

#[test]
fn env_should_fail_for_missing_explicit_config() {
    let repo = git_worktree();

    treeboot()
        .args(["env", "--config", "missing.toml"])
        .current_dir(repo.worktree_path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("config file not found"));
}

#[test]
fn env_should_resolve_configured_identifier_in_root_checkout() {
    let repo = git_repo();
    write_file(
        &repo.path().join(".treeboot.toml"),
        "worktree_id = { length = 9 }\nworktree_slug = { separator = \"_\" }\n",
    );

    let json = treeboot()
        .args(["env", "--json"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let json = parse_json(json, "root env");
    let id = json["TREEBOOT_WORKTREE_ID"]
        .as_str()
        .expect("ID should be a string");

    assert_eq!(id.len(), 9);
}

#[test]
fn env_paths_should_not_use_windows_verbatim_prefix() {
    let repo = git_worktree();
    let expected_root = canonical_path(repo.root_path());
    let expected_worktree = canonical_path(repo.worktree_path());

    let json = treeboot()
        .args(["env", "--json"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let json = parse_json(json, "env");

    for key in ["TREEBOOT_ROOT_PATH", "TREEBOOT_WORKTREE_PATH"] {
        let value = json[key].as_str().expect("path value should be a string");
        assert!(
            !value.starts_with(r"\\?\"),
            "{key} should not expose a Windows verbatim prefix: {value}"
        );
    }
    assert_eq!(
        json["TREEBOOT_ROOT_PATH"],
        expected_root.display().to_string()
    );
    assert_eq!(
        json["TREEBOOT_WORKTREE_PATH"],
        expected_worktree.display().to_string()
    );
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
    let expected_root = canonical_path(repo.root_path());

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
fn env_root_override_should_not_change_worktree_identifier() {
    let repo = git_worktree();
    let alternate_root = tempfile::TempDir::new().expect("alternate root should exist");

    let default = treeboot()
        .args(["env", "--json"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let overridden = treeboot()
        .arg("env")
        .arg("--root")
        .arg(alternate_root.path())
        .arg("--json")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    assert_eq!(
        parse_json(default, "default env")["TREEBOOT_WORKTREE_ID"],
        parse_json(overridden, "overridden env")["TREEBOOT_WORKTREE_ID"]
    );
}

#[test]
fn env_root_aliases_should_not_change_worktree_identifier() {
    let repo = git_worktree();
    let alternate_root = tempfile::TempDir::new().expect("alternate root should exist");
    let default = treeboot()
        .args(["env", "--json"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let expected = parse_json(default, "default env")["TREEBOOT_WORKTREE_ID"].clone();

    for variable in [
        "TREEBOOT_ROOT_PATH",
        "CODEX_SOURCE_TREE_PATH",
        "CONDUCTOR_ROOT_PATH",
        "SUPERSET_ROOT_PATH",
    ] {
        let output = treeboot()
            .args(["env", "--json"])
            .env(variable, alternate_root.path())
            .current_dir(repo.worktree_path())
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        assert_eq!(
            parse_json(output, variable)["TREEBOOT_WORKTREE_ID"],
            expected,
            "{variable} should not affect worktree identity"
        );
    }
}

#[test]
fn env_branch_creation_checkout_rename_and_detach_should_not_change_identifier() {
    let repo = git_worktree();
    let read_identifier = || {
        let output = treeboot()
            .args(["env", "--json"])
            .current_dir(repo.worktree_path())
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        parse_json(output, "branch env")["TREEBOOT_WORKTREE_ID"].clone()
    };
    let expected = read_identifier();

    git(&["branch", "temporary"], repo.worktree_path());
    assert_eq!(read_identifier(), expected);
    git(&["checkout", "temporary"], repo.worktree_path());
    assert_eq!(read_identifier(), expected);
    git(&["branch", "-m", "renamed"], repo.worktree_path());
    assert_eq!(read_identifier(), expected);
    git(&["checkout", "--detach", "HEAD"], repo.worktree_path());
    assert_eq!(read_identifier(), expected);
}

#[cfg(unix)]
#[test]
fn env_symlink_alias_should_not_change_worktree_identifier() {
    let repo = git_worktree();
    let alias_parent = tempfile::TempDir::new().expect("alias parent should exist");
    let alias = alias_parent.path().join("alias");
    symlink_dir(repo.worktree_path(), &alias);

    let actual = treeboot()
        .args(["env", "--json"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let aliased = treeboot()
        .args(["env", "--json"])
        .current_dir(&alias)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    assert_eq!(
        parse_json(actual, "actual env")["TREEBOOT_WORKTREE_ID"],
        parse_json(aliased, "aliased env")["TREEBOOT_WORKTREE_ID"]
    );
}

#[test]
fn env_root_environment_alias_should_override_source_checkout() {
    let repo = git_worktree();
    let expected_root = canonical_path(repo.root_path());

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
    let expected_root = canonical_path(repo.root_path());

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
