use predicates::prelude::*;
use tempfile::TempDir;

mod common;

use common::{git_repo, git_worktree, treeboot, write_file};

#[cfg(unix)]
use common::write_executable_script;

fn toml_string_path(path: &std::path::Path) -> String {
    path.display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

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

#[test]
fn config_file_should_be_detected_until_config_execution_exists() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(&config, "commands = [\"mise install\"]\n");

    treeboot()
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stdout(predicate::str::contains("treeboot: config detected"))
        .stderr(predicate::str::contains(
            "declarative config execution is not implemented yet",
        ));
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
        .code(1)
        .stderr(predicate::str::contains("invalid config").not())
        .stderr(predicate::str::contains(
            "declarative config execution is not implemented",
        ));
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
        .code(1)
        .stderr(predicate::str::contains("invalid config").not())
        .stderr(predicate::str::contains(
            "declarative config execution is not implemented",
        ));
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
        .code(1)
        .stderr(predicate::str::contains("invalid config").not())
        .stderr(predicate::str::contains(
            "declarative config execution is not implemented",
        ));
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

#[cfg(unix)]
#[test]
fn run_unsafe_source_symlink_should_exit_with_config_error() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    let source_dir = repo.root_path().join("shared");
    let outside = tempfile::NamedTempFile::new().expect("outside file should be created");
    std::fs::create_dir_all(&source_dir).expect("source dir should be created");
    std::os::unix::fs::symlink(outside.path(), source_dir.join("outside"))
        .expect("source symlink should be created");
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
    let root_path = std::fs::canonicalize(root.path()).expect("root should normalize");
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
    let root_path = std::fs::canonicalize(repo.root_path()).expect("root path should normalize");
    let worktree_path =
        std::fs::canonicalize(repo.worktree_path()).expect("worktree path should normalize");
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
        .code(1)
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
