use std::process::Command;

fn treeboot() -> Command {
    Command::new(env!("CARGO_BIN_EXE_treeboot"))
}

#[test]
fn help_should_print_usage() {
    let output = treeboot()
        .arg("--help")
        .output()
        .expect("failed to run treeboot --help");

    assert!(output.status.success());

    let stdout =
        String::from_utf8(output.stdout).expect("stdout must be utf-8");
    assert!(stdout.contains("Usage: treeboot [OPTIONS]"));
}

#[test]
fn short_help_should_print_usage() {
    let output = treeboot()
        .arg("-h")
        .output()
        .expect("failed to run treeboot -h");

    assert!(output.status.success());

    let stdout =
        String::from_utf8(output.stdout).expect("stdout must be utf-8");
    assert!(stdout.contains("Usage: treeboot [OPTIONS]"));
}

#[test]
fn version_should_print_package_version() {
    let output = treeboot()
        .arg("--version")
        .output()
        .expect("failed to run treeboot --version");

    assert!(output.status.success());

    let stdout =
        String::from_utf8(output.stdout).expect("stdout must be utf-8");
    assert_eq!(stdout, format!("treeboot {}\n", env!("CARGO_PKG_VERSION")));
}

#[test]
fn short_version_should_print_package_version() {
    let output = treeboot()
        .arg("-V")
        .output()
        .expect("failed to run treeboot -V");

    assert!(output.status.success());

    let stdout =
        String::from_utf8(output.stdout).expect("stdout must be utf-8");
    assert_eq!(stdout, format!("treeboot {}\n", env!("CARGO_PKG_VERSION")));
}

#[test]
fn no_args_should_exit_without_output() {
    let output = treeboot().output().expect("failed to run treeboot");

    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn unknown_option_should_exit_with_usage_error() {
    let output = treeboot()
        .arg("--dry-run")
        .output()
        .expect("failed to run treeboot --dry-run");

    assert_eq!(output.status.code(), Some(2));

    let stderr =
        String::from_utf8(output.stderr).expect("stderr must be utf-8");
    assert!(stderr.contains("treeboot: unknown option: --dry-run"));
}
