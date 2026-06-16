use std::path::Path;
use std::process::Command as StdCommand;

use assert_cmd::Command;
use tempfile::TempDir;

pub fn treeboot() -> Command {
    Command::cargo_bin("treeboot").expect("treeboot binary should build")
}

pub fn git(args: &[&str], cwd: &Path) {
    let output = StdCommand::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("git should run");

    assert!(output.status.success(), "git {args:?} should succeed");
}

pub fn git_repo() -> TempDir {
    let repo = TempDir::new().expect("tempdir should be created");
    git(&["init"], repo.path());
    repo
}

pub fn write_file(path: &Path, content: &str) {
    std::fs::write(path, content).expect("file should be written");
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
