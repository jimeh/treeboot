use predicates::prelude::*;
use tempfile::TempDir;

mod common;

use common::{
    canonical_path, display_path, git_repo, git_worktree, symlink_dir, symlink_file,
    toml_string_path, treeboot, write_file,
};

#[cfg(unix)]
use common::write_executable_script;

#[test]
fn no_args_should_report_root_checkout_noop() {
    let repo = git_repo();

    treeboot()
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "treeboot: This is not a work tree",
        ));
}

#[test]
fn run_should_report_root_checkout_noop_like_no_args() {
    let repo = git_repo();

    treeboot()
        .arg("run")
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "treeboot: This is not a work tree",
        ));
}

#[test]
fn skip_commands_should_be_accepted_for_run() {
    let repo = git_repo();

    treeboot()
        .args(["run", "--skip-commands"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "treeboot: This is not a work tree",
        ));
}

#[test]
fn strict_missing_config_should_exit_with_runtime_failure() {
    let repo = git_worktree();

    treeboot()
        .arg("-S")
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stdout(predicate::str::contains("treeboot: no config detected"))
        .stderr(predicate::str::contains("treeboot: no config detected"));
}

#[test]
fn env_strict_missing_config_should_exit_with_runtime_failure() {
    let repo = git_worktree();

    treeboot()
        .env("TREEBOOT_STRICT", "yes")
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stdout(predicate::str::contains("treeboot: no config detected"))
        .stderr(predicate::str::contains("treeboot: no config detected"));
}

#[test]
fn strict_root_checkout_should_exit_with_runtime_failure() {
    let repo = git_repo();

    treeboot()
        .arg("--strict")
        .current_dir(repo.path())
        .assert()
        .code(1)
        .stdout(predicate::str::contains(
            "treeboot: This is not a work tree",
        ))
        .stderr(predicate::str::contains(
            "treeboot: This is not a work tree",
        ));
}

#[test]
fn env_strict_root_checkout_should_exit_with_runtime_failure() {
    let repo = git_repo();

    treeboot()
        .env("TREEBOOT_STRICT", "on")
        .current_dir(repo.path())
        .assert()
        .code(1)
        .stdout(predicate::str::contains(
            "treeboot: This is not a work tree",
        ))
        .stderr(predicate::str::contains(
            "treeboot: This is not a work tree",
        ));
}

#[test]
fn root_checkout_should_skip_config_detection() {
    let repo = git_repo();
    let config = repo.path().join(".treeboot.toml");
    write_file(&config, "commands = [\"mise install\"]\n");

    treeboot()
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "treeboot: This is not a work tree",
        ))
        .stdout(predicate::str::contains("treeboot: config detected").not());
}

#[cfg(unix)]
#[test]
fn root_checkout_should_skip_executable_init_script() {
    let repo = git_repo();
    let script = repo.path().join(".treeboot.sh");
    let marker = repo.path().join("script.out");
    write_executable_script(
        &script,
        &format!("#!/bin/sh\nprintf 'ran\\n' > {}\n", marker.display()),
    );

    treeboot()
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "treeboot: This is not a work tree",
        ))
        .stdout(predicate::str::contains("treeboot: run").not());

    assert!(!marker.exists());
}

#[test]
fn run_outside_git_worktree_should_exit_with_runtime_failure() {
    let dir = TempDir::new().expect("tempdir should be created");

    treeboot()
        .current_dir(dir.path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("not inside a Git worktree"));
}

#[cfg(unix)]
#[test]
fn run_command_only_config_should_execute_from_worktree() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    let marker = repo.worktree_path().join("pwd.out");
    write_file(
        &config,
        &format!(
            r#"commands = [{{ program = "sh", args = ["-c", "pwd > {}"] }}]"#,
            toml_string_path(&marker),
        ),
    );

    treeboot()
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: config detected"))
        .stdout(predicate::str::contains("treeboot: run sh -c"));

    let pwd = std::fs::read_to_string(marker).expect("pwd marker should be readable");
    let worktree = canonical_path(repo.worktree_path()).display().to_string();
    assert_eq!(pwd.trim(), worktree);
}

#[test]
fn run_should_apply_configured_file_ignore_patterns() {
    let repo = git_worktree();
    std::fs::create_dir_all(repo.root_path().join("shared/vendor/keep"))
        .expect("source directory should be created");
    write_file(&repo.root_path().join("shared/config"), "copy\n");
    write_file(&repo.root_path().join("shared/.DS_Store"), "keep\n");
    write_file(&repo.root_path().join("shared/vendor/drop"), "skip\n");
    write_file(
        &repo.root_path().join("shared/vendor/keep/config"),
        "keep\n",
    );
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        r#"
default_ignore = [".DS_Store"]
copy = [{ source = "shared", ignore = ["!.DS_Store", "**/vendor/**", "!**/vendor/keep/**"] }]
"#,
    );

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: copy shared -> shared"));

    assert_eq!(
        std::fs::read_to_string(repo.worktree_path().join("shared/config"))
            .expect("target should be readable"),
        "copy\n"
    );
    assert_eq!(
        std::fs::read_to_string(repo.worktree_path().join("shared/vendor/keep/config"))
            .expect("re-included target should be readable"),
        "keep\n"
    );
    assert_eq!(
        std::fs::read_to_string(repo.worktree_path().join("shared/.DS_Store"))
            .expect("default re-included target should be readable"),
        "keep\n"
    );
    assert!(!repo.worktree_path().join("shared/vendor/drop").exists());
}

#[test]
fn run_invalid_config_should_exit_with_config_error() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(
        &config,
        "commands = [{ run = \"npm\", program = \"npm\" }]\n",
    );

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stdout(predicate::str::contains("treeboot: config detected"))
        .stderr(predicate::str::contains("invalid config"))
        .stderr(predicate::str::contains("mutually exclusive"));
}

#[test]
fn run_duplicate_file_targets_should_exit_with_config_error() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(
        &config,
        r#"
copy = [
  { source = "a", target = ".env" },
  { source = "b", target = "./.env" },
]
"#,
    );

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("invalid config"))
        .stderr(predicate::str::contains("duplicate configured target"));
}

#[test]
fn run_should_compose_directory_targets_from_multiple_sources() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    std::fs::create_dir_all(repo.root_path().join("examples/config"))
        .expect("base config source should be created");
    std::fs::create_dir_all(repo.root_path().join("examples/config-addons"))
        .expect("addon source should be created");
    write_file(&repo.root_path().join("examples/config/base.yml"), "base\n");
    write_file(
        &repo.root_path().join("examples/config-addons/docker.yml"),
        "docker\n",
    );
    write_file(
        &config,
        r#"
copy = [
  { source = "examples/config", target = "config" },
  { source = "examples/config-addons/docker.yml", target = "config/docker.yml" },
]
"#,
    );

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty());

    assert_eq!(
        std::fs::read_to_string(repo.worktree_path().join("config/base.yml"))
            .expect("base config should be copied"),
        "base\n"
    );
    assert_eq!(
        std::fs::read_to_string(repo.worktree_path().join("config/docker.yml"))
            .expect("addon config should be copied"),
        "docker\n"
    );
}

#[test]
fn run_overlapping_sync_delete_targets_should_fail_before_side_effects() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(&repo.root_path().join("child"), "copied\n");
    std::fs::create_dir_all(repo.root_path().join("shared"))
        .expect("sync source should be created");
    write_file(
        &config,
        r#"
copy = [
  { source = "child", target = "shared/child" },
]
sync = [
  { source = "shared", target = "shared", delete = true },
]
"#,
    );

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("invalid config"))
        .stderr(predicate::str::contains("overlapping configured targets"));

    assert!(!repo.worktree_path().join("shared").exists());
    assert!(!repo.worktree_path().join("shared/child").exists());
}

#[test]
fn run_target_outside_worktree_should_exit_with_config_error() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(&config, r#"copy = [{ source = "a", target = "../.env" }]"#);

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("invalid config"))
        .stderr(predicate::str::contains("target resolves outside worktree"));
}

#[test]
fn run_source_outside_root_should_exit_with_config_error() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(
        &config,
        r#"copy = [{ source = "../shared", target = "shared" }]"#,
    );

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("invalid config"))
        .stderr(predicate::str::contains("source resolves outside root"));
}

#[test]
fn run_required_missing_source_should_exit_with_config_error() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(&config, r#"copy = [{ source = ".env", required = true }]"#);

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("invalid config"))
        .stderr(predicate::str::contains("required source does not exist"));
}

#[test]
fn run_strict_sync_should_exit_with_config_error() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(&config, r#"sync = ["shared"]"#);

    treeboot()
        .args(["run", "--strict"])
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("invalid config"))
        .stderr(predicate::str::contains("cannot be used with sync"));
}

#[test]
fn run_config_strict_sync_should_exit_with_config_error() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(
        &config,
        r#"
strict = true
sync = ["shared"]
"#,
    );

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("invalid config"))
        .stderr(predicate::str::contains("cannot be used with sync"));
}

#[test]
fn run_env_false_should_override_config_strict() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    std::fs::create_dir_all(repo.root_path().join("shared"))
        .expect("sync source should be created");
    write_file(
        &config,
        r#"
strict = true
sync = ["shared"]
"#,
    );

    treeboot()
        .arg("run")
        .env("TREEBOOT_STRICT", "off")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: sync shared -> shared"))
        .stderr(predicate::str::contains("invalid config").not());

    assert!(repo.worktree_path().join("shared").is_dir());
}

#[test]
fn run_cli_strict_should_override_env_false() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(
        &config,
        r#"
strict = false
sync = ["shared"]
"#,
    );

    treeboot()
        .args(["run", "--strict"])
        .env("TREEBOOT_STRICT", "false")
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("invalid config"))
        .stderr(predicate::str::contains("cannot be used with sync"));
}

#[test]
fn run_config_dangerous_source_option_should_allow_outside_source() {
    let repo = git_worktree();
    let outside = tempfile::NamedTempFile::new().expect("outside source should be created");
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(
        &config,
        &format!(
            r#"
dangerously_allow_sources_outside_root = true
copy = [{{ source = "{}", target = "outside" }}]
"#,
            toml_string_path(outside.path())
        ),
    );

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: copy"))
        .stderr(predicate::str::contains("invalid config").not());

    let copied = std::fs::read_to_string(repo.worktree_path().join("outside"))
        .expect("outside source should be copied");
    let source = std::fs::read_to_string(outside.path()).expect("source should be readable");
    assert_eq!(copied, source);
}

#[test]
fn run_env_dangerous_target_option_should_allow_outside_target() {
    let repo = git_worktree();
    let outside = TempDir::new().expect("outside target parent should be created");
    let source = repo.root_path().join("source");
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(&source, "value\n");
    write_file(
        &config,
        &format!(
            r#"
copy = [{{ source = "source", target = "{}" }}]
"#,
            toml_string_path(&outside.path().join("target"))
        ),
    );

    treeboot()
        .arg("run")
        .env("TREEBOOT_DANGEROUSLY_ALLOW_TARGETS_OUTSIDE_WORKTREE", "1")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: copy"))
        .stderr(predicate::str::contains("invalid config").not());

    let copied = std::fs::read_to_string(outside.path().join("target"))
        .expect("outside target should be copied");
    assert_eq!(copied, "value\n");
}

#[test]
fn run_should_accept_absolute_paths_inside_root_and_worktree() {
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
commands = [{{ program = "git", args = ["rev-parse", "--show-prefix"], cwd = "{}" }}]
"#,
            toml_string_path(&source),
            toml_string_path(&target),
            toml_string_path(&app),
        ),
    );

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("treeboot: copy"))
        .stdout(predicate::str::contains("app/"));

    let copied = std::fs::read_to_string(target).expect("absolute target should be copied");
    assert_eq!(copied, "TOKEN=1\n");
}

#[test]
fn run_dangerous_source_option_should_allow_symlink_to_outside_source() {
    let repo = git_worktree();
    let outside = tempfile::NamedTempFile::new().expect("outside source should be created");
    let config = repo.worktree_path().join(".treeboot.toml");
    let target = repo.worktree_path().join("outside.link");
    write_file(outside.path(), "value\n");
    write_file(
        &config,
        &format!(
            r#"
dangerously_allow_sources_outside_root = true
symlink = [{{ source = "{}", target = "outside.link" }}]
"#,
            toml_string_path(outside.path())
        ),
    );

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: symlink"))
        .stderr(predicate::str::contains("invalid config").not());

    assert!(
        std::fs::symlink_metadata(&target)
            .expect("target metadata should be readable")
            .file_type()
            .is_symlink()
    );
    assert_eq!(canonical_path(&target), canonical_path(outside.path()));
}

#[test]
fn run_dangerous_source_option_should_allow_existing_symlink_to_outside_source() {
    let repo = git_worktree();
    let outside = tempfile::NamedTempFile::new().expect("outside source should be created");
    let config = repo.worktree_path().join(".treeboot.toml");
    let target = repo.worktree_path().join("outside.link");
    write_file(outside.path(), "value\n");
    symlink_file(outside.path(), &target);
    write_file(
        &config,
        &format!(
            r#"
dangerously_allow_sources_outside_root = true
symlink = [{{ source = "{}", target = "outside.link" }}]
"#,
            toml_string_path(outside.path())
        ),
    );

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "treeboot: skip symlink outside.link; target exists",
        ))
        .stderr(predicate::str::contains("invalid config").not());

    assert_eq!(canonical_path(&target), canonical_path(outside.path()));
}

#[test]
fn run_dangerous_target_option_should_allow_symlink_outside_worktree() {
    let repo = git_worktree();
    let outside = TempDir::new().expect("outside target parent should be created");
    let source = repo.root_path().join("source");
    let target = outside.path().join("source.link");
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(&source, "value\n");
    write_file(
        &config,
        &format!(
            r#"
dangerously_allow_targets_outside_worktree = true
symlink = [{{ source = "source", target = "{}" }}]
"#,
            toml_string_path(&target)
        ),
    );

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: symlink source ->"))
        .stderr(predicate::str::contains("invalid config").not());

    assert!(
        std::fs::symlink_metadata(&target)
            .expect("target metadata should be readable")
            .file_type()
            .is_symlink()
    );
    assert_eq!(canonical_path(&target), canonical_path(&source));
}

#[test]
fn run_dangerous_target_option_should_allow_existing_symlink_outside_worktree() {
    let repo = git_worktree();
    let outside = TempDir::new().expect("outside target parent should be created");
    let source = repo.root_path().join("source");
    let target = outside.path().join("source.link");
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(&source, "value\n");
    symlink_file(&source, &target);
    write_file(
        &config,
        &format!(
            r#"
dangerously_allow_targets_outside_worktree = true
symlink = [{{ source = "source", target = "{}" }}]
"#,
            toml_string_path(&target)
        ),
    );

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: skip symlink"))
        .stdout(predicate::str::contains("target exists"))
        .stderr(predicate::str::contains("invalid config").not());

    assert_eq!(canonical_path(&target), canonical_path(&source));
}

#[test]
fn run_symlink_should_reject_target_parent_symlink_to_root_subdirectory() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    let source_dir = repo.root_path().join("config");
    std::fs::create_dir_all(&source_dir).expect("source dir should be created");
    write_file(&source_dir.join("master.key"), "secret\n");
    symlink_dir(&source_dir, repo.worktree_path().join("config"));
    write_file(&config, r#"symlink = ["config/master.key"]"#);

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("invalid config"))
        .stderr(predicate::str::contains(
            "target resolves outside worktree for symlink",
        ));
}

#[test]
fn run_symlink_should_reject_target_parent_file_in_subdirectory() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    let source = repo.root_path().join("config/master.key");
    std::fs::create_dir_all(source.parent().unwrap()).expect("source dir should be created");
    write_file(&source, "secret\n");
    write_file(&repo.worktree_path().join("config"), "not a directory\n");
    write_file(&config, r#"symlink = ["config/master.key"]"#);

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("invalid config"))
        .stderr(predicate::str::contains("cannot create target for symlink"))
        .stderr(predicate::str::contains("target parent"))
        .stderr(predicate::str::contains("is not a directory"));
}

#[test]
fn run_copy_should_reject_target_parent_symlink_to_root_subdirectory() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    let source_dir = repo.root_path().join("config");
    std::fs::create_dir_all(&source_dir).expect("source dir should be created");
    write_file(&source_dir.join("master.key"), "secret\n");
    symlink_dir(&source_dir, repo.worktree_path().join("config"));
    write_file(&config, r#"copy = ["config/master.key"]"#);

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("invalid config"))
        .stderr(predicate::str::contains(
            "target resolves outside worktree for copy",
        ));
}

#[test]
fn run_copy_should_reject_target_parent_file_in_subdirectory() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    let source = repo.root_path().join("config/master.key");
    std::fs::create_dir_all(source.parent().unwrap()).expect("source dir should be created");
    write_file(&source, "secret\n");
    write_file(&repo.worktree_path().join("config"), "not a directory\n");
    write_file(&config, r#"copy = ["config/master.key"]"#);

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("invalid config"))
        .stderr(predicate::str::contains("cannot create target for copy"))
        .stderr(predicate::str::contains("target parent"))
        .stderr(predicate::str::contains("is not a directory"));
}

#[test]
fn run_sync_should_reject_target_parent_symlink_to_root_subdirectory() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    let source = repo.root_path().join("templates/config");
    let linked = repo.root_path().join("config");
    std::fs::create_dir_all(&source).expect("source dir should be created");
    std::fs::create_dir_all(&linked).expect("linked dir should be created");
    write_file(&source.join("master.key"), "secret\n");
    symlink_dir(&linked, repo.worktree_path().join("config"));
    write_file(
        &config,
        r#"sync = [{ source = "templates/config", target = "config/app" }]"#,
    );

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("invalid config"))
        .stderr(predicate::str::contains(
            "target resolves outside worktree for sync",
        ));
}

#[test]
fn run_sync_should_reject_target_parent_file_in_subdirectory() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    let source = repo.root_path().join("templates/config");
    std::fs::create_dir_all(&source).expect("source dir should be created");
    write_file(&source.join("master.key"), "secret\n");
    write_file(&repo.worktree_path().join("config"), "not a directory\n");
    write_file(
        &config,
        r#"sync = [{ source = "templates/config", target = "config/app" }]"#,
    );

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("invalid config"))
        .stderr(predicate::str::contains("cannot create target for sync"))
        .stderr(predicate::str::contains("target parent"))
        .stderr(predicate::str::contains("is not a directory"));
}

#[test]
fn run_default_symlink_should_skip_existing_mismatched_subdirectory_symlink() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    let source = repo.root_path().join("config/master.key");
    let target = repo.worktree_path().join("config/master.key");
    std::fs::create_dir_all(source.parent().unwrap()).expect("source dir should be created");
    std::fs::create_dir_all(target.parent().unwrap()).expect("target dir should be created");
    write_file(&source, "new\n");
    write_file(&repo.worktree_path().join("config/old.key"), "old\n");
    symlink_file("old.key", &target);
    write_file(&config, r#"symlink = ["config/master.key"]"#);

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "treeboot: skip symlink config/master.key; target exists",
        ));

    assert_eq!(
        std::fs::read_link(&target).expect("target symlink should be readable"),
        std::path::PathBuf::from("old.key")
    );
}

#[test]
fn run_strict_existing_subdirectory_symlink_should_fail_before_mutation() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    let source = repo.root_path().join("config/master.key");
    let target = repo.worktree_path().join("config/master.key");
    std::fs::create_dir_all(source.parent().unwrap()).expect("source dir should be created");
    std::fs::create_dir_all(target.parent().unwrap()).expect("target dir should be created");
    write_file(&source, "new\n");
    write_file(&repo.worktree_path().join("config/old.key"), "old\n");
    symlink_file("old.key", &target);
    write_file(&config, r#"symlink = ["config/master.key"]"#);

    treeboot()
        .args(["run", "--strict"])
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("target exists"));

    assert_eq!(
        std::fs::read_link(&target).expect("target symlink should be readable"),
        std::path::PathBuf::from("old.key")
    );
}

#[test]
fn run_force_should_replace_existing_subdirectory_symlink() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    let source = repo.root_path().join("config/master.key");
    let target = repo.worktree_path().join("config/master.key");
    std::fs::create_dir_all(source.parent().unwrap()).expect("source dir should be created");
    std::fs::create_dir_all(target.parent().unwrap()).expect("target dir should be created");
    write_file(&source, "new\n");
    write_file(&repo.worktree_path().join("config/old.key"), "old\n");
    symlink_file("old.key", &target);
    write_file(&config, r#"symlink = ["config/master.key"]"#);

    treeboot()
        .args(["run", "--force"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "treeboot: symlink config/master.key -> config/master.key",
        ));

    assert_eq!(canonical_path(&target), canonical_path(&source));
}

#[test]
fn run_default_symlink_should_skip_broken_existing_subdirectory_symlink() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    let source = repo.root_path().join("config/master.key");
    let target = repo.worktree_path().join("config/master.key");
    std::fs::create_dir_all(source.parent().unwrap()).expect("source dir should be created");
    std::fs::create_dir_all(target.parent().unwrap()).expect("target dir should be created");
    write_file(&source, "new\n");
    symlink_file("missing.key", &target);
    write_file(&config, r#"symlink = ["config/master.key"]"#);

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "treeboot: skip symlink config/master.key; target exists",
        ));

    assert_eq!(
        std::fs::read_link(&target).expect("target symlink should be readable"),
        std::path::PathBuf::from("missing.key")
    );
}

#[test]
fn run_dry_run_symlink_should_handle_existing_and_missing_subdirectory_targets() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    let existing_source = repo.root_path().join("config/master.key");
    let missing_target_source = repo.root_path().join("config/other.key");
    let existing_target = repo.worktree_path().join("config/master.key");
    let missing_target = repo.worktree_path().join("config/other.key");
    std::fs::create_dir_all(existing_source.parent().unwrap())
        .expect("source dir should be created");
    std::fs::create_dir_all(existing_target.parent().unwrap())
        .expect("target dir should be created");
    write_file(&existing_source, "secret\n");
    write_file(&missing_target_source, "other\n");
    symlink_file(&existing_source, &existing_target);
    write_file(
        &config,
        r#"symlink = ["config/master.key", "config/other.key"]"#,
    );

    treeboot()
        .args(["run", "--dry-run"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "treeboot: would skip symlink config/master.key; target exists",
        ))
        .stdout(predicate::str::contains(
            "treeboot: would symlink config/other.key -> config/other.key",
        ));

    assert_eq!(
        canonical_path(&existing_target),
        canonical_path(&existing_source)
    );
    assert!(std::fs::symlink_metadata(missing_target).is_err());
}

#[test]
fn run_dangerous_source_option_should_skip_optional_missing_outside_symlink_source() {
    let repo = git_worktree();
    let outside = TempDir::new().expect("outside source parent should be created");
    let missing = outside.path().join("missing.key");
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(
        &config,
        &format!(
            r#"
dangerously_allow_sources_outside_root = true
symlink = [{{ source = "{}", target = "outside.link" }}]
"#,
            toml_string_path(&missing)
        ),
    );

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "treeboot: skip symlink outside.link; missing source",
        ));

    assert!(std::fs::symlink_metadata(repo.worktree_path().join("outside.link")).is_err());
}

#[test]
fn run_dangerous_source_option_should_reject_required_missing_outside_symlink_source() {
    let repo = git_worktree();
    let outside = TempDir::new().expect("outside source parent should be created");
    let missing = outside.path().join("missing.key");
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(
        &config,
        &format!(
            r#"
dangerously_allow_sources_outside_root = true
symlink = [{{ source = "{}", target = "outside.link", required = true }}]
"#,
            toml_string_path(&missing)
        ),
    );

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("required source does not exist"));

    assert!(std::fs::symlink_metadata(repo.worktree_path().join("outside.link")).is_err());
}

#[test]
fn run_dangerous_target_option_should_create_nested_parent_dirs_outside_worktree() {
    let repo = git_worktree();
    let outside = TempDir::new().expect("outside target parent should be created");
    let source = repo.root_path().join("source");
    let target = outside.path().join("nested/links/source.link");
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(&source, "value\n");
    write_file(
        &config,
        &format!(
            r#"
dangerously_allow_targets_outside_worktree = true
symlink = [{{ source = "source", target = "{}" }}]
"#,
            toml_string_path(&target)
        ),
    );

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: symlink source ->"));

    assert_eq!(canonical_path(&target), canonical_path(&source));
}

#[test]
fn run_dangerous_target_option_should_reject_existing_directory_target_outside_worktree() {
    let repo = git_worktree();
    let outside = TempDir::new().expect("outside target parent should be created");
    let source = repo.root_path().join("source");
    let target = outside.path().join("source.link");
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(&source, "value\n");
    std::fs::create_dir_all(&target).expect("outside target dir should be created");
    write_file(
        &config,
        &format!(
            r#"
dangerously_allow_targets_outside_worktree = true
symlink = [{{ source = "source", target = "{}" }}]
"#,
            toml_string_path(&target)
        ),
    );

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("target is a directory"));

    assert!(target.is_dir());
}

#[cfg(unix)]
#[test]
fn run_invalid_boolean_env_should_fail_before_init_script() {
    let repo = git_worktree();
    let script = repo.worktree_path().join(".treeboot.sh");
    let marker = repo.worktree_path().join("script.out");
    write_executable_script(
        &script,
        &format!("#!/bin/sh\nprintf 'ran\\n' > {}\n", marker.display()),
    );

    treeboot()
        .env("TREEBOOT_STRICT", "sometimes")
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "invalid boolean environment value for TREEBOOT_STRICT",
        ));

    assert!(!marker.exists());
}

#[test]
fn run_command_cwd_outside_worktree_should_exit_with_config_error() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(
        &config,
        r#"commands = [{ run = "pwd", cwd = "../outside" }]"#,
    );

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("invalid config"))
        .stderr(predicate::str::contains(
            "command cwd resolves outside worktree",
        ));
}

#[test]
fn run_command_env_owned_override_should_exit_with_config_error() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(
        &config,
        r#"commands = [{ run = "pwd", env = { TREEBOOT_ROOT_PATH = "/tmp" } }]"#,
    );

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("invalid config"))
        .stderr(predicate::str::contains(
            "overrides treeboot-owned variable",
        ));
}

#[test]
fn run_unsafe_source_symlink_should_exit_with_config_error() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    let source_dir = repo.root_path().join("shared");
    let outside = tempfile::NamedTempFile::new().expect("outside file should be created");
    std::fs::create_dir_all(&source_dir).expect("source dir should be created");
    symlink_file(outside.path(), source_dir.join("outside"));
    write_file(
        &config,
        r#"copy = [{ source = "shared", target = "shared" }]"#,
    );

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("invalid config"))
        .stderr(predicate::str::contains("unsafe symlink"));
}

#[test]
fn run_file_only_config_should_copy_file() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(&repo.root_path().join(".env"), "TOKEN=1\n");
    write_file(&config, r#"copy = [".env"]"#);

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: copy .env -> .env"));

    let copied = std::fs::read_to_string(repo.worktree_path().join(".env"))
        .expect("copied file should be readable");
    assert_eq!(copied, "TOKEN=1\n");
}

#[test]
fn run_optional_missing_source_should_report_skip() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(&config, r#"copy = [".env.local"]"#);

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "treeboot: skip copy .env.local; missing source",
        ));

    assert!(!repo.worktree_path().join(".env.local").exists());
}

#[cfg(unix)]
#[test]
fn run_config_with_commands_should_apply_files_before_commands() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    let marker = repo.worktree_path().join("command.out");
    write_file(&repo.root_path().join(".env"), "TOKEN=1\n");
    write_file(
        &config,
        &format!(
            "copy = [\".env\"]\ncommands = [{{ program = \"sh\", args = [\"-c\", \"cat .env > {}\"] }}]\n",
            toml_string_path(&marker),
        ),
    );

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: copy .env -> .env"))
        .stdout(predicate::str::contains("treeboot: run sh -c"));

    assert_eq!(
        std::fs::read_to_string(repo.worktree_path().join(".env"))
            .expect("copied file should be readable"),
        "TOKEN=1\n"
    );
    assert_eq!(
        std::fs::read_to_string(marker).expect("command marker should be readable"),
        "TOKEN=1\n"
    );
}

#[cfg(unix)]
#[test]
fn run_file_failure_should_prevent_command_execution() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    let marker = repo.worktree_path().join("command.out");
    write_file(&repo.root_path().join(".env"), "TOKEN=1\n");
    std::fs::create_dir_all(repo.worktree_path().join("target"))
        .expect("target dir should be created");
    write_file(
        &config,
        &format!(
            "copy = [{{ source = \".env\", target = \"target\" }}]\ncommands = [{{ program = \"sh\", args = [\"-c\", \"touch {}\"] }}]\n",
            toml_string_path(&marker),
        ),
    );

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("file operation cannot use"));

    assert!(!marker.exists());
}

#[test]
fn run_sync_config_should_copy_then_sync() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    std::fs::create_dir_all(repo.root_path().join("shared"))
        .expect("sync source should be created");
    write_file(&repo.root_path().join(".env"), "TOKEN=1\n");
    write_file(&repo.root_path().join("shared/config"), "value\n");
    write_file(&config, "copy = [\".env\"]\nsync = [\"shared\"]\n");

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: copy .env -> .env"))
        .stdout(predicate::str::contains(
            "treeboot: sync shared -> shared (2 changed)",
        ));

    let copied = std::fs::read_to_string(repo.worktree_path().join(".env"))
        .expect("copied file should be readable");
    let synced = std::fs::read_to_string(repo.worktree_path().join("shared/config"))
        .expect("synced file should be readable");
    assert_eq!(copied, "TOKEN=1\n");
    assert_eq!(synced, "value\n");
}

#[test]
fn run_verbose_sync_should_report_concrete_file_actions() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    std::fs::create_dir_all(repo.root_path().join("shared"))
        .expect("sync source should be created");
    write_file(&repo.root_path().join("shared/config"), "value\n");
    write_file(&config, "sync = [\"shared\"]\n");

    treeboot()
        .args(["run", "--verbose"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "treeboot: sync {} -> {}",
            display_path("shared/config"),
            display_path("shared/config")
        )));
}

#[test]
fn run_sync_delete_should_remove_target_only_file() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    std::fs::create_dir_all(repo.root_path().join("shared"))
        .expect("sync source should be created");
    std::fs::create_dir_all(repo.worktree_path().join("shared"))
        .expect("sync target should be created");
    write_file(&repo.root_path().join("shared/config"), "value\n");
    write_file(&repo.worktree_path().join("shared/old"), "remove\n");
    write_file(
        &config,
        r#"sync = [{ source = "shared", target = "shared", delete = true }]"#,
    );

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "treeboot: sync shared -> shared (1 changed, 1 deleted)",
        ));

    assert!(!repo.worktree_path().join("shared/old").exists());
}

#[test]
fn run_sync_delete_should_preserve_default_ignored_target_only_file() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    std::fs::create_dir_all(repo.root_path().join("shared"))
        .expect("sync source should be created");
    std::fs::create_dir_all(repo.worktree_path().join("shared"))
        .expect("sync target should be created");
    write_file(&repo.root_path().join("shared/config"), "value\n");
    write_file(&repo.worktree_path().join("shared/old"), "remove\n");
    write_file(&repo.worktree_path().join("shared/.DS_Store"), "keep\n");
    write_file(
        &config,
        r#"
default_ignore = [".DS_Store"]
sync = [{ source = "shared", target = "shared", delete = true }]
"#,
    );

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .success();

    assert!(!repo.worktree_path().join("shared/old").exists());
    assert_eq!(
        std::fs::read_to_string(repo.worktree_path().join("shared/.DS_Store"))
            .expect("default-ignored target-only file should be readable"),
        "keep\n"
    );
}

#[test]
fn run_sync_should_preserve_target_only_file_by_default() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    std::fs::create_dir_all(repo.root_path().join("shared"))
        .expect("sync source should be created");
    std::fs::create_dir_all(repo.worktree_path().join("shared"))
        .expect("sync target should be created");
    write_file(&repo.root_path().join("shared/config"), "value\n");
    write_file(&repo.worktree_path().join("shared/old"), "keep\n");
    write_file(
        &config,
        r#"sync = [{ source = "shared", target = "shared" }]"#,
    );

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: delete").not());

    let extra = std::fs::read_to_string(repo.worktree_path().join("shared/old"))
        .expect("target-only file should remain readable");
    assert_eq!(extra, "keep\n");
}

#[test]
fn run_dry_run_sync_delete_should_not_remove_target_only_file() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    std::fs::create_dir_all(repo.root_path().join("shared"))
        .expect("sync source should be created");
    std::fs::create_dir_all(repo.worktree_path().join("shared"))
        .expect("sync target should be created");
    write_file(&repo.worktree_path().join("shared/old"), "keep\n");
    write_file(
        &config,
        r#"sync = [{ source = "shared", target = "shared", delete = true }]"#,
    );

    treeboot()
        .args(["run", "--dry-run"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "treeboot: would sync shared -> shared (1 delete)",
        ));

    let extra = std::fs::read_to_string(repo.worktree_path().join("shared/old"))
        .expect("target-only file should remain readable");
    assert_eq!(extra, "keep\n");
}

#[cfg(unix)]
#[test]
fn run_dry_run_sync_metadata_should_not_mutate_directory_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    let source = repo.root_path().join("shared");
    let target = repo.worktree_path().join("shared");
    std::fs::create_dir_all(&source).expect("sync source should be created");
    std::fs::create_dir_all(&target).expect("sync target should be created");
    let mut source_permissions = std::fs::metadata(&source)
        .expect("source metadata should be readable")
        .permissions();
    source_permissions.set_mode(0o700);
    std::fs::set_permissions(&source, source_permissions).expect("source mode should be set");
    let mut target_permissions = std::fs::metadata(&target)
        .expect("target metadata should be readable")
        .permissions();
    target_permissions.set_mode(0o755);
    std::fs::set_permissions(&target, target_permissions).expect("target mode should be set");
    write_file(
        &config,
        r#"sync = [{ source = "shared", target = "shared" }]"#,
    );

    treeboot()
        .args(["run", "--dry-run"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "treeboot: would sync metadata shared -> shared",
        ));

    let mode = std::fs::metadata(&target)
        .expect("target metadata should be readable")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o755);
}

#[test]
fn run_checksum_sync_should_update_when_metadata_matches() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    let source = repo.root_path().join("shared/config");
    let target = repo.worktree_path().join("shared/config");
    std::fs::create_dir_all(repo.root_path().join("shared"))
        .expect("sync source should be created");
    std::fs::create_dir_all(repo.worktree_path().join("shared"))
        .expect("sync target should be created");
    write_file(&source, "new\n");
    write_file(&target, "old\n");
    let modified = std::fs::metadata(&source)
        .expect("source metadata should be readable")
        .modified()
        .expect("source mtime should be readable");
    let times = std::fs::FileTimes::new().set_modified(modified);
    std::fs::File::options()
        .write(true)
        .open(&target)
        .expect("target should be opened")
        .set_times(times)
        .expect("target mtime should match source");
    write_file(
        &config,
        r#"sync = [{ source = "shared/config", target = "shared/config", compare = "checksum" }]"#,
    );

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "treeboot: sync shared/config -> shared/config",
        ));

    let synced = std::fs::read_to_string(target).expect("target should be readable");
    assert_eq!(synced, "new\n");
}

#[test]
fn run_config_with_commands_and_skip_commands_should_copy_file() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(&repo.root_path().join(".env"), "TOKEN=1\n");
    write_file(
        &config,
        "copy = [\".env\"]\ncommands = [\"mise install\"]\n",
    );

    treeboot()
        .args(["run", "--skip-commands"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: copy .env -> .env"));

    let copied = std::fs::read_to_string(repo.worktree_path().join(".env"))
        .expect("copied file should be readable");
    assert_eq!(copied, "TOKEN=1\n");
}

#[cfg(unix)]
#[test]
fn run_skip_commands_should_not_spawn_failing_command() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(&repo.root_path().join(".env"), "TOKEN=1\n");
    write_file(
        &config,
        r#"copy = [".env"]
commands = [{ name = "missing", program = "treeboot-missing-program-for-test" }]
"#,
    );

    treeboot()
        .args(["run", "--skip-commands"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: copy .env -> .env"))
        .stdout(predicate::str::contains("treeboot: run missing").not())
        .stderr(predicate::str::contains("failed to run command").not());

    assert!(repo.worktree_path().join(".env").exists());
}

#[cfg(unix)]
#[test]
fn run_dry_run_should_report_files_and_commands_without_side_effects() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    let marker = repo.worktree_path().join("command.out");
    write_file(&repo.root_path().join(".env"), "TOKEN=1\n");
    write_file(
        &config,
        &format!(
            "copy = [\".env\"]\ncommands = [{{ name = \"mark\", program = \"sh\", args = [\"-c\", \"touch {}\"] }}]\n",
            toml_string_path(&marker),
        ),
    );

    treeboot()
        .args(["run", "--dry-run"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "treeboot: would copy .env -> .env",
        ))
        .stdout(predicate::str::contains("treeboot: would run mark: sh -c"));

    assert!(!repo.worktree_path().join(".env").exists());
    assert!(!marker.exists());
}

#[cfg(unix)]
#[test]
fn run_commands_should_isolate_env_and_honor_cwd() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    let app = repo.worktree_path().join("app");
    let first = repo.worktree_path().join("first.out");
    let second = repo.worktree_path().join("second.out");
    std::fs::create_dir_all(&app).expect("app dir should be created");
    write_file(
        &config,
        &format!(
            r#"
[[command]]
name = "env one"
program = "sh"
args = ["-c", "printf '%s\n%s\n%s\n%s\n' \"$LOCAL_VALUE\" \"$TREEBOOT_ROOT_PATH\" \"$TREEBOOT_WORKTREE_PATH\" \"$(pwd)\" > {}"]
cwd = "app"
env = {{ LOCAL_VALUE = "local" }}

[[command]]
name = "env two"
program = "sh"
args = ["-c", "printf '%s\n' \"$LOCAL_VALUE\" > {}"]
"#,
            toml_string_path(&first),
            toml_string_path(&second),
        ),
    );

    treeboot()
        .env("LOCAL_VALUE", "outer")
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .success();

    let first = std::fs::read_to_string(first).expect("first marker should be readable");
    let lines = first.lines().collect::<Vec<_>>();
    assert_eq!(lines[0], "local");
    let root = canonical_path(repo.root_path()).display().to_string();
    let worktree = canonical_path(repo.worktree_path()).display().to_string();
    let app = canonical_path(&app).display().to_string();
    assert_eq!(lines[1], root);
    assert_eq!(lines[2], worktree);
    assert_eq!(lines[3], app);
    assert_eq!(
        std::fs::read_to_string(second).expect("second marker should be readable"),
        "outer\n"
    );
}

#[cfg(unix)]
#[test]
fn run_direct_program_args_should_preserve_argument_boundaries() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    let marker = repo.worktree_path().join("args.out");
    write_file(
        &config,
        &format!(
            r#"
commands = [{{
  program = "sh",
  args = ["-c", "printf '%s\n%s\n' \"$1\" \"$2\" > {}", "helper", "one two", "three"]
}}]
"#,
            toml_string_path(&marker),
        ),
    );

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .success();

    assert_eq!(
        std::fs::read_to_string(marker).expect("args marker should be readable"),
        "one two\nthree\n"
    );
}

#[cfg(unix)]
#[test]
fn run_should_reject_async_command_field() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(
        &config,
        r#"
commands = [{ run = "npm install", async = true }]
"#,
    );

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown field"));
}

#[cfg(unix)]
#[test]
fn run_fatal_command_failure_should_keep_file_side_effects_and_stop_later_commands() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    let marker = repo.worktree_path().join("later.out");
    write_file(&repo.root_path().join(".env"), "TOKEN=1\n");
    write_file(
        &config,
        &format!(
            r#"
copy = [".env"]

[[command]]
name = "fail"
run = "exit 9"

[[command]]
name = "later"
run = "touch {}"
"#,
            toml_string_path(&marker),
        ),
    );

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stdout(predicate::str::contains("treeboot: copy .env -> .env"))
        .stderr(predicate::str::contains(
            "treeboot: command fail: exit 9 failed with exit status: 9",
        ));

    assert!(repo.worktree_path().join(".env").exists());
    assert!(!marker.exists());
}

#[cfg(unix)]
#[test]
fn run_allowed_command_failure_should_warn_and_continue() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    let marker = repo.worktree_path().join("later.out");
    write_file(
        &config,
        &format!(
            r#"
[[command]]
name = "optional"
program = "treeboot-missing-program-for-test"
allow_failure = true

[[command]]
name = "later"
run = "touch {}"
"#,
            toml_string_path(&marker),
        ),
    );

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "treeboot: warning: command optional: treeboot-missing-program-for-test failed to start:",
        ))
        .stdout(predicate::str::contains("treeboot: run later:"));

    assert!(marker.exists());
}

#[cfg(unix)]
#[test]
fn run_fatal_spawn_failure_should_exit_nonzero() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(
        &config,
        r#"commands = [{ name = "missing", program = "treeboot-missing-program-for-test" }]"#,
    );

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stdout(predicate::str::contains(
            "treeboot: run missing: treeboot-missing-program-for-test",
        ))
        .stderr(predicate::str::contains(
            "treeboot: failed to run command missing: treeboot-missing-program-for-test",
        ));
}

#[test]
fn run_sync_config_with_commands_and_skip_commands_should_sync_file() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    std::fs::create_dir_all(repo.root_path().join("shared"))
        .expect("sync source should be created");
    write_file(&repo.root_path().join("shared/config"), "value\n");
    write_file(
        &config,
        "sync = [\"shared\"]\ncommands = [\"mise install\"]\n",
    );

    treeboot()
        .args(["run", "--skip-commands"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "treeboot: sync shared -> shared (2 changed)",
        ));

    let synced = std::fs::read_to_string(repo.worktree_path().join("shared/config"))
        .expect("synced file should be readable");
    assert_eq!(synced, "value\n");
}

#[test]
fn run_strict_existing_copy_target_should_fail_before_mutation() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(&repo.root_path().join(".env"), "new\n");
    write_file(&repo.worktree_path().join(".env"), "old\n");
    write_file(&config, r#"copy = [".env"]"#);

    treeboot()
        .args(["run", "--strict"])
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("target exists"));

    let existing = std::fs::read_to_string(repo.worktree_path().join(".env"))
        .expect("existing target should be readable");
    assert_eq!(existing, "old\n");
}

#[test]
fn run_force_copy_should_replace_existing_target() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(&repo.root_path().join(".env"), "new\n");
    write_file(&repo.worktree_path().join(".env"), "old\n");
    write_file(&config, r#"copy = [".env"]"#);

    treeboot()
        .args(["run", "--force"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: copy .env -> .env"));

    let replaced = std::fs::read_to_string(repo.worktree_path().join(".env"))
        .expect("replaced target should be readable");
    assert_eq!(replaced, "new\n");
}

#[test]
fn run_dry_run_copy_should_not_mutate_target() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(&repo.root_path().join(".env"), "TOKEN=1\n");
    write_file(&config, r#"copy = [".env"]"#);

    treeboot()
        .args(["run", "--dry-run"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "treeboot: would copy .env -> .env",
        ));

    assert!(!repo.worktree_path().join(".env").exists());
}

#[test]
fn run_default_copy_should_skip_existing_target() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(&repo.root_path().join(".env"), "new\n");
    write_file(&repo.worktree_path().join(".env"), "old\n");
    write_file(&config, r#"copy = [".env"]"#);

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "treeboot: skip copy .env; target exists",
        ));

    let existing = std::fs::read_to_string(repo.worktree_path().join(".env"))
        .expect("existing target should be readable");
    assert_eq!(existing, "old\n");
}

#[test]
fn run_copied_symlink_should_warn_when_final_target_is_missing() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    std::fs::create_dir_all(repo.root_path().join("shared")).expect("source dir should be created");
    write_file(&repo.root_path().join("shared/config"), "value\n");
    symlink_file("config", repo.root_path().join("shared/link"));
    write_file(
        &config,
        r#"copy = [{ source = "shared/link", target = "shared/link" }]"#,
    );

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "treeboot: warning: shared/link symlink target does not exist",
        ));

    let link = std::fs::read_link(repo.worktree_path().join("shared/link"))
        .expect("copied symlink should exist");
    assert_eq!(link, std::path::PathBuf::from("config"));
}

#[test]
fn run_symlink_should_create_relative_symlink() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    let source = repo.root_path().join("tool");
    let target = repo.worktree_path().join(".tool");
    write_file(&source, "tool\n");
    write_file(
        &config,
        r#"symlink = [{ source = "tool", target = ".tool" }]"#,
    );

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: symlink tool -> .tool"));

    let link = std::fs::read_link(&target).expect("target should be a symlink");
    let resolved = canonical_path(&target.parent().unwrap().join(&link));
    let expected = canonical_path(&source);
    assert!(!link.is_absolute());
    assert_eq!(resolved, expected);
}

#[test]
fn run_symlink_should_allow_existing_link_to_root_source_in_subdirectory() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    let source = repo.root_path().join("config/master.key");
    let target = repo.worktree_path().join("config/master.key");
    std::fs::create_dir_all(source.parent().unwrap()).expect("source dir should be created");
    std::fs::create_dir_all(target.parent().unwrap()).expect("target dir should be created");
    write_file(&source, "secret\n");
    symlink_file(&source, &target);
    write_file(&config, r#"symlink = ["config/master.key"]"#);

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "treeboot: skip symlink config/master.key; target exists",
        ));

    assert!(
        std::fs::symlink_metadata(&target)
            .expect("target metadata should be readable")
            .file_type()
            .is_symlink()
    );
    assert_eq!(canonical_path(&target), canonical_path(&source));
}

#[test]
fn run_file_operations_should_apply_sources_in_subdirectories() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    let copy_source = repo.root_path().join("templates/env.local");
    let symlink_source = repo.root_path().join("config/master.key");
    let sync_source = repo.root_path().join("shared/config");
    std::fs::create_dir_all(copy_source.parent().unwrap())
        .expect("copy source dir should be created");
    std::fs::create_dir_all(symlink_source.parent().unwrap())
        .expect("symlink source dir should be created");
    std::fs::create_dir_all(sync_source.parent().unwrap())
        .expect("sync source dir should be created");
    write_file(&copy_source, "TOKEN=1\n");
    write_file(&symlink_source, "secret\n");
    write_file(&sync_source, "value\n");
    write_file(
        &config,
        r#"
copy = [{ source = "templates/env.local", target = ".env.local" }]
symlink = ["config/master.key"]
sync = ["shared/config"]
"#,
    );

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "treeboot: copy templates/env.local -> .env.local",
        ))
        .stdout(predicate::str::contains(
            "treeboot: symlink config/master.key -> config/master.key",
        ))
        .stdout(predicate::str::contains(
            "treeboot: sync shared/config -> shared/config",
        ));

    assert_eq!(
        std::fs::read_to_string(repo.worktree_path().join(".env.local"))
            .expect("copied file should be readable"),
        "TOKEN=1\n"
    );
    assert_eq!(
        canonical_path(&repo.worktree_path().join("config/master.key")),
        canonical_path(&symlink_source)
    );
    assert_eq!(
        std::fs::read_to_string(repo.worktree_path().join("shared/config"))
            .expect("synced file should be readable"),
        "value\n"
    );
}

#[cfg(unix)]
#[test]
fn root_option_should_set_script_root_env() {
    let repo = git_worktree();
    let root = TempDir::new().expect("root tempdir should be created");
    let output = repo.worktree_path().join("root.out");
    let script = repo.worktree_path().join(".treeboot.sh");
    write_executable_script(
        &script,
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$TREEBOOT_ROOT_PATH\" > {}\n",
            output.display()
        ),
    );

    treeboot()
        .arg("-r")
        .arg(root.path())
        .current_dir(repo.worktree_path())
        .assert()
        .success();

    let script_output = std::fs::read_to_string(output).expect("script output should exist");
    let root_path = canonical_path(root.path());
    assert_eq!(script_output, format!("{}\n", root_path.display()));
}

#[cfg(unix)]
#[test]
fn conductor_default_branch_env_should_set_script_branch_env() {
    let repo = git_worktree();
    let output = repo.worktree_path().join("branch.out");
    let script = repo.worktree_path().join(".treeboot.sh");
    write_executable_script(
        &script,
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$TREEBOOT_DEFAULT_BRANCH\" > {}\n",
            output.display()
        ),
    );

    treeboot()
        .env("CONDUCTOR_DEFAULT_BRANCH", "series-1.2")
        .current_dir(repo.worktree_path())
        .assert()
        .success();

    let script_output = std::fs::read_to_string(output).expect("script output should exist");
    assert_eq!(script_output, "series-1.2\n");
}

#[cfg(unix)]
#[test]
fn executable_init_script_should_win_over_missing_config() {
    let repo = git_worktree();
    let script = repo.worktree_path().join(".treeboot.sh");
    let output = repo.worktree_path().join("script.out");
    write_executable_script(
        &script,
        &format!(
            "#!/bin/sh\nprintf '%s:%s\\n' \"$TREEBOOT_ROOT_PATH\" \
             \"$CODEX_WORKTREE_PATH\" > {}\n",
            output.display()
        ),
    );

    treeboot()
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: run"));

    let script_output = std::fs::read_to_string(output).expect("script output should exist");
    let root_path = canonical_path(repo.root_path());
    let worktree_path = canonical_path(repo.worktree_path());
    let expected = format!("{}:{}\n", root_path.display(), worktree_path.display());
    assert_eq!(script_output, expected);
}

#[cfg(unix)]
#[test]
fn dry_run_init_script_should_not_execute_script() {
    let repo = git_worktree();
    let script = repo.worktree_path().join(".treeboot.sh");
    let output = repo.worktree_path().join("script.out");
    write_executable_script(
        &script,
        &format!("#!/bin/sh\nprintf 'ran\\n' > {}\n", output.display()),
    );

    treeboot()
        .arg("-n")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: would run"));

    assert!(!output.exists());
}

#[cfg(unix)]
#[test]
fn failing_init_script_should_exit_with_runtime_failure() {
    let repo = git_worktree();
    let script = repo.worktree_path().join(".treeboot.sh");
    write_executable_script(&script, "#!/bin/sh\nexit 7\n");

    treeboot()
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("init script"))
        .stderr(predicate::str::contains("failed"));
}

#[cfg(unix)]
#[test]
fn config_option_should_skip_executable_script_discovery() {
    let repo = git_worktree();
    let config = repo.worktree_path().join("custom.treeboot.toml");
    let script = repo.worktree_path().join(".treeboot.sh");
    let marker = repo.worktree_path().join("script.out");
    write_file(&config, "commands = []\n");
    write_executable_script(
        &script,
        &format!("#!/bin/sh\nprintf 'ran\\n' > {}\n", marker.display()),
    );

    treeboot()
        .args(["-c", "custom.treeboot.toml"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: config detected"));

    assert!(!marker.exists());
}

#[cfg(unix)]
#[test]
fn no_init_script_should_skip_executable_script_and_discover_config() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    let script = repo.worktree_path().join(".treeboot.sh");
    let marker = repo.worktree_path().join("script.out");
    write_file(&config, "commands = []\n");
    write_executable_script(
        &script,
        &format!("#!/bin/sh\nprintf 'ran\\n' > {}\n", marker.display()),
    );

    treeboot()
        .args(["--no-init-script"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: config detected"));

    assert!(!marker.exists());
}

#[cfg(unix)]
#[test]
fn non_executable_init_script_should_be_ignored() {
    let repo = git_worktree();
    write_file(
        &repo.worktree_path().join(".treeboot.sh"),
        "#!/bin/sh\nexit 1\n",
    );

    treeboot()
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: ignore"))
        .stdout(predicate::str::contains("treeboot: no config detected"));
}
