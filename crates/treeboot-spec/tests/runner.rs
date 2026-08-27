use std::ffi::OsString;
use std::time::Duration;

use treeboot_spec::{
    CommandTemplate, Invocation, LocalProcessRunner, Runner, StdinMode, Termination,
};

fn shell_command(script: &str) -> CommandTemplate {
    #[cfg(unix)]
    {
        CommandTemplate::with_args("sh", ["-c", script])
    }
    #[cfg(windows)]
    {
        CommandTemplate::with_args("cmd", ["/C", script])
    }
}

#[test]
#[cfg(unix)]
fn local_runner_should_preserve_prefix_args_environment_cwd_and_stdin() {
    let temp = tempfile::TempDir::new().expect("tempdir should be created");
    let template = shell_command("printf '%s|%s|' \"$1\" \"$TREEBOOT_SPEC_VALUE\"; cat; pwd")
        .with_prefix_arg("treeboot-spec-prefix");
    let invocation = Invocation::new()
        .arg(OsString::from("native-argument"))
        .current_dir(temp.path())
        .env("TREEBOOT_SPEC_VALUE", "environment")
        .stdin(StdinMode::Piped(b"stdin".to_vec()));

    let output = LocalProcessRunner::new(template)
        .run(&invocation)
        .expect("candidate should run");

    assert_eq!(output.termination(), Termination::Exited { code: 0 });
    let stdout = String::from_utf8(output.stdout().to_vec()).expect("stdout should be UTF-8");
    assert!(stdout.starts_with("native-argument|environment|stdin"));
    assert!(
        stdout
            .trim_end()
            .ends_with(&temp.path().display().to_string())
    );
}

#[test]
#[cfg(unix)]
fn local_runner_should_remove_environment_values() {
    let template = shell_command("if [ -z \"${CARGO_MANIFEST_DIR+x}\" ]; then exit 0; fi; exit 9");
    let invocation = Invocation::new().env_remove("CARGO_MANIFEST_DIR");

    let output = LocalProcessRunner::new(template)
        .run(&invocation)
        .expect("candidate should run");

    assert_eq!(output.termination(), Termination::Exited { code: 0 });
}

#[test]
#[cfg(unix)]
fn local_runner_should_capture_partial_output_and_terminate_on_timeout() {
    let template = shell_command("printf before; printf error >&2; sleep 30; printf after");
    let invocation = Invocation::new().timeout(Duration::from_millis(100));

    let output = LocalProcessRunner::new(template)
        .run(&invocation)
        .expect("timed out candidates still produce an invocation result");

    assert_eq!(output.termination(), Termination::TimedOut);
    assert_eq!(output.stdout(), b"before");
    assert_eq!(output.stderr(), b"error");
    assert!(output.duration() < Duration::from_secs(10));
}

#[test]
#[cfg(unix)]
fn local_runner_timeout_should_cover_blocked_stdin_writes() {
    let template = shell_command("sleep 30");
    let invocation = Invocation::new()
        .stdin(StdinMode::Piped(vec![b'x'; 16 * 1024 * 1024]))
        .timeout(Duration::from_millis(100));

    let output = LocalProcessRunner::new(template)
        .run(&invocation)
        .expect("timed out candidates still produce an invocation result");

    assert_eq!(output.termination(), Termination::TimedOut);
    assert!(output.duration() < Duration::from_secs(10));
}

#[test]
fn local_runner_should_report_launch_failures() {
    let invocation = Invocation::new();
    let error = LocalProcessRunner::new(CommandTemplate::new(
        "treeboot-spec-candidate-that-does-not-exist",
    ))
    .run(&invocation)
    .expect_err("missing candidate should fail to launch");

    assert!(error.to_string().contains("failed to launch"));
}

#[test]
fn local_runner_should_reject_terminal_input_without_terminal_capability() {
    let invocation = Invocation::new().stdin(StdinMode::Terminal(b"yes\n".to_vec()));
    let error = LocalProcessRunner::new(shell_command("exit 0"))
        .run(&invocation)
        .expect_err("terminal input requires an adapter capability");

    assert!(error.to_string().contains("terminal input"));
}
