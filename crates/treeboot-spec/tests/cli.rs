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
    assert_eq!(stdout.lines().count(), 322);
    assert!(stdout.lines().all(|line| !line.trim().is_empty()));
}

#[cfg(unix)]
#[test]
fn functional_exact_only_filter_is_a_selection_error() {
    let output = spec_command()
        .args([
            "test",
            "--profile",
            "functional",
            "--filter",
            "closure.exact.",
            "--",
            "sh",
            "-c",
            "exit 0",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("no conformance cases match"));
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
fn no_matching_filter_is_a_selection_error() {
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

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("no conformance cases match"));
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

#[cfg(unix)]
#[test]
fn concise_human_report_defers_failure_details_and_omits_passes() {
    let output = spec_command()
        .args([
            "test",
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
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("candidate: "));
    assert!(stdout.contains("sh -c \"exit 0\""), "{stdout}");
    assert!(stdout.contains("profile: full"), "{stdout}");
    assert!(stdout.contains("1 case:"), "{stdout}");
    assert!(stdout.contains("Failures:"), "{stdout}");
    assert!(stdout.contains("1. cli.help.print-usage"), "{stdout}");
    assert!(!stdout.contains("PASS "), "{stdout}");
    assert!(
        stdout.contains("#cli-surface-fifteen-subcommands-one-default-path"),
        "{stdout}"
    );
    assert!(stdout.contains("duration: "), "{stdout}");
    assert!(stdout.contains(":"), "{stdout}");
    assert!(
        stdout.find("Result:").unwrap() < stdout.find("Failures:").unwrap()
            && stdout.find("Failures:").unwrap() < stdout.find("Failure details:").unwrap(),
        "{stdout}"
    );
}

#[cfg(unix)]
#[test]
fn verbose_human_report_restores_pass_lines() {
    let output = spec_command()
        .args([
            "test",
            "--verbose",
            "--no-progress",
            "--filter",
            "cli.help.print-usage",
            "--",
            "sh",
            "-c",
            "printf 'treeboot usage\\n'",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("PASS cli.help.print-usage"), "{stdout}");
    assert!(!stdout.contains("candidate version:"), "{stdout}");
    assert!(output.stderr.is_empty());
}

#[cfg(unix)]
#[test]
fn timeout_uses_human_label_and_exit_three() {
    let output = spec_command()
        .args([
            "test",
            "--timeout-ms",
            "10",
            "--filter",
            "cli.help.print-usage",
            "--",
            "sh",
            "-c",
            "sleep 1",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(3));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Result: ERROR"), "{stdout}");
    assert!(stdout.contains("TIMEOUT cli.help.print-usage"), "{stdout}");
    assert!(
        stdout.contains("1. cli.help.print-usage [TIMEOUT]"),
        "{stdout}"
    );
}

#[cfg(unix)]
#[test]
fn fixture_setup_failure_is_error_with_exit_three() {
    let empty_path = tempfile::TempDir::new().unwrap();
    let output = spec_command()
        .args([
            "test",
            "--format",
            "json",
            "--filter",
            "check.should-validate-config-without-side-effects",
            "--",
            env!("CARGO_BIN_EXE_treeboot-spec"),
        ])
        .env("PATH", empty_path.path())
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(3),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
            panic!(
                "fixture error report should be JSON: {error}\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )
        });
    assert_eq!(
        report["cases"][0]["outcome"]["kind"], "error",
        "report: {report:#}"
    );
    let details = report["cases"][0]["outcome"]["details"]
        .as_str()
        .unwrap_or_else(|| panic!("fixture error details should be text; report: {report:#}"));
    assert!(details.contains("fixture setup"), "report: {report:#}");
}
