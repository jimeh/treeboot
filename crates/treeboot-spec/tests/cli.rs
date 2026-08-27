use std::process::Command;

use treeboot_spec::{CONFIG_SCHEMA_JSON, SPEC_MARKDOWN};

fn spec_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_treeboot-spec"))
}

#[test]
fn list_prints_the_stable_registry() {
    let output = spec_command().arg("list").output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.lines().count() > 302);
    assert!(stdout.lines().all(|line| !line.trim().is_empty()));
}

#[test]
fn show_and_schema_print_exact_canonical_assets() {
    let specification = spec_command().arg("show").output().unwrap();
    assert!(specification.status.success());
    assert_eq!(specification.stdout, SPEC_MARKDOWN.as_bytes());

    let schema = spec_command().arg("schema").output().unwrap();
    assert!(schema.status.success());
    assert_eq!(schema.stdout, CONFIG_SCHEMA_JSON.as_bytes());
}

#[cfg(unix)]
#[test]
fn test_accepts_a_candidate_with_prefix_arguments_and_emits_json() {
    let output = spec_command()
        .args([
            "test",
            "--format",
            "json",
            "--filter",
            "no-such-case",
            "--",
            "sh",
            "-c",
            "exec \"$@\"",
            "candidate-wrapper",
            "sh",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["candidate"]["prefix_args"][0], "-c");
    assert_eq!(report["cases"].as_array().unwrap().len(), 0);
}

#[cfg(unix)]
#[test]
fn json_failure_report_is_valid_and_does_not_leak_caught_panics() {
    let output = spec_command()
        .args([
            "test",
            "--format",
            "json",
            "--filter",
            "cli.help.print-usage",
            "--",
            "sh",
            "-c",
            "exit 0",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["cases"][0]["outcome"]["kind"], "failed");
}
