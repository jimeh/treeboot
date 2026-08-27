use predicates::prelude::*;

use crate::cases::support::treeboot;

pub(crate) fn help_should_print_usage() {
    treeboot()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot"));
}

pub(crate) fn version_flags_should_print_package_and_spec_version() {
    let version = crate::cases::support::candidate_package_version();
    treeboot()
        .arg("--version")
        .assert()
        .success()
        .stdout(format!(
            "treeboot {} (spec {})\n",
            version,
            crate::SPEC_VERSION
        ));

    treeboot().arg("-V").assert().success().stdout(format!(
        "treeboot {} (spec {})\n",
        version,
        crate::SPEC_VERSION
    ));

    for command in [
        &["run"][..],
        &["teardown"][..],
        &["status"][..],
        &["config"][..],
        &["check"][..],
        &["init"][..],
        &["schema"][..],
        &["doctor"][..],
        &["env"][..],
        &["completions"][..],
        &["copy"][..],
        &["symlink"][..],
        &["sync"][..],
        &["version"][..],
        &["worktree"][..],
        &["worktree", "id"][..],
        &["worktree", "slug"][..],
        &["worktree", "path"][..],
        &["worktree", "list"][..],
    ] {
        for flag in ["--version", "-V"] {
            treeboot()
                .args(command)
                .arg(flag)
                .assert()
                .success()
                .stdout(predicate::str::contains(&version))
                .stdout(predicate::str::contains(format!(
                    "(spec {})",
                    crate::SPEC_VERSION
                )));
        }
    }
}

pub(crate) fn unknown_option_should_exit_with_usage_error() {
    treeboot()
        .arg("--unknown")
        .assert()
        .code(2)
        .stderr(predicate::str::is_empty().not());
}

pub(crate) fn legacy_no_commands_option_should_exit_with_usage_error() {
    treeboot()
        .arg("--no-commands")
        .assert()
        .code(2)
        .stderr(predicate::str::is_empty().not());
}

pub(crate) fn text_only_commands_should_reject_structured_output_options() {
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
            .stderr(predicate::str::is_empty().not());
    }
}
