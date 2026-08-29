use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn treeboot() -> Command {
    let mut command = Command::cargo_bin("treeboot").expect("treeboot binary should build");
    command
        .env_remove("TREEBOOT_ROOT_PATH")
        .env_remove("CODEX_SOURCE_TREE_PATH")
        .env_remove("CONDUCTOR_ROOT_PATH")
        .env_remove("SUPERSET_ROOT_PATH")
        .env_remove("CONDUCTOR_DEFAULT_BRANCH")
        .env_remove("TREEBOOT_STRICT")
        .env_remove("TREEBOOT_DANGEROUSLY_ALLOW_SOURCES_OUTSIDE_ROOT")
        .env_remove("TREEBOOT_DANGEROUSLY_ALLOW_TARGETS_OUTSIDE_WORKTREE");
    command
}

#[test]
fn reference_help_and_versions_should_match_embedded_assets() {
    treeboot()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage: treeboot"));

    for flag in ["--version", "-V"] {
        treeboot().arg(flag).assert().success().stdout(format!(
            "treeboot {} (spec {})\n",
            treeboot_core::TREEBOOT_VERSION,
            treeboot_core::SPEC_VERSION
        ));
    }

    for command in [
        &["run"][..],
        &["plan"][..],
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
                .stdout(predicate::str::contains(treeboot_core::TREEBOOT_VERSION))
                .stdout(predicate::str::contains(format!(
                    "(spec {})",
                    treeboot_core::SPEC_VERSION
                )));
        }
    }

    let json = treeboot()
        .args(["version", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&json).expect("version JSON should parse");
    assert_eq!(json["version"], treeboot_core::TREEBOOT_VERSION);
}

#[test]
fn reference_clap_diagnostics_should_preserve_parser_wording() {
    for args in [&["--unknown"][..], &["--no-commands"][..]] {
        treeboot()
            .args(args)
            .assert()
            .code(2)
            .stderr(predicate::str::contains("unexpected argument"));
    }

    treeboot()
        .args(["run", "--json"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "structured run output requires --dry-run or --skip-commands",
        ));

    treeboot()
        .args(["version", "--json", "--yaml"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("cannot be used with"));

    treeboot()
        .args(["completions", "nu"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("invalid value"))
        .stderr(predicate::str::contains("possible values"));

    treeboot()
        .arg("worktree")
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("Usage:"));
}

#[test]
fn reference_generated_completion_scripts_should_include_complete_marker() {
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
fn reference_output_failures_should_use_treeboot_diagnostic() {
    let temp = TempDir::new().expect("tempdir should be created");
    let output = temp.path().join("missing").join("schema.json");

    treeboot()
        .args(["schema", "--output"])
        .arg(output)
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("failed to write output"));
}
