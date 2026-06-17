use predicates::prelude::*;
use tempfile::TempDir;

mod common;

use common::{git_worktree, treeboot, write_file};

#[test]
fn config_command_should_print_normalized_config() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
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
        .current_dir(repo.worktree_path())
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
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(&config, "commands = [\"mise install\"]\n");

    treeboot()
        .args(["config", "--format", "json"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"commands\""))
        .stdout(predicate::str::contains("\"run\": \"mise install\""));
}

#[test]
fn config_command_missing_config_should_exit_with_runtime_failure() {
    let repo = git_worktree();

    treeboot()
        .arg("config")
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("treeboot: no config detected"));
}

#[test]
fn config_command_config_option_should_use_requested_file() {
    let repo = git_worktree();
    let default_config = repo.worktree_path().join(".treeboot.toml");
    let requested_config = repo.worktree_path().join("custom.treeboot.toml");
    write_file(&default_config, "commands = [\"default\"]\n");
    write_file(
        &requested_config,
        "commands = [{ program = \"npm\", args = [\"install\"] }]\n",
    );

    treeboot()
        .args(["config", "--config", "custom.treeboot.toml"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("custom.treeboot.toml"))
        .stdout(predicate::str::contains("exec npm install"))
        .stdout(predicate::str::contains("default").not());
}

#[test]
fn config_command_should_print_file_and_command_options() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    let shared_dir = repo.root_path().join("shared");
    std::fs::create_dir_all(&shared_dir).expect("shared dir should be created");
    std::fs::write(repo.root_path().join(".env.required"), "TOKEN=1\n")
        .expect("required source should be written");
    std::fs::write(shared_dir.join("tool"), "tool\n").expect("symlink source should be written");
    std::fs::create_dir_all(repo.worktree_path().join("app"))
        .expect("command cwd should be created");
    write_file(
        &config,
        r#"
copy = [{ source = ".env.required", required = true }]
symlink = ["shared/tool"]
commands = [{
  program = "npm",
  args = ["install"],
  cwd = "app",
  async = true,
  allow_failure = true,
}]
"#,
    );

    treeboot()
        .arg("config")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "copy .env.required -> .env.required required=true",
        ))
        .stdout(predicate::str::contains(
            "symlink shared/tool -> shared/tool",
        ))
        .stdout(predicate::str::contains(
            "exec npm install async=true allow_failure=true cwd=app",
        ));
}

#[test]
fn config_command_root_option_should_resolve_json_source_paths() {
    let repo = git_worktree();
    let root = TempDir::new().expect("root tempdir should be created");
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(&config, "copy = [\"shared/.env\"]\n");

    let root_path = std::fs::canonicalize(root.path()).expect("root should normalize");
    let source_path = root_path.join("shared/.env").display().to_string();
    let source_path_json = source_path.replace('\\', "\\\\");

    treeboot()
        .args(["config", "--root"])
        .arg(root.path())
        .arg("--format")
        .arg("json")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"source_path\""))
        .stdout(predicate::str::contains(source_path_json));
}

#[test]
fn config_command_invalid_config_should_exit_with_config_error() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(
        &config,
        "commands = [{ run = \"npm\", program = \"npm\" }]\n",
    );

    treeboot()
        .arg("config")
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("invalid config"))
        .stderr(predicate::str::contains("mutually exclusive"));
}

#[test]
fn config_command_should_warn_when_run_validation_would_fail() {
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
        .arg("config")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: config"))
        .stderr(predicate::str::contains("treeboot: warning"))
        .stderr(predicate::str::contains("duplicate configured target"));
}
