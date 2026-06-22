use predicates::prelude::*;
use tempfile::TempDir;

mod common;

use common::{treeboot, write_file};

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
fn init_without_kind_should_create_starter_config() {
    let dir = TempDir::new().expect("tempdir should be created");

    treeboot()
        .arg("init")
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: created"));

    assert!(dir.path().join(".treeboot.toml").is_file());
}

#[test]
fn init_config_should_fail_when_target_exists() {
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
fn init_force_should_be_usage_error() {
    let dir = TempDir::new().expect("tempdir should be created");

    treeboot()
        .args(["init", "--config", "-f"])
        .current_dir(dir.path())
        .assert()
        .code(2)
        .stderr(predicate::str::contains("unexpected argument"));
}

#[cfg(unix)]
#[test]
fn init_config_should_fail_when_target_is_symlink_without_writing_through_it() {
    use std::os::unix::fs::symlink;

    let dir = TempDir::new().expect("tempdir should be created");
    let target = dir.path().join("target.toml");
    let link = dir.path().join(".treeboot.toml");
    write_file(&target, "old\n");
    symlink(&target, &link).expect("symlink should be created");

    treeboot()
        .args(["init", "--config"])
        .current_dir(dir.path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("init target already exists"));

    assert_eq!(
        std::fs::read_to_string(target).expect("target should be readable"),
        "old\n"
    );
    assert!(
        std::fs::symlink_metadata(link)
            .expect("link metadata should load")
            .file_type()
            .is_symlink()
    );
}

#[test]
fn init_path_should_create_parent_directories() {
    let dir = TempDir::new().expect("tempdir should be created");

    treeboot()
        .args(["init", "-p", "nested/.treeboot.toml"])
        .current_dir(dir.path())
        .assert()
        .success();

    assert!(dir.path().join("nested/.treeboot.toml").is_file());
}

#[test]
fn init_config_and_script_should_be_usage_error() {
    let dir = TempDir::new().expect("tempdir should be created");

    treeboot()
        .args(["init", "--config", "--script"])
        .current_dir(dir.path())
        .assert()
        .code(2)
        .stderr(predicate::str::contains("cannot be used with"));
}

#[cfg(unix)]
#[test]
fn init_script_should_create_executable_script() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().expect("tempdir should be created");

    treeboot()
        .args(["init", "-s"])
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
