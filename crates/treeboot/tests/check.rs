use predicates::prelude::*;

mod common;

use common::{
    assert_context_shape, assert_json_object_keys, canonical_path, git_repo, git_worktree,
    parse_json, symlink_file, toml_string_path, treeboot, write_file,
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
    assert_json_object_keys(&json, &["action", "context", "warnings"]);
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
fn check_should_fail_when_teardown_validation_fails() {
    let repo = git_worktree();
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        r#"teardown_commands = [{ run = "echo teardown", cwd = ".." }]"#,
    );

    treeboot()
        .arg("check")
        .current_dir(repo.worktree_path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("teardown:"))
        .stderr(predicate::str::contains(
            "command cwd resolves outside worktree",
        ));
}

#[test]
fn check_should_report_all_phase_validation_failures_in_order() {
    let repo = git_worktree();
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        "commands = [{ run = \"echo bootstrap\", cwd = \"..\" }]\n\
         teardown_commands = [{ run = \"echo teardown\", cwd = \"..\" }]\n",
    );

    let output = treeboot()
        .arg("check")
        .current_dir(repo.worktree_path())
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let stderr = String::from_utf8(output).expect("stderr should be UTF-8");
    let bootstrap = stderr
        .find("bootstrap:")
        .expect("bootstrap failure should be reported");
    let teardown = stderr
        .find("teardown:")
        .expect("teardown failure should be reported");
    assert!(bootstrap < teardown);
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
    assert_json_object_keys(&json, &["action", "context", "warnings"]);
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
    assert_json_object_keys(&json, &["action", "context", "warnings"]);
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
fn check_should_validate_existing_symlink_to_root_source_in_subdirectory() {
    let repo = git_worktree();
    let source = repo.root_path().join("config/master.key");
    let target = repo.worktree_path().join("config/master.key");
    std::fs::create_dir_all(source.parent().unwrap()).expect("source dir should be created");
    std::fs::create_dir_all(target.parent().unwrap()).expect("target dir should be created");
    write_file(&source, "secret\n");
    symlink_file(&source, &target);
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        r#"symlink = ["config/master.key"]"#,
    );

    treeboot()
        .arg("check")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout("treeboot: check ok\n");

    assert_eq!(canonical_path(&target), canonical_path(&source));
}

#[test]
fn check_should_honor_source_boundary_environment_override_for_symlink() {
    let repo = git_worktree();
    let outside = tempfile::NamedTempFile::new().expect("outside source should be created");
    write_file(outside.path(), "TOKEN=1\n");
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        &format!(
            "symlink = [{{ source = \"{}\", target = \"secret\" }}]\n",
            toml_string_path(outside.path())
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
fn check_should_honor_target_boundary_environment_override_for_symlink() {
    let repo = git_worktree();
    let outside = tempfile::TempDir::new().expect("outside target dir should be created");
    let outside_target = outside.path().join("target");
    write_file(&repo.root_path().join(".env"), "TOKEN=1\n");
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        &format!(
            "symlink = [{{ source = \".env\", target = \"{}\" }}]\n",
            toml_string_path(&outside_target)
        ),
    );

    treeboot()
        .arg("check")
        .current_dir(repo.worktree_path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("outside worktree"));

    treeboot()
        .arg("check")
        .env(
            "TREEBOOT_DANGEROUSLY_ALLOW_TARGETS_OUTSIDE_WORKTREE",
            "true",
        )
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout("treeboot: check ok\n");

    assert!(!outside_target.exists());
}

#[test]
fn check_should_accept_absolute_paths_inside_root_and_worktree() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    let source = repo.root_path().join("shared/.env");
    let target = repo.worktree_path().join("local/.env");
    let app = repo.worktree_path().join("app");
    std::fs::create_dir_all(source.parent().expect("source should have parent"))
        .expect("source parent should be created");
    std::fs::create_dir_all(&app).expect("app dir should be created");
    write_file(&source, "TOKEN=1\n");
    write_file(
        &config,
        &format!(
            r#"
copy = [{{ source = "{}", target = "{}" }}]
commands = [{{ program = "git", args = ["--version"], cwd = "{}" }}]
"#,
            toml_string_path(&source),
            toml_string_path(&target),
            toml_string_path(&app),
        ),
    );

    treeboot()
        .arg("check")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout("treeboot: check ok\n");
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
fn check_should_ignore_executable_legacy_script_and_validate_config() {
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
    assert_eq!(json["action"]["kind"], "config");
    assert!(json["action"]["path"].is_string());

    assert!(!marker.exists());
}

#[cfg(unix)]
#[test]
fn check_config_option_should_ignore_legacy_script_and_validate_requested_config() {
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

#[test]
fn check_no_init_script_flag_should_be_usage_error() {
    let repo = git_worktree();

    treeboot()
        .args(["check", "--no-init-script"])
        .current_dir(repo.worktree_path())
        .assert()
        .code(2)
        .stderr(predicate::str::contains("unexpected argument"));
}

#[test]
fn check_should_warn_on_zero_match_include_and_stay_ok() {
    let repo = git_worktree();
    std::fs::create_dir_all(repo.root_path().join("shared")).expect("source should be created");
    write_file(&repo.root_path().join("shared/file.txt"), "data\n");
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        r#"copy = [{ source = "shared", include = ["docs/**"] }]"#,
    );

    treeboot()
        .arg("check")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "treeboot: warning: include patterns match no source paths for copy shared -> shared",
        ))
        .stdout(predicate::str::contains("treeboot: check ok"));
}

#[test]
fn check_json_should_carry_zero_match_include_warnings() {
    let repo = git_worktree();
    std::fs::create_dir_all(repo.root_path().join("shared")).expect("source should be created");
    write_file(&repo.root_path().join("shared/file.txt"), "data\n");
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        r#"copy = [{ source = "shared", include = ["docs/**"] }]"#,
    );

    let json = treeboot()
        .args(["check", "--json"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json = parse_json(json, "check");

    assert_json_object_keys(&json, &["action", "context", "warnings"]);
    let warnings = json["warnings"]
        .as_array()
        .expect("warnings should be an array");
    assert_eq!(warnings.len(), 1);
    assert!(
        warnings[0]
            .as_str()
            .expect("warning should be a string")
            .contains("include patterns match no source paths")
    );
}

#[test]
fn check_should_reject_include_with_sync_delete() {
    let repo = git_worktree();
    std::fs::create_dir_all(repo.root_path().join("shared")).expect("source should be created");
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        r#"sync = [{ source = "shared", include = ["docs/**"], delete = true }]"#,
    );

    treeboot()
        .arg("check")
        .current_dir(repo.worktree_path())
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "`include` cannot be combined with `delete = true`",
        ));
}

#[test]
fn check_yaml_should_carry_zero_match_include_warnings() {
    let repo = git_worktree();
    std::fs::create_dir_all(repo.root_path().join("shared")).expect("source should be created");
    write_file(&repo.root_path().join("shared/file.txt"), "data\n");
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        r#"copy = [{ source = "shared", include = ["docs/**"] }]"#,
    );

    treeboot()
        .args(["check", "--yaml"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("warnings:"))
        .stdout(predicate::str::contains(
            "include patterns match no source paths for copy shared -> shared",
        ));
}
