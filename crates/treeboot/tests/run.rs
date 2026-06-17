use predicates::prelude::*;
use tempfile::TempDir;

mod common;

use common::{git_repo, git_worktree, treeboot, write_file};

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
fn strict_missing_config_should_exit_with_runtime_failure() {
    let repo = git_worktree();

    treeboot()
        .arg("--strict")
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
        .arg("--root")
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
        .arg("--dry-run")
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
        .args(["--config", "custom.treeboot.toml"])
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
