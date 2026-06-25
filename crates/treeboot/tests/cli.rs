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
fn version_flags_should_print_package_and_spec_version() {
    treeboot()
        .arg("--version")
        .assert()
        .success()
        .stdout(format!(
            "treeboot {} (spec {})\n",
            treeboot_core::TREEBOOT_VERSION,
            treeboot_core::SPEC_VERSION
        ));

    treeboot().arg("-V").assert().success().stdout(format!(
        "treeboot {} (spec {})\n",
        treeboot_core::TREEBOOT_VERSION,
        treeboot_core::SPEC_VERSION
    ));

    treeboot()
        .args(["run", "--version"])
        .assert()
        .success()
        .stdout(format!(
            "treeboot {} (spec {})\n",
            treeboot_core::TREEBOOT_VERSION,
            treeboot_core::SPEC_VERSION
        ));
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

#[test]
fn text_only_commands_should_reject_structured_output_options() {
    for args in [
        &["run", "--json"][..],
        &["init", "--json"][..],
        &["copy", "source", "--json"][..],
        &["symlink", "source", "--json"][..],
        &["sync", "source", "--json"][..],
        &["completions", "bash", "--json"][..],
        &["run", "--format", "json"][..],
        &["init", "--format", "json"][..],
        &["copy", "source", "--format", "json"][..],
        &["symlink", "source", "--format", "json"][..],
        &["sync", "source", "--format", "json"][..],
        &["completions", "bash", "--format", "json"][..],
    ] {
        treeboot()
            .args(args)
            .assert()
            .code(2)
            .stderr(predicate::str::contains("unexpected argument"));
    }
}
