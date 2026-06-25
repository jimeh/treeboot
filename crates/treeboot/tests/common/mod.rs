#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

pub fn display_path(path: &str) -> String {
    path.split('/').collect::<PathBuf>().display().to_string()
}

pub fn treeboot() -> Command {
    let mut command = Command::cargo_bin("treeboot").expect("treeboot binary should build");
    command
        .env_remove("TREEBOOT_STRICT")
        .env_remove("TREEBOOT_DANGEROUSLY_ALLOW_SOURCES_OUTSIDE_ROOT")
        .env_remove("TREEBOOT_DANGEROUSLY_ALLOW_TARGETS_OUTSIDE_WORKTREE");
    command
}

pub fn git(args: &[&str], cwd: &Path) {
    let output = StdCommand::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("git should run");

    assert!(
        output.status.success(),
        "git {args:?} should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

pub fn git_repo() -> TempDir {
    let repo = TempDir::new().expect("tempdir should be created");
    git(&["init"], repo.path());
    repo
}

pub struct GitWorktree {
    root: TempDir,
    _worktree_parent: TempDir,
    worktree_path: PathBuf,
}

impl GitWorktree {
    pub fn root_path(&self) -> &Path {
        self.root.path()
    }

    pub fn worktree_path(&self) -> &Path {
        &self.worktree_path
    }
}

pub fn git_worktree() -> GitWorktree {
    let root = git_repo();
    git(&["config", "user.name", "treeboot"], root.path());
    git(
        &["config", "user.email", "treeboot@example.invalid"],
        root.path(),
    );
    git(&["config", "commit.gpgsign", "false"], root.path());
    write_file(&root.path().join("README.md"), "treeboot test repo\n");
    git(&["add", "README.md"], root.path());
    git(&["commit", "-m", "Initial commit"], root.path());

    let worktree_parent = TempDir::new().expect("worktree parent should be created");
    let worktree_path = worktree_parent.path().join("linked");
    let worktree = worktree_path
        .to_str()
        .expect("worktree path should be valid UTF-8");
    git(
        &["worktree", "add", "-b", "treeboot-test-worktree", worktree],
        root.path(),
    );

    GitWorktree {
        root,
        _worktree_parent: worktree_parent,
        worktree_path,
    }
}

pub fn write_file(path: &Path, content: &str) {
    std::fs::write(path, content).expect("file should be written");
}

pub fn parse_json(stdout: Vec<u8>, context: &str) -> Value {
    serde_json::from_slice(&stdout).unwrap_or_else(|error| {
        panic!("{context} JSON should parse: {error}");
    })
}

pub fn assert_json_object_keys(value: &Value, expected: &[&str]) {
    let object = value.as_object().expect("value should be a JSON object");
    let mut actual = object.keys().map(String::as_str).collect::<Vec<_>>();
    actual.sort_unstable();

    let mut expected = expected.to_vec();
    expected.sort_unstable();

    assert_eq!(actual, expected);
}

pub fn assert_context_shape(value: &Value) {
    assert_json_object_keys(value, &["default_branch", "root_path", "worktree_path"]);
    assert!(value["root_path"].is_string());
    assert!(value["worktree_path"].is_string());
    assert!(value["default_branch"].is_string());
}

#[cfg(unix)]
pub fn write_executable_script(path: &Path, content: &str) {
    use std::os::unix::fs::PermissionsExt;

    write_file(path, content);
    let mut permissions = path
        .metadata()
        .expect("script metadata should be readable")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions).expect("script permissions should be set");
}
