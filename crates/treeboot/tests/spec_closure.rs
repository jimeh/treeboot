#![cfg(unix)]

use predicates::prelude::*;
use std::path::Path;

mod common;

use common::{git_worktree, treeboot};

fn symlink(source: impl AsRef<Path>, target: impl AsRef<Path>) {
    std::os::unix::fs::symlink(source.as_ref(), target.as_ref())
        .expect("symlink should be created");
}

#[test]
fn recursive_diagnostic_uses_normalized_symlinked_directory_source() {
    let repo = git_worktree();
    std::fs::create_dir_all(repo.root_path().join("actual/nested"))
        .expect("source directory should be created");
    symlink("missing", repo.root_path().join("actual/nested/dangling"));
    symlink("actual", repo.root_path().join("alias"));

    treeboot()
        .args(["copy", "alias"])
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("invalid copy file operation"))
        .stderr(predicate::str::contains("actual/nested/dangling"))
        .stderr(predicate::str::contains("alias/nested/dangling").not());
}
