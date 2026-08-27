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
