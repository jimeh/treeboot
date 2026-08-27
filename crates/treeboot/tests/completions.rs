use predicates::prelude::*;
#[cfg(unix)]
use std::path::PathBuf;
#[cfg(unix)]
use std::process::Command as StdCommand;
use tempfile::TempDir;

mod common;

#[cfg(unix)]
use common::{GitWorktree, git_worktree};
use common::{treeboot, write_file};

#[cfg(unix)]
const TREEBOOT_ENVIRONMENT: &[&str] = &[
    "TREEBOOT_ROOT_PATH",
    "CODEX_SOURCE_TREE_PATH",
    "CONDUCTOR_ROOT_PATH",
    "SUPERSET_ROOT_PATH",
];

#[test]
fn completions_supported_shells_should_emit_scripts() {
    for shell in ["bash", "zsh", "fish", "powershell", "elvish"] {
        treeboot()
            .args(["completions", shell])
            .assert()
            .success()
            .stderr(predicate::str::is_empty())
            .stdout(predicate::str::contains("treeboot"))
            .stdout(predicate::str::contains("COMPLETE"));
    }
}

#[test]
fn completions_should_include_current_subcommands_and_flags() {
    treeboot()
        .env("COMPLETE", "fish")
        .args(["--", "treeboot", ""])
        .assert()
        .success()
        .stdout(predicate::str::contains("copy"))
        .stdout(predicate::str::contains("symlink"))
        .stdout(predicate::str::contains("sync"))
        .stdout(predicate::str::contains("run"))
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("config"))
        .stdout(predicate::str::contains("init"))
        .stdout(predicate::str::contains("version"))
        .stdout(predicate::str::contains("check"))
        .stdout(predicate::str::contains("schema"))
        .stdout(predicate::str::contains("doctor"))
        .stdout(predicate::str::contains("env"))
        .stdout(predicate::str::contains("teardown"))
        .stdout(predicate::str::contains("worktree"))
        .stdout(predicate::str::contains("--root"))
        .stdout(predicate::str::contains("--config"))
        .stdout(predicate::str::contains("--no-init-script").not())
        .stdout(predicate::str::contains("--dry-run"))
        .stdout(predicate::str::contains("--verbose"));
}

#[test]
fn dynamic_completions_should_include_nested_worktree_commands_and_formats() {
    treeboot()
        .env("COMPLETE", "fish")
        .args(["--", "treeboot", "worktree", ""])
        .assert()
        .success()
        .stdout(predicate::str::contains("id"))
        .stdout(predicate::str::contains("slug"))
        .stdout(predicate::str::contains("path"))
        .stdout(predicate::str::contains("list"));

    for command in ["id", "slug", "path", "list"] {
        treeboot()
            .env("COMPLETE", "fish")
            .args(["--", "treeboot", "worktree", command, "--"])
            .assert()
            .success()
            .stdout(predicate::str::contains("--format"))
            .stdout(predicate::str::contains("--json"))
            .stdout(predicate::str::contains("--yaml"));
    }
}

#[test]
fn dynamic_identity_completions_should_suggest_directories() {
    let dir = TempDir::new().expect("tempdir should be created");
    std::fs::create_dir(dir.path().join("target-dir")).expect("target directory should be created");
    write_file(&dir.path().join("target-file"), "file\n");

    for command in ["id", "slug"] {
        treeboot()
            .env("COMPLETE", "fish")
            .args(["--", "treeboot", "worktree", command, "target"])
            .current_dir(dir.path())
            .assert()
            .success()
            .stdout(predicate::str::contains("target-dir"))
            .stdout(predicate::str::contains("target-file").not());
    }
}

#[test]
fn dynamic_completions_should_include_teardown_flags() {
    treeboot()
        .env("COMPLETE", "fish")
        .args(["--", "treeboot", "teardown", "--"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--worktree"))
        .stdout(predicate::str::contains("--root"))
        .stdout(predicate::str::contains("--config"))
        .stdout(predicate::str::contains("--dry-run"))
        .stdout(predicate::str::contains("--yes"));
}

#[test]
fn dynamic_completions_should_include_manual_command_flags() {
    treeboot()
        .env("COMPLETE", "fish")
        .args(["--", "treeboot", "sync", "--"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--target"))
        .stdout(predicate::str::contains("--required"))
        .stdout(predicate::str::contains("--compare"))
        .stdout(predicate::str::contains("--delete"))
        .stdout(predicate::str::contains("--no-delete"))
        .stdout(predicate::str::contains("--symlinks"))
        .stdout(predicate::str::contains("--verbose"));
}

#[test]
fn completions_should_omit_removed_init_script_flag() {
    treeboot()
        .env("COMPLETE", "fish")
        .args(["--", "treeboot", "init", "--"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--config"))
        .stdout(predicate::str::contains("--script").not());
}

#[test]
fn completions_unsupported_shell_should_exit_with_usage_error() {
    treeboot()
        .args(["completions", "nu"])
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("invalid value"))
        .stderr(predicate::str::contains("possible values"));
}

#[test]
fn completions_should_not_require_git_or_config_discovery() {
    let dir = TempDir::new().expect("tempdir should be created");
    write_file(&dir.path().join(".treeboot.toml"), "invalid toml = [\n");

    treeboot()
        .args(["completions", "fish"])
        .env("TREEBOOT_STRICT", "not-a-bool")
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("treeboot"));
}

#[cfg(unix)]
#[test]
fn installed_bash_completion_helper_should_list_root_sources() {
    if !shell_available("bash", &["--version"]) {
        eprintln!("skipping reference-only Bash completion helper test: Bash is unavailable");
        return;
    }

    let (repo, _temp, script_path) = completion_fixture("bash", "bash");
    let script = std::fs::read_to_string(&script_path).expect("completion script should be read");
    write_file(
        &script_path,
        &format!(
            "{script}\nCOMP_WORDS=(treeboot copy sh)\nCOMP_CWORD=2\nCOMP_TYPE=9\n_clap_complete_treeboot '' 'sh'\nprintf '%s\\n' \"${{COMPREPLY[@]}}\"\n"
        ),
    );

    let mut command = StdCommand::new("bash");
    scrub_treeboot_environment(&mut command);
    let completion = command
        .arg(&script_path)
        .current_dir(repo.worktree_path())
        .output()
        .expect("installed Bash completion script should run");

    assert_completion(&completion);
}

#[cfg(unix)]
#[test]
fn installed_zsh_completion_helper_should_list_root_sources() {
    if !shell_available(
        "zsh",
        &[
            "-f",
            "-c",
            "autoload -Uz compinit; compinit; whence compdef >/dev/null",
        ],
    ) {
        eprintln!("skipping reference-only Zsh completion helper test: Zsh is unavailable");
        return;
    }

    let (repo, _temp, script_path) = completion_fixture("zsh", "zsh");
    let mut command = StdCommand::new("zsh");
    scrub_treeboot_environment(&mut command);
    let completion = command
        .args([
            "-f",
            "-c",
            "autoload -Uz compinit; compinit; source \"$1\"; function _describe { print -rl -- \"${(@P)3}\"; }; words=(treeboot copy sh); CURRENT=3; _clap_dynamic_completer_treeboot; true",
            "completion-test",
        ])
        .arg(&script_path)
        .current_dir(repo.worktree_path())
        .output()
        .expect("installed Zsh completion script should run");

    assert_completion(&completion);
}

#[cfg(unix)]
fn shell_available(shell: &str, args: &[&str]) -> bool {
    let mut command = StdCommand::new(shell);
    scrub_treeboot_environment(&mut command);
    command
        .args(args)
        .output()
        .is_ok_and(|output| output.status.success())
}

#[cfg(unix)]
fn scrub_treeboot_environment(command: &mut StdCommand) {
    for name in TREEBOOT_ENVIRONMENT {
        command.env_remove(name);
    }
}

#[cfg(unix)]
fn completion_fixture(shell: &str, extension: &str) -> (GitWorktree, TempDir, PathBuf) {
    let repo = git_worktree();
    std::fs::create_dir_all(repo.root_path().join("shared-source"))
        .expect("root source directory should be created");
    let output = treeboot()
        .args(["completions", shell])
        .current_dir(repo.worktree_path())
        .output()
        .expect("candidate should generate completion script");
    assert!(
        output.status.success(),
        "candidate failed to generate {shell} completion script with status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let temp = TempDir::new().expect("completion script directory should be created");
    let script_path = temp.path().join(format!("completion-test.{extension}"));
    std::fs::write(&script_path, &output.stdout).expect("completion script should be written");
    (repo, temp, script_path)
}

#[cfg(unix)]
fn assert_completion(completion: &std::process::Output) {
    assert!(
        completion.status.success(),
        "completion shell failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
        completion.status.code(),
        String::from_utf8_lossy(&completion.stdout),
        String::from_utf8_lossy(&completion.stderr)
    );
    assert!(
        String::from_utf8_lossy(&completion.stdout).contains("shared-source"),
        "completion output did not contain shared-source\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&completion.stdout),
        String::from_utf8_lossy(&completion.stderr)
    );
}
