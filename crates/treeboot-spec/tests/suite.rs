use std::io;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use treeboot_spec::{
    CaseOutcome, CommandTemplate, ConformanceProfile, Invocation, InvocationResult, RunOptions,
    Runner, RunnerError, Suite, SuiteEvent, Termination,
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

struct TimedOutRunner {
    command: CommandTemplate,
}

struct ConcurrentRunner {
    command: CommandTemplate,
    active: AtomicUsize,
    maximum_active: AtomicUsize,
    invocations: AtomicUsize,
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

impl Runner for TimedOutRunner {
    fn command(&self) -> &CommandTemplate {
        &self.command
    }

    fn run(&self, _invocation: &Invocation) -> Result<InvocationResult, RunnerError> {
        Ok(InvocationResult::new(
            Termination::TimedOut,
            b"partial stdout".to_vec(),
            b"partial stderr".to_vec(),
            Duration::from_millis(10),
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

impl Runner for ConcurrentRunner {
    fn command(&self) -> &CommandTemplate {
        &self.command
    }

    fn run(&self, _invocation: &Invocation) -> Result<InvocationResult, RunnerError> {
        let invocation = self.invocations.fetch_add(1, Ordering::SeqCst);
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.maximum_active.fetch_max(active, Ordering::SeqCst);
        if invocation == 0 {
            std::thread::sleep(Duration::from_millis(100));
        } else {
            std::thread::sleep(Duration::from_millis(10));
        }
        self.active.fetch_sub(1, Ordering::SeqCst);

        Ok(InvocationResult::new(
            Termination::Exited { code: 0 },
            b"treeboot usage\n".to_vec(),
            Vec::new(),
            Duration::from_millis(1),
        ))
    }
}

#[test]
fn run_options_builder_configures_public_execution_options() {
    assert_eq!(RunOptions::new().concurrency, NonZeroUsize::MIN);

    let options = RunOptions::new()
        .with_profile(ConformanceProfile::Functional)
        .with_filter("cli.help")
        .with_invocation_timeout(Duration::from_secs(7))
        .with_concurrency(NonZeroUsize::new(3).unwrap());

    assert_eq!(options.profile, ConformanceProfile::Functional);
    assert_eq!(options.filter.as_deref(), Some("cli.help"));
    assert_eq!(options.invocation_timeout, Duration::from_secs(7));
    assert_eq!(options.concurrency, NonZeroUsize::new(3).unwrap());
}

#[test]
fn concurrent_run_overlaps_cases_and_preserves_registry_order() {
    let runner = Arc::new(ConcurrentRunner {
        command: CommandTemplate::new("remote-treeboot"),
        active: AtomicUsize::new(0),
        maximum_active: AtomicUsize::new(0),
        invocations: AtomicUsize::new(0),
    });
    let expected_ids = Suite::current()
        .cases()
        .filter(|case| case.id().contains("cli."))
        .map(|case| case.id())
        .collect::<Vec<_>>();
    let suite_thread = std::thread::current().id();
    let report = Suite::current().run_with_observer(
        runner.clone(),
        RunOptions::new()
            .with_filter("cli.")
            .with_concurrency(NonZeroUsize::new(3).unwrap()),
        |_| assert_eq!(std::thread::current().id(), suite_thread),
    );

    let report_ids = report
        .cases
        .iter()
        .map(|result| result.case.id())
        .collect::<Vec<_>>();
    assert_eq!(report_ids, expected_ids);
    assert!(runner.maximum_active.load(Ordering::SeqCst) >= 2);
}

#[test]
fn concurrent_run_preserves_error_failure_and_timeout_outcomes() {
    #[derive(Debug, Clone, Copy)]
    enum ExpectedOutcome {
        Error,
        Failed,
        TimedOut,
    }

    for (runner, expected) in [
        (
            Arc::new(ErrorRunner {
                command: CommandTemplate::new("remote-treeboot"),
                unsupported: false,
            }) as Arc<dyn Runner>,
            ExpectedOutcome::Error,
        ),
        (
            Arc::new(SignaledRunner {
                command: CommandTemplate::new("remote-treeboot"),
            }) as Arc<dyn Runner>,
            ExpectedOutcome::Failed,
        ),
        (
            Arc::new(TimedOutRunner {
                command: CommandTemplate::new("remote-treeboot"),
            }) as Arc<dyn Runner>,
            ExpectedOutcome::TimedOut,
        ),
    ] {
        let serial =
            Suite::current().run_with(runner.clone(), RunOptions::new().with_filter("cli."));
        let concurrent = Suite::current().run_with(
            runner,
            RunOptions::new()
                .with_filter("cli.")
                .with_concurrency(NonZeroUsize::new(3).unwrap()),
        );

        assert_eq!(serial.cases.len(), concurrent.cases.len());
        for (serial, concurrent) in serial.cases.iter().zip(&concurrent.cases) {
            assert_eq!(serial.case.id(), concurrent.case.id());
            assert_eq!(serial.outcome, concurrent.outcome);
        }
        assert!(
            concurrent.cases.iter().any(|result| match expected {
                ExpectedOutcome::Error => matches!(result.outcome, CaseOutcome::Error { .. }),
                ExpectedOutcome::Failed => matches!(result.outcome, CaseOutcome::Failed { .. }),
                ExpectedOutcome::TimedOut => {
                    matches!(result.outcome, CaseOutcome::TimedOut { .. })
                }
            }),
            "expected at least one {expected:?} result",
        );
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
        RunOptions::new().with_filter("cli.help.print-usage"),
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
        RunOptions::new().with_filter("cli.help.print-usage"),
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
        RunOptions::new().with_filter("cli.help.print-usage"),
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
        RunOptions::new().with_filter("run.strict-missing-config.exit-with-runtime-failure"),
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
        RunOptions::new().with_filter("no-case-has-this-id"),
    );

    assert!(!report.passed());
    assert!(report.cases.is_empty());
}

#[test]
fn functional_profile_omits_exact_cases_before_execution() {
    let runner = Arc::new(RecordingRunner {
        command: CommandTemplate::new("remote-treeboot"),
        invocations: AtomicUsize::new(0),
    });
    let mut events = Vec::new();
    let report = Suite::current().run_with_observer(
        runner.clone(),
        RunOptions::new()
            .with_profile(ConformanceProfile::Functional)
            .with_filter("closure.exact."),
        |event| match event {
            SuiteEvent::SuiteStarted { selected_cases } => events.push(selected_cases),
            _ => panic!("an omitted case must not emit a case event"),
        },
    );

    assert!(report.cases.is_empty());
    assert_eq!(report.profile(), ConformanceProfile::Functional);
    assert_eq!(report.omitted_exact_case_count(), 6);
    assert_eq!(runner.invocations.load(Ordering::Relaxed), 0);
    assert_eq!(events, [0]);
}

#[test]
fn observed_run_emits_ordered_events_for_selected_skip() {
    let mut events = Vec::new();
    let report = Suite::current().run_with_observer(
        Arc::new(RecordingRunner {
            command: CommandTemplate::new("remote-treeboot"),
            invocations: AtomicUsize::new(0),
        }),
        RunOptions::new()
            .with_filter("closure.completions.installed-bash-script-lists-root-sources"),
        |event| match event {
            SuiteEvent::SuiteStarted { selected_cases } => {
                events.push(format!("suite:{selected_cases}"));
            }
            SuiteEvent::CaseStarted { index, total, case } => {
                events.push(format!("start:{index}/{total}:{}", case.id()))
            }
            SuiteEvent::CaseFinished {
                index,
                total,
                result,
            } => {
                assert!(matches!(result.outcome, CaseOutcome::Skipped { .. }));
                events.push(format!("finish:{index}/{total}:{}", result.case.id()));
            }
            _ => {}
        },
    );

    assert_eq!(report.cases.len(), 1);
    assert_eq!(
        events,
        [
            "suite:1",
            "start:1/1:closure.completions.installed-bash-script-lists-root-sources",
            "finish:1/1:closure.completions.installed-bash-script-lists-root-sources",
        ]
    );
}

#[test]
fn profile_fields_do_not_change_serialized_report_shape() {
    let report = Suite::current().run_with(
        Arc::new(RecordingRunner {
            command: CommandTemplate::new("remote-treeboot"),
            invocations: AtomicUsize::new(0),
        }),
        RunOptions::new()
            .with_profile(ConformanceProfile::Functional)
            .with_filter("no-case-has-this-id"),
    );
    let value = serde_json::to_value(report).unwrap();

    assert!(value.get("profile").is_none());
    assert!(value.get("omitted_exact_case_count").is_none());
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
        RunOptions::new()
            .with_filter("closure.completions.installed-fish-script-lists-root-sources"),
    );

    assert!(matches!(
        &report.cases[0].outcome,
        treeboot_spec::CaseOutcome::Skipped { reason }
            if reason == "runner cannot execute generated completion scripts on the fixture host"
    ));
    assert_eq!(report.cases.len(), 1);
    assert_eq!(runner.invocations.load(Ordering::Relaxed), 0);
}

#[test]
fn terminal_backed_completion_gaps_should_remain_explicit() {
    for (id, expected_reason) in [
        (
            "closure.completions.installed-bash-script-lists-root-sources",
            "requires a terminal-backed Bash Readline harness because Bash has no portable non-interactive public completion API",
        ),
        (
            "closure.completions.installed-zsh-script-lists-root-sources",
            "requires a terminal-backed Zsh ZLE harness because Zsh has no portable non-interactive public completion API",
        ),
    ] {
        let runner = Arc::new(RecordingRunner {
            command: CommandTemplate::new("remote-treeboot"),
            invocations: AtomicUsize::new(0),
        });
        let adapter: Arc<dyn Runner> = runner.clone();
        let report = Suite::current().run_with(adapter, RunOptions::new().with_filter(id));

        assert_eq!(report.cases.len(), 1);
        assert!(matches!(
            &report.cases[0].outcome,
            treeboot_spec::CaseOutcome::Skipped { reason } if reason == expected_reason
        ));
        assert_eq!(runner.invocations.load(Ordering::Relaxed), 0);
    }
}
