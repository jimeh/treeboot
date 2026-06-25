use predicates::prelude::*;
use tempfile::TempDir;

mod common;

use common::{assert_json_object_keys, git_worktree, parse_json, treeboot, write_file};

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
        .stdout(predicate::str::contains(
            "copy .env.local -> .env.local symlinks=preserve",
        ))
        .stdout(predicate::str::contains(concat!(
            "sync shared/config -> shared/config ",
            "compare=metadata delete=false symlinks=preserve"
        )))
        .stdout(predicate::str::contains("run \"mise install\""));
}

#[test]
fn config_command_json_should_print_normalized_config() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(&repo.root_path().join(".env"), "TOKEN=1\n");
    std::fs::create_dir_all(repo.root_path().join("shared")).expect("shared dir should be created");
    write_file(&repo.root_path().join("shared/config"), "value\n");
    write_file(
        &config,
        r#"
copy = [{ source = ".env", target = ".env", required = true }]
sync = [{ source = "shared", target = ".config/shared", compare = "checksum", delete = true }]
commands = [
  { name = "Install packages", run = "mise install", cwd = ".", env = { FOO = "bar" }, allow_failure = true },
  { program = "npm", args = ["install"] },
]
"#,
    );

    let json = treeboot()
        .args(["config", "--format", "json"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let json = parse_json(json, "config");
    assert_json_object_keys(&json, &["config", "path"]);
    assert!(json["path"].is_string());

    let config = &json["config"];
    assert_json_object_keys(
        config,
        &[
            "commands",
            "dangerously_allow_sources_outside_root",
            "dangerously_allow_targets_outside_worktree",
            "files",
            "strict",
        ],
    );
    assert_eq!(config["strict"], false);
    assert_eq!(config["dangerously_allow_sources_outside_root"], false);
    assert_eq!(config["dangerously_allow_targets_outside_worktree"], false);

    let files = config["files"]
        .as_array()
        .expect("files should be an array");
    assert_eq!(files.len(), 2);
    for file in files {
        assert_json_object_keys(
            file,
            &[
                "compare",
                "declaration",
                "delete",
                "operation",
                "required",
                "source",
                "source_path",
                "symlinks",
                "target",
                "target_path",
            ],
        );
        assert_json_object_keys(&file["declaration"], &["column", "end", "line", "start"]);
    }
    assert_eq!(files[0]["operation"], "copy");
    assert_eq!(files[0]["source"], ".env");
    assert_eq!(files[0]["target"], ".env");
    assert_eq!(files[0]["required"], true);
    assert_eq!(files[0]["compare"], serde_json::Value::Null);
    assert_eq!(files[0]["delete"], serde_json::Value::Null);
    assert_eq!(files[0]["symlinks"], "preserve");
    assert!(files[0]["source_path"].is_string());
    assert!(files[0]["target_path"].is_string());

    assert_eq!(files[1]["operation"], "sync");
    assert_eq!(files[1]["source"], "shared");
    assert_eq!(files[1]["target"], ".config/shared");
    assert_eq!(files[1]["required"], false);
    assert_eq!(files[1]["compare"], "checksum");
    assert_eq!(files[1]["delete"], true);
    assert_eq!(files[1]["symlinks"], "preserve");

    let commands = config["commands"]
        .as_array()
        .expect("commands should be an array");
    assert_eq!(commands.len(), 2);
    for command in commands {
        assert_json_object_keys(
            command,
            &[
                "allow_failure",
                "command",
                "cwd",
                "cwd_path",
                "declaration",
                "env",
                "name",
            ],
        );
        assert_json_object_keys(&command["declaration"], &["column", "end", "line", "start"]);
    }
    assert_eq!(commands[0]["name"], "Install packages");
    assert_json_object_keys(&commands[0]["command"], &["kind", "run"]);
    assert_eq!(commands[0]["command"]["kind"], "shell");
    assert_eq!(commands[0]["command"]["run"], "mise install");
    assert_eq!(commands[0]["cwd"], ".");
    assert!(commands[0]["cwd_path"].is_string());
    assert_eq!(commands[0]["env"]["FOO"], "bar");
    assert_eq!(commands[0]["allow_failure"], true);

    assert_eq!(commands[1]["name"], serde_json::Value::Null);
    assert_json_object_keys(&commands[1]["command"], &["args", "kind", "program"]);
    assert_eq!(commands[1]["command"]["kind"], "direct");
    assert_eq!(commands[1]["command"]["program"], "npm");
    assert_eq!(commands[1]["command"]["args"][0], "install");
    assert_eq!(commands[1]["cwd"], serde_json::Value::Null);
    assert_eq!(commands[1]["cwd_path"], serde_json::Value::Null);
    assert_eq!(
        commands[1]["env"],
        serde_json::Value::Object(serde_json::Map::new())
    );
    assert_eq!(commands[1]["allow_failure"], false);
}

#[test]
fn config_command_json_shortcut_should_print_normalized_config() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(&config, "commands = [\"mise install\"]\n");

    treeboot()
        .args(["config", "--json"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"commands\""))
        .stdout(predicate::str::contains("\"run\": \"mise install\""));
}

#[test]
fn config_command_yaml_should_print_normalized_config() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(&config, "commands = [\"mise install\"]\n");

    treeboot()
        .args(["config", "--format", "yaml"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("commands:"))
        .stdout(predicate::str::contains("run: mise install"));
}

#[test]
fn config_command_yaml_shortcut_should_print_normalized_config() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(&config, "commands = [\"mise install\"]\n");

    treeboot()
        .args(["config", "--yaml"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("commands:"))
        .stdout(predicate::str::contains("run: mise install"));
}

#[test]
fn config_command_text_format_should_print_normalized_config() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(&config, "commands = [\"mise install\"]\n");

    treeboot()
        .args(["config", "--format", "text"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: config"))
        .stdout(predicate::str::contains("run \"mise install\""));
}

#[test]
fn config_command_output_shortcuts_should_conflict_with_each_other() {
    let repo = git_worktree();

    treeboot()
        .args(["config", "--json", "--yaml"])
        .current_dir(repo.worktree_path())
        .assert()
        .code(2)
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn config_command_json_should_conflict_with_format() {
    let repo = git_worktree();

    treeboot()
        .args(["config", "--json", "--format", "json"])
        .current_dir(repo.worktree_path())
        .assert()
        .code(2)
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn config_command_yaml_should_conflict_with_format() {
    let repo = git_worktree();

    treeboot()
        .args(["config", "--yaml", "--format", "yaml"])
        .current_dir(repo.worktree_path())
        .assert()
        .code(2)
        .stderr(predicate::str::contains("cannot be used with"));
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
        .args(["config", "-c", "custom.treeboot.toml"])
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
  allow_failure = true,
  env = { NODE_ENV = "development" },
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
        .stdout(predicate::str::contains(concat!(
            "exec npm install allow_failure=true cwd=app ",
            "env={NODE_ENV=\"development\"}"
        )));
}

#[test]
fn config_command_should_reject_async_command_field() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(
        &config,
        r#"
commands = [{ run = "npm install", async = true }]
"#,
    );

    treeboot()
        .arg("config")
        .current_dir(repo.worktree_path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown field"));
}

#[test]
fn config_command_should_reject_args_with_shell_run() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(
        &config,
        r#"commands = [{ run = "npm install", args = ["--silent"] }]"#,
    );

    treeboot()
        .arg("config")
        .current_dir(repo.worktree_path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid config"))
        .stderr(predicate::str::contains("`args` requires `program`"));
}

#[test]
fn config_command_should_reject_missing_operation_in_mixed_file_entry() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    write_file(&config, r#"files = [{ source = ".env" }]"#);

    treeboot()
        .arg("config")
        .current_dir(repo.worktree_path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid config"))
        .stderr(predicate::str::contains("missing required `operation`"));
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
        .args(["config", "-r"])
        .arg(root.path())
        .arg("-J")
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

#[test]
fn config_command_json_should_warn_when_run_validation_would_fail() {
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
        .args(["config", "--format", "json"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"config\""))
        .stdout(predicate::str::contains("\"files\""))
        .stderr(predicate::str::contains("treeboot: warning"))
        .stderr(predicate::str::contains("duplicate configured target"));
}

#[test]
fn config_command_should_warn_when_config_strict_would_fail() {
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
        .arg("config")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: config"))
        .stderr(predicate::str::contains("treeboot: warning"))
        .stderr(predicate::str::contains("cannot be used with sync"));
}

#[test]
fn config_command_should_warn_when_env_strict_would_fail() {
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
        .arg("config")
        .env("TREEBOOT_STRICT", "true")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: config"))
        .stderr(predicate::str::contains("treeboot: warning"))
        .stderr(predicate::str::contains("cannot be used with sync"));
}
