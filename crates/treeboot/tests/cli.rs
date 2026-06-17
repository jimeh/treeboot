use predicates::prelude::*;
use tempfile::TempDir;

mod common;

use common::{git_repo, treeboot, write_file};

#[cfg(unix)]
use common::write_executable_script;

#[test]
fn help_should_print_usage() {
    treeboot()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage: treeboot"));
}

#[test]
fn version_should_print_package_version() {
    treeboot()
        .arg("--version")
        .assert()
        .success()
        .stdout(format!("treeboot {}\n", env!("CARGO_PKG_VERSION")));
}

#[test]
fn no_args_should_run_and_report_missing_config() {
    let repo = git_repo();

    treeboot()
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: no config detected"));
}

#[test]
fn run_should_report_missing_config_like_no_args() {
    let repo = git_repo();

    treeboot()
        .arg("run")
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: no config detected"));
}

#[test]
fn strict_missing_config_should_exit_with_runtime_failure() {
    let repo = git_repo();

    treeboot()
        .arg("--strict")
        .current_dir(repo.path())
        .assert()
        .code(1)
        .stdout(predicate::str::contains("treeboot: no config detected"))
        .stderr(predicate::str::contains("treeboot: no config detected"));
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
    let repo = git_repo();
    let config = repo.path().join(".treeboot.toml");
    write_file(&config, "commands = [\"mise install\"]\n");

    treeboot()
        .current_dir(repo.path())
        .assert()
        .code(1)
        .stdout(predicate::str::contains("treeboot: config detected"))
        .stderr(predicate::str::contains(
            "declarative config execution is not implemented yet",
        ));
}

#[test]
fn run_invalid_config_should_exit_with_config_error() {
    let repo = git_repo();
    let config = repo.path().join(".treeboot.toml");
    write_file(
        &config,
        "commands = [{ run = \"npm\", program = \"npm\" }]\n",
    );

    treeboot()
        .arg("run")
        .current_dir(repo.path())
        .assert()
        .code(1)
        .stdout(predicate::str::contains("treeboot: config detected"))
        .stderr(predicate::str::contains("invalid config"))
        .stderr(predicate::str::contains("mutually exclusive"));
}

#[test]
fn config_command_should_print_normalized_config() {
    let repo = git_repo();
    let config = repo.path().join(".treeboot.toml");
    write_file(
        &config,
        r#"
copy = [{ source = ".env.local" }]
sync = ["shared/config"]
commands = ["mise install"]
"#,
    );

    treeboot()
        .arg("config")
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: config"))
        .stdout(predicate::str::contains("copy .env.local -> .env.local"))
        .stdout(predicate::str::contains(
            "sync shared/config -> shared/config compare=metadata delete_extra=true",
        ))
        .stdout(predicate::str::contains("run \"mise install\""));
}

#[test]
fn config_command_json_should_print_normalized_config() {
    let repo = git_repo();
    let config = repo.path().join(".treeboot.toml");
    write_file(&config, "commands = [\"mise install\"]\n");

    treeboot()
        .args(["config", "--format", "json"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"commands\""))
        .stdout(predicate::str::contains("\"run\": \"mise install\""));
}

#[test]
fn config_command_missing_config_should_exit_with_runtime_failure() {
    let repo = git_repo();

    treeboot()
        .arg("config")
        .current_dir(repo.path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("treeboot: no config detected"));
}

#[test]
fn config_command_config_option_should_use_requested_file() {
    let repo = git_repo();
    let default_config = repo.path().join(".treeboot.toml");
    let requested_config = repo.path().join("custom.treeboot.toml");
    write_file(&default_config, "commands = [\"default\"]\n");
    write_file(
        &requested_config,
        "commands = [{ program = \"npm\", args = [\"install\"] }]\n",
    );

    treeboot()
        .args(["config", "--config", "custom.treeboot.toml"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("custom.treeboot.toml"))
        .stdout(predicate::str::contains("exec npm install"))
        .stdout(predicate::str::contains("default").not());
}

#[test]
fn config_command_root_option_should_resolve_json_source_paths() {
    let repo = git_repo();
    let root = TempDir::new().expect("root tempdir should be created");
    let config = repo.path().join(".treeboot.toml");
    write_file(&config, "copy = [\"shared/.env\"]\n");

    let root_path = std::fs::canonicalize(root.path()).expect("root should normalize");
    let source_path = root_path.join("shared/.env").display().to_string();
    let source_path_json = source_path.replace('\\', "\\\\");

    treeboot()
        .args(["config", "--root"])
        .arg(root.path())
        .arg("--format")
        .arg("json")
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"source_path\""))
        .stdout(predicate::str::contains(source_path_json));
}

#[test]
fn config_command_invalid_config_should_exit_with_config_error() {
    let repo = git_repo();
    let config = repo.path().join(".treeboot.toml");
    write_file(
        &config,
        "commands = [{ run = \"npm\", program = \"npm\" }]\n",
    );

    treeboot()
        .arg("config")
        .current_dir(repo.path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("invalid config"))
        .stderr(predicate::str::contains("mutually exclusive"));
}

#[test]
fn unknown_option_should_exit_with_usage_error() {
    treeboot()
        .arg("--unknown")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("unexpected argument"));
}

#[cfg(unix)]
#[test]
fn root_option_should_set_script_root_env() {
    let repo = git_repo();
    let root = TempDir::new().expect("root tempdir should be created");
    let output = repo.path().join("root.out");
    let script = repo.path().join(".treeboot.sh");
    write_executable_script(
        &script,
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$TREEBOOT_ROOT_PATH\" > {}\n",
            output.display()
        ),
    );

    treeboot()
        .arg("--root")
        .arg(root.path())
        .current_dir(repo.path())
        .assert()
        .success();

    let script_output = std::fs::read_to_string(output).expect("script output should exist");
    let root_path = std::fs::canonicalize(root.path()).expect("root should normalize");
    assert_eq!(script_output, format!("{}\n", root_path.display()));
}

#[cfg(unix)]
#[test]
fn conductor_default_branch_env_should_set_script_branch_env() {
    let repo = git_repo();
    let output = repo.path().join("branch.out");
    let script = repo.path().join(".treeboot.sh");
    write_executable_script(
        &script,
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$TREEBOOT_DEFAULT_BRANCH\" > {}\n",
            output.display()
        ),
    );

    treeboot()
        .env("CONDUCTOR_DEFAULT_BRANCH", "series-1.2")
        .current_dir(repo.path())
        .assert()
        .success();

    let script_output = std::fs::read_to_string(output).expect("script output should exist");
    assert_eq!(script_output, "series-1.2\n");
}

#[cfg(unix)]
#[test]
fn executable_init_script_should_win_over_missing_config() {
    let repo = git_repo();
    let script = repo.path().join(".treeboot.sh");
    let output = repo.path().join("script.out");
    write_executable_script(
        &script,
        &format!(
            "#!/bin/sh\nprintf '%s:%s\\n' \"$TREEBOOT_ROOT_PATH\" \
             \"$CODEX_WORKTREE_PATH\" > {}\n",
            output.display()
        ),
    );

    treeboot()
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: run"));

    let script_output = std::fs::read_to_string(output).expect("script output should exist");
    let repo_path = std::fs::canonicalize(repo.path()).expect("repo path should normalize");
    let expected = format!("{}:{}\n", repo_path.display(), repo_path.display());
    assert_eq!(script_output, expected);
}

#[cfg(unix)]
#[test]
fn dry_run_init_script_should_not_execute_script() {
    let repo = git_repo();
    let script = repo.path().join(".treeboot.sh");
    let output = repo.path().join("script.out");
    write_executable_script(
        &script,
        &format!("#!/bin/sh\nprintf 'ran\\n' > {}\n", output.display()),
    );

    treeboot()
        .arg("--dry-run")
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: would run"));

    assert!(!output.exists());
}

#[cfg(unix)]
#[test]
fn failing_init_script_should_exit_with_runtime_failure() {
    let repo = git_repo();
    let script = repo.path().join(".treeboot.sh");
    write_executable_script(&script, "#!/bin/sh\nexit 7\n");

    treeboot()
        .current_dir(repo.path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("init script"))
        .stderr(predicate::str::contains("failed"));
}

#[cfg(unix)]
#[test]
fn config_option_should_skip_executable_script_discovery() {
    let repo = git_repo();
    let config = repo.path().join("custom.treeboot.toml");
    let script = repo.path().join(".treeboot.sh");
    let marker = repo.path().join("script.out");
    write_file(&config, "commands = []\n");
    write_executable_script(
        &script,
        &format!("#!/bin/sh\nprintf 'ran\\n' > {}\n", marker.display()),
    );

    treeboot()
        .args(["--config", "custom.treeboot.toml"])
        .current_dir(repo.path())
        .assert()
        .code(1)
        .stdout(predicate::str::contains("treeboot: config detected"));

    assert!(!marker.exists());
}

#[cfg(unix)]
#[test]
fn non_executable_init_script_should_be_ignored() {
    let repo = git_repo();
    write_file(&repo.path().join(".treeboot.sh"), "#!/bin/sh\nexit 1\n");

    treeboot()
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: ignore"))
        .stdout(predicate::str::contains("treeboot: no config detected"));
}

#[test]
fn init_config_should_create_starter_config() {
    let dir = TempDir::new().expect("tempdir should be created");

    treeboot()
        .args(["init", "--config"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: created"));

    assert!(dir.path().join(".treeboot.toml").is_file());
}

#[test]
fn init_without_kind_should_exit_with_runtime_failure() {
    let dir = TempDir::new().expect("tempdir should be created");

    treeboot()
        .arg("init")
        .current_dir(dir.path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "treeboot init requires --config or --script",
        ));
}

#[test]
fn init_config_should_fail_when_target_exists_without_force() {
    let dir = TempDir::new().expect("tempdir should be created");
    write_file(&dir.path().join(".treeboot.toml"), "old\n");

    treeboot()
        .args(["init", "--config"])
        .current_dir(dir.path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("init target already exists"));
}

#[test]
fn init_config_force_should_replace_existing_target() {
    let dir = TempDir::new().expect("tempdir should be created");
    let config = dir.path().join(".treeboot.toml");
    write_file(&config, "old\n");

    treeboot()
        .args(["init", "--config", "--force"])
        .current_dir(dir.path())
        .assert()
        .success();

    let content = std::fs::read_to_string(config).expect("config should be readable");
    assert!(content.contains("copy = ["));
}

#[test]
fn init_path_should_create_parent_directories() {
    let dir = TempDir::new().expect("tempdir should be created");

    treeboot()
        .args(["init", "--config", "--path", "nested/.treeboot.toml"])
        .current_dir(dir.path())
        .assert()
        .success();

    assert!(dir.path().join("nested/.treeboot.toml").is_file());
}

#[cfg(unix)]
#[test]
fn init_script_should_create_executable_script() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().expect("tempdir should be created");

    treeboot()
        .args(["init", "--script"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: created"));

    let metadata = dir
        .path()
        .join(".treeboot.sh")
        .metadata()
        .expect("script should exist");
    assert!(metadata.permissions().mode() & 0o111 != 0);
}
