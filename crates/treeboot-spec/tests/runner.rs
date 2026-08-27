#[cfg(unix)]
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
#[cfg(unix)]
fn local_runner_timeout_should_cover_output_held_by_descendant_after_leader_exit() {
    let template = shell_command("(sleep 30) & printf leader-exited; exit 0");
    let invocation = Invocation::new().timeout(Duration::from_millis(100));
    let started = std::time::Instant::now();

    let output = LocalProcessRunner::new(template)
        .run(&invocation)
        .expect("timed out descendants still produce an invocation result");

    assert_eq!(output.termination(), Termination::TimedOut);
    assert_eq!(output.stdout(), b"leader-exited");
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
#[cfg(windows)]
fn local_runner_timeout_should_terminate_job_after_leader_exit() {
    let fixture = tempfile::TempDir::new().expect("fixture directory should be created");
    let held_file = fixture.path().join("descendant-held-file");
    let template = CommandTemplate::with_args(
        std::env::current_exe().expect("current test executable should resolve"),
        [
            "windows_job_leader_helper",
            "--exact",
            "--ignored",
            "--nocapture",
        ],
    );
    let invocation = Invocation::new()
        .env("TREEBOOT_SPEC_HELD_FILE", &held_file)
        .timeout(Duration::from_secs(5));
    let started = std::time::Instant::now();

    let output = LocalProcessRunner::new(template)
        .run(&invocation)
        .expect("timed out Windows jobs still produce an invocation result");

    assert_eq!(output.termination(), Termination::TimedOut);
    assert!(
        String::from_utf8_lossy(output.stdout()).contains("leader-exited"),
        "{}",
        String::from_utf8_lossy(output.stdout())
    );
    assert!(started.elapsed() < Duration::from_secs(10));
    assert!(held_file.exists(), "descendant should signal readiness");

    let removal_deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        match std::fs::remove_file(&held_file) {
            Ok(()) => break,
            Err(_) if std::time::Instant::now() < removal_deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(error) => panic!(
                "descendant still holds its non-shareable file after Job Object termination: {error}"
            ),
        }
    }
}

#[test]
#[cfg(windows)]
#[ignore = "helper process for the Windows Job Object timeout regression"]
fn windows_job_leader_helper() {
    use std::io::Write as _;

    let held_file = std::env::var_os("TREEBOOT_SPEC_HELD_FILE")
        .expect("held-file path should be provided to leader helper");
    let _descendant = std::process::Command::new(
        std::env::current_exe().expect("test executable should resolve"),
    )
    .args([
        "windows_job_descendant_helper",
        "--exact",
        "--ignored",
        "--nocapture",
    ])
    .env("TREEBOOT_SPEC_HELD_FILE", &held_file)
    .spawn()
    .expect("descendant helper should spawn");

    let held_file = std::path::Path::new(&held_file);
    let readiness_deadline = std::time::Instant::now() + Duration::from_secs(3);
    while !held_file.exists() && std::time::Instant::now() < readiness_deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        held_file.exists(),
        "descendant helper should signal readiness"
    );
    println!("leader-exited");
    std::io::stdout()
        .flush()
        .expect("leader progress output should flush");
}

#[test]
#[cfg(windows)]
#[ignore = "helper process for the Windows Job Object timeout regression"]
fn windows_job_descendant_helper() {
    use std::io::Write as _;
    use std::os::windows::fs::OpenOptionsExt as _;

    let held_file = std::env::var_os("TREEBOOT_SPEC_HELD_FILE")
        .expect("held-file path should be provided to descendant helper");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .share_mode(0)
        .open(held_file)
        .expect("descendant should open its readiness file without delete sharing");
    file.write_all(b"descendant-ready\n")
        .expect("descendant should write its readiness signal");
    file.flush()
        .expect("descendant readiness signal should flush");
    std::thread::sleep(Duration::from_secs(30));
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
fn local_runner_should_treat_unrepresentable_timeout_as_unbounded() {
    let output = LocalProcessRunner::new(shell_command("exit 0"))
        .run(&Invocation::new().timeout(Duration::MAX))
        .expect("an unrepresentable deadline should not panic");

    assert_eq!(output.termination(), Termination::Exited { code: 0 });
}

#[test]
fn local_runner_should_reject_terminal_input_without_terminal_capability() {
    let invocation = Invocation::new().stdin(StdinMode::Terminal(b"yes\n".to_vec()));
    let error = LocalProcessRunner::new(shell_command("exit 0"))
        .run(&invocation)
        .expect_err("terminal input requires an adapter capability");

    assert!(error.to_string().contains("terminal input"));
}
