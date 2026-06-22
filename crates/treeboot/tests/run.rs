use predicates::prelude::*;
use tempfile::TempDir;

mod common;

use common::{display_path, git_repo, git_worktree, treeboot, write_file};

#[cfg(unix)]
use common::write_executable_script;

fn toml_string_path(path: &std::path::Path) -> String {
    toml_string(&path.display().to_string())
}

fn toml_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
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
    let worktree = std::fs::canonicalize(repo.worktree_path())
        .expect("worktree should canonicalize")
        .display()
        .to_string();
    assert_eq!(pwd.trim(), worktree);
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
        .stdout(predicate::str::contains(format!(
            "treeboot: sync {} -> {}",
            display_path("shared/config"),
            display_path("shared/config")
        )));

    let copied = std::fs::read_to_string(repo.worktree_path().join(".env"))
        .expect("copied file should be readable");
    let synced = std::fs::read_to_string(repo.worktree_path().join("shared/config"))
        .expect("synced file should be readable");
    assert_eq!(copied, "TOKEN=1\n");
    assert_eq!(synced, "value\n");
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
        .stdout(predicate::str::contains(format!(
            "treeboot: delete {}",
            display_path("shared/old")
        )));

    assert!(!repo.worktree_path().join("shared/old").exists());
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
        .stdout(predicate::str::contains(format!(
            "treeboot: would delete {}",
            display_path("shared/old")
        )));

    let extra = std::fs::read_to_string(repo.worktree_path().join("shared/old"))
        .expect("target-only file should remain readable");
    assert_eq!(extra, "keep\n");
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
    let root = std::fs::canonicalize(repo.root_path())
        .expect("root should canonicalize")
        .display()
        .to_string();
    let worktree = std::fs::canonicalize(repo.worktree_path())
        .expect("worktree should canonicalize")
        .display()
        .to_string();
    let app = std::fs::canonicalize(app)
        .expect("app should canonicalize")
        .display()
        .to_string();
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
        .stdout(predicate::str::contains(format!(
            "treeboot: sync {} -> {}",
            display_path("shared/config"),
            display_path("shared/config")
        )));

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

#[cfg(unix)]
#[test]
fn run_copied_symlink_should_warn_when_final_target_is_missing() {
    let repo = git_worktree();
    let config = repo.worktree_path().join(".treeboot.toml");
    std::fs::create_dir_all(repo.root_path().join("shared")).expect("source dir should be created");
    write_file(&repo.root_path().join("shared/config"), "value\n");
    std::os::unix::fs::symlink("config", repo.root_path().join("shared/link"))
        .expect("source symlink should be created");
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

#[cfg(unix)]
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
    let resolved = std::fs::canonicalize(target.parent().unwrap().join(&link))
        .expect("relative symlink should resolve");
    let expected = std::fs::canonicalize(source).expect("source should resolve");
    assert!(!link.is_absolute());
    assert_eq!(resolved, expected);
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
