use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use treeboot_spec::{
    CommandTemplate, Invocation, InvocationResult, RunOptions, Runner, RunnerError, Suite,
    Termination,
};

struct RecordingRunner {
    command: CommandTemplate,
    invocations: AtomicUsize,
}

struct ErrorRunner {
    command: CommandTemplate,
    unsupported: bool,
}

impl Runner for ErrorRunner {
    fn command(&self) -> &CommandTemplate {
        &self.command
    }

    fn run(&self, _invocation: &Invocation) -> Result<InvocationResult, RunnerError> {
        if self.unsupported {
            return Err(RunnerError::UnsupportedCapability {
                capability: "test capability",
            });
        }
        Err(RunnerError::Launch {
            program: "candidate-does-not-support-launching".to_owned(),
            source: io::Error::new(
                io::ErrorKind::NotFound,
                "does not support executable format",
            ),
        })
    }
}

struct SignaledRunner {
    command: CommandTemplate,
}

impl Runner for SignaledRunner {
    fn command(&self) -> &CommandTemplate {
        &self.command
    }

    fn run(&self, _invocation: &Invocation) -> Result<InvocationResult, RunnerError> {
        Ok(InvocationResult::new(
            Termination::Signaled,
            b"treeboot: no config detected".to_vec(),
            b"treeboot: no config detected".to_vec(),
            Duration::from_millis(1),
        ))
    }
}

impl Runner for RecordingRunner {
    fn command(&self) -> &CommandTemplate {
        &self.command
    }

    fn run(&self, invocation: &Invocation) -> Result<InvocationResult, RunnerError> {
        self.invocations.fetch_add(1, Ordering::Relaxed);
        assert_eq!(invocation.arguments(), &["--help"]);
        Ok(InvocationResult::new(
            Termination::Exited { code: 0 },
            b"treeboot usage\n".to_vec(),
            Vec::new(),
            Duration::from_millis(1),
        ))
    }
}

#[test]
fn custom_runner_executes_the_same_private_case_body() {
    let runner = Arc::new(RecordingRunner {
        command: CommandTemplate::new("remote-treeboot"),
        invocations: AtomicUsize::new(0),
    });
    let adapter: Arc<dyn Runner> = runner.clone();

    let report = Suite::current().run_with(
        adapter,
        RunOptions {
            filter: Some("cli.help.print-usage".to_owned()),
            ..RunOptions::default()
        },
    );

    assert!(report.passed());
    assert_eq!(report.cases.len(), 1);
    assert_eq!(runner.invocations.load(Ordering::Relaxed), 1);
}

#[test]
fn display_text_does_not_turn_runner_errors_into_skips() {
    let report = Suite::current().run_with(
        Arc::new(ErrorRunner {
            command: CommandTemplate::new("remote-treeboot"),
            unsupported: false,
        }),
        RunOptions {
            filter: Some("cli.help.print-usage".to_owned()),
            ..RunOptions::default()
        },
    );

    assert!(matches!(
        report.cases[0].outcome,
        treeboot_spec::CaseOutcome::Error { .. }
    ));
}

#[test]
fn unsupported_capability_is_an_explicit_skip() {
    let report = Suite::current().run_with(
        Arc::new(ErrorRunner {
            command: CommandTemplate::new("remote-treeboot"),
            unsupported: true,
        }),
        RunOptions {
            filter: Some("cli.help.print-usage".to_owned()),
            ..RunOptions::default()
        },
    );

    assert!(matches!(
        report.cases[0].outcome,
        treeboot_spec::CaseOutcome::Skipped { .. }
    ));
    assert!(report.passed());
}

#[test]
fn signaled_candidate_cannot_satisfy_exit_code_one_assertions() {
    let report = Suite::current().run_with(
        Arc::new(SignaledRunner {
            command: CommandTemplate::new("remote-treeboot"),
        }),
        RunOptions {
            filter: Some("run.strict-missing-config.exit-with-runtime-failure".to_owned()),
            ..RunOptions::default()
        },
    );

    assert!(!report.passed());
    assert!(matches!(
        report.cases[0].outcome,
        treeboot_spec::CaseOutcome::Failed { .. }
    ));
}

#[test]
fn empty_report_cannot_pass() {
    let report = Suite::current().run_with(
        Arc::new(RecordingRunner {
            command: CommandTemplate::new("remote-treeboot"),
            invocations: AtomicUsize::new(0),
        }),
        RunOptions {
            filter: Some("no-case-has-this-id".to_owned()),
            ..RunOptions::default()
        },
    );

    assert!(!report.passed());
    assert!(report.cases.is_empty());
}

#[test]
fn completion_execution_case_skips_before_using_runner_without_capability() {
    let runner = Arc::new(RecordingRunner {
        command: CommandTemplate::new("remote-treeboot"),
        invocations: AtomicUsize::new(0),
    });
    let adapter: Arc<dyn Runner> = runner.clone();
    let report = Suite::current().run_with(
        adapter,
        RunOptions {
            filter: Some("closure.completions.installed-zsh-script-lists-root-sources".to_owned()),
            ..RunOptions::default()
        },
    );

    assert!(matches!(
        report.cases[0].outcome,
        treeboot_spec::CaseOutcome::Skipped { .. }
    ));
    assert_eq!(runner.invocations.load(Ordering::Relaxed), 0);
}
