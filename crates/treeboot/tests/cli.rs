use predicates::prelude::*;

mod common;

use common::treeboot;

#[test]
fn help_should_print_usage() {
    treeboot()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage: treeboot"));
}

#[test]
fn version_flag_should_print_package_version() {
    treeboot()
        .arg("--version")
        .assert()
        .success()
        .stdout(format!("treeboot {}\n", env!("CARGO_PKG_VERSION")));
}

#[test]
fn unknown_option_should_exit_with_usage_error() {
    treeboot()
        .arg("--unknown")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("unexpected argument"));
}

#[test]
fn legacy_no_commands_option_should_exit_with_usage_error() {
    treeboot()
        .arg("--no-commands")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("unexpected argument"));
}
