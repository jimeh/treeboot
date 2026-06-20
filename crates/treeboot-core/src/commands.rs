use std::io::{BufRead, BufReader, Read};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;

use crate::{
    CommandKind, Error, OutputEvent, OutputStream, PlannedCommand, Reporter, Result, RunContext,
    RunPlan,
};

/// Options that affect command execution.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct CommandExecutionOptions {
    /// Prints planned commands without spawning processes.
    pub dry_run: bool,
}

pub(crate) fn execute_commands(
    plan: &RunPlan,
    options: CommandExecutionOptions,
    reporter: &mut dyn Reporter,
) -> Result<()> {
    let mut index = 0;

    while index < plan.commands.len() {
        let command = &plan.commands[index];

        if command.async_command {
            let batch_start = index;
            while index < plan.commands.len() && plan.commands[index].async_command {
                index += 1;
            }
            let batch = &plan.commands[batch_start..index];

            if options.dry_run {
                report_would_run_batch(batch, reporter)?;
            } else {
                run_async_batch(batch, &plan.context, reporter)?;
            }
        } else {
            if options.dry_run {
                report(
                    reporter,
                    OutputEvent::CommandWouldRun {
                        label: command_label(command),
                    },
                )?;
            } else {
                run_sequential(command, &plan.context, reporter)?;
            }
            index += 1;
        }
    }

    Ok(())
}

fn report_would_run_batch(batch: &[PlannedCommand], reporter: &mut dyn Reporter) -> Result<()> {
    let labels = batch.iter().map(command_label).collect::<Vec<_>>();
    report(
        reporter,
        OutputEvent::CommandWouldRunBatch {
            labels: labels.clone(),
        },
    )?;

    for label in labels {
        report(reporter, OutputEvent::CommandWouldRun { label })?;
    }

    Ok(())
}

fn run_sequential(
    command: &PlannedCommand,
    context: &RunContext,
    reporter: &mut dyn Reporter,
) -> Result<()> {
    let label = command_label(command);
    report(
        reporter,
        OutputEvent::CommandStarted {
            label: label.clone(),
        },
    )?;

    let status = match build_command(command, context).status() {
        Ok(status) => status,
        Err(source) => {
            if command.allow_failure {
                report_allowed_failure(reporter, label, format!("failed to start: {source}"))?;
                return Ok(());
            }

            return Err(Error::CommandIo { label, source });
        }
    };

    handle_exit_status(command.allow_failure, label, status, reporter)
}

fn handle_exit_status(
    allow_failure: bool,
    label: String,
    status: ExitStatus,
    reporter: &mut dyn Reporter,
) -> Result<()> {
    if status.success() {
        return Ok(());
    }

    if allow_failure {
        report_allowed_failure(reporter, label, format!("failed with {status}"))
    } else {
        Err(Error::CommandFailed { label, status })
    }
}

fn run_async_batch(
    batch: &[PlannedCommand],
    context: &RunContext,
    reporter: &mut dyn Reporter,
) -> Result<()> {
    let (sender, receiver) = mpsc::channel();
    let mut handles = Vec::new();
    let mut started = 0;
    let mut streams = 0;
    let mut failures = Vec::new();

    for command in batch {
        let label = command_label(command);
        report(
            reporter,
            OutputEvent::CommandStarted {
                label: label.clone(),
            },
        )?;

        let mut process = build_command(command, context);
        process.stdout(Stdio::piped()).stderr(Stdio::piped());

        let mut child = match process.spawn() {
            Ok(child) => child,
            Err(source) => {
                if command.allow_failure {
                    report_allowed_failure(reporter, label, format!("failed to start: {source}"))?;
                } else {
                    failures.push(label);
                }
                continue;
            }
        };

        started += 1;

        if let Some(stdout) = child.stdout.take() {
            streams += 1;
            handles.push(spawn_output_reader(
                stdout,
                OutputStream::Stdout,
                label.clone(),
                sender.clone(),
            ));
        }
        if let Some(stderr) = child.stderr.take() {
            streams += 1;
            handles.push(spawn_output_reader(
                stderr,
                OutputStream::Stderr,
                label.clone(),
                sender.clone(),
            ));
        }

        let allow_failure = command.allow_failure;
        let exit_sender = sender.clone();
        handles.push(thread::spawn(move || {
            let result = child.wait();
            let _ = exit_sender.send(AsyncMessage::Exit {
                label,
                allow_failure,
                result,
            });
        }));
    }

    drop(sender);

    collect_async_messages(receiver, started, streams, reporter, &mut failures)?;

    for handle in handles {
        let _ = handle.join();
    }

    if failures.is_empty() {
        Ok(())
    } else {
        let count = failures.len();
        Err(Error::CommandBatchFailed {
            count,
            plural: if count == 1 { "" } else { "s" },
            labels: failures.join(", "),
        })
    }
}

fn collect_async_messages(
    receiver: mpsc::Receiver<AsyncMessage>,
    expected_exits: usize,
    expected_streams: usize,
    reporter: &mut dyn Reporter,
    failures: &mut Vec<String>,
) -> Result<()> {
    let mut exits = 0;
    let mut streams = 0;

    while exits < expected_exits || streams < expected_streams {
        match receiver.recv() {
            Ok(AsyncMessage::Line {
                label,
                stream,
                line,
            }) => report(
                reporter,
                OutputEvent::CommandOutput {
                    label,
                    stream,
                    line,
                },
            )?,
            Ok(AsyncMessage::StreamDone) => {
                streams += 1;
            }
            Ok(AsyncMessage::Exit {
                label,
                allow_failure,
                result,
            }) => {
                exits += 1;
                match result {
                    Ok(status) if status.success() => {}
                    Ok(status) if allow_failure => {
                        report_allowed_failure(reporter, label, format!("failed with {status}"))?;
                    }
                    Ok(_) => failures.push(label),
                    Err(source) if allow_failure => {
                        report_allowed_failure(
                            reporter,
                            label,
                            format!("failed to wait: {source}"),
                        )?;
                    }
                    Err(_) => failures.push(label),
                }
            }
            Err(_) => break,
        }
    }

    Ok(())
}

fn spawn_output_reader<R>(
    reader: R,
    stream: OutputStream,
    label: String,
    sender: mpsc::Sender<AsyncMessage>,
) -> thread::JoinHandle<()>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        let mut buffer = Vec::new();

        loop {
            buffer.clear();
            match reader.read_until(b'\n', &mut buffer) {
                Ok(0) => break,
                Ok(_) => {
                    trim_line_ending(&mut buffer);
                    let line = String::from_utf8_lossy(&buffer).into_owned();
                    if sender
                        .send(AsyncMessage::Line {
                            label: label.clone(),
                            stream,
                            line,
                        })
                        .is_err()
                    {
                        return;
                    }
                }
                Err(_) => break,
            }
        }

        let _ = sender.send(AsyncMessage::StreamDone);
    })
}

fn trim_line_ending(buffer: &mut Vec<u8>) {
    if buffer.ends_with(b"\n") {
        buffer.pop();
    }
    if buffer.ends_with(b"\r") {
        buffer.pop();
    }
}

fn report_allowed_failure(
    reporter: &mut dyn Reporter,
    label: String,
    reason: String,
) -> Result<()> {
    report(
        reporter,
        OutputEvent::CommandAllowedFailure { label, reason },
    )
}

fn build_command(command: &PlannedCommand, context: &RunContext) -> Command {
    let mut process = match &command.command {
        CommandKind::Shell { run } => build_shell_command(run),
        CommandKind::Direct { program, args } => {
            let mut process = Command::new(program);
            process.args(args);
            process
        }
    };

    process
        .current_dir(&command.cwd_path)
        .envs(&context.environment)
        .envs(&command.env);
    process
}

#[cfg(windows)]
fn build_shell_command(run: &str) -> Command {
    let mut process = Command::new("cmd");
    process.args(["/C", run]);
    process
}

#[cfg(not(windows))]
fn build_shell_command(run: &str) -> Command {
    let mut process = Command::new("sh");
    process.args(["-c", run]);
    process
}

fn command_label(command: &PlannedCommand) -> String {
    let invocation = invocation_label(&command.command);

    if let Some(name) = &command.name {
        format!("{name}: {invocation}")
    } else {
        invocation
    }
}

fn invocation_label(command: &CommandKind) -> String {
    match command {
        CommandKind::Shell { run } => run.clone(),
        CommandKind::Direct { program, args } => {
            if args.is_empty() {
                program.clone()
            } else {
                format!("{} {}", program, args.join(" "))
            }
        }
    }
}

fn report(reporter: &mut dyn Reporter, event: OutputEvent) -> Result<()> {
    reporter
        .report(event)
        .map_err(|source| Error::Output { source })
}

enum AsyncMessage {
    Line {
        label: String,
        stream: OutputStream,
        line: String,
    },
    StreamDone,
    Exit {
        label: String,
        allow_failure: bool,
        result: std::io::Result<ExitStatus>,
    },
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::{RunPlan, SourceSpan};

    #[test]
    fn command_label_should_include_name_and_invocation() {
        let command = planned_command(
            Some("Install packages"),
            CommandKind::Shell {
                run: "npm install".to_owned(),
            },
        );

        assert_eq!(
            command_label(&command.inner),
            "Install packages: npm install"
        );
    }

    #[test]
    fn command_label_should_format_direct_invocation() {
        let command = planned_command(
            None,
            CommandKind::Direct {
                program: "cargo".to_owned(),
                args: vec!["test".to_owned(), "--locked".to_owned()],
            },
        );

        assert_eq!(command_label(&command.inner), "cargo test --locked");
    }

    #[test]
    fn trim_line_ending_should_flush_trailing_fragment() {
        let mut line = b"partial".to_vec();

        trim_line_ending(&mut line);

        assert_eq!(line, b"partial");
    }

    #[cfg(unix)]
    #[test]
    fn execute_commands_should_run_shell_command_with_merged_env_and_cwd() {
        let (temp, context) = context("shell-env-cwd");
        let app = temp.path().join("worktree/app");
        std::fs::create_dir_all(&app).expect("app dir should be created");
        let marker = temp.path().join("worktree/app/marker");
        let command = planned_command(
            None,
            CommandKind::Shell {
                run: format!(
                    "printf '%s:%s' \"$TREEBOOT_ROOT_PATH\" \"$LOCAL_VALUE\" > {}",
                    shell_path(&marker)
                ),
            },
        )
        .with_cwd(app)
        .with_env("LOCAL_VALUE", "local");
        let plan = plan(context, vec![command]);

        execute_commands(
            &plan,
            CommandExecutionOptions::default(),
            &mut Recorder::default(),
        )
        .expect("command should run");

        assert_eq!(
            std::fs::read_to_string(marker).expect("marker should be readable"),
            format!("{}:local", temp.path().join("root").display())
        );
    }

    #[cfg(unix)]
    #[test]
    fn execute_commands_should_report_dry_run_without_spawning() {
        let (temp, context) = context("dry-run");
        let marker = temp.path().join("worktree/marker");
        let command = planned_command(
            None,
            CommandKind::Shell {
                run: format!("touch {}", shell_path(&marker)),
            },
        );
        let plan = plan(context, vec![command]);
        let mut reporter = Recorder::default();

        execute_commands(
            &plan,
            CommandExecutionOptions { dry_run: true },
            &mut reporter,
        )
        .expect("dry-run should succeed");

        assert!(!marker.exists());
        assert_eq!(
            reporter.messages(),
            vec![format!("treeboot: would run touch {}", marker.display())]
        );
    }

    #[cfg(unix)]
    #[test]
    fn execute_commands_should_report_dry_run_async_batch_without_spawning() {
        let (temp, context) = context("dry-run-async");
        let first_marker = temp.path().join("worktree/first");
        let second_marker = temp.path().join("worktree/second");
        let first = planned_command(
            Some("first"),
            CommandKind::Shell {
                run: format!("touch {}", shell_path(&first_marker)),
            },
        )
        .with_async();
        let second = planned_command(
            Some("second"),
            CommandKind::Shell {
                run: format!("touch {}", shell_path(&second_marker)),
            },
        )
        .with_async();
        let plan = plan(context, vec![first, second]);
        let mut reporter = Recorder::default();

        execute_commands(
            &plan,
            CommandExecutionOptions { dry_run: true },
            &mut reporter,
        )
        .expect("dry-run should succeed");

        assert!(!first_marker.exists());
        assert!(!second_marker.exists());
        assert_eq!(
            reporter.messages(),
            vec![
                format!(
                    "treeboot: would run async batch: first: touch {}, second: touch {}",
                    first_marker.display(),
                    second_marker.display()
                ),
                format!(
                    "treeboot: would run first: touch {}",
                    first_marker.display()
                ),
                format!(
                    "treeboot: would run second: touch {}",
                    second_marker.display()
                ),
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn execute_commands_should_warn_and_continue_after_allowed_failure() {
        let (temp, context) = context("allowed-failure");
        let marker = temp.path().join("worktree/marker");
        let failing = planned_command(
            Some("optional"),
            CommandKind::Shell {
                run: "exit 7".to_owned(),
            },
        )
        .with_allow_failure();
        let next = planned_command(
            None,
            CommandKind::Shell {
                run: format!("touch {}", shell_path(&marker)),
            },
        );
        let plan = plan(context, vec![failing, next]);
        let mut reporter = Recorder::default();

        execute_commands(&plan, CommandExecutionOptions::default(), &mut reporter)
            .expect("allowed failure should continue");

        assert!(marker.exists());
        assert!(reporter.messages().iter().any(|message| {
            message == "treeboot: warning: command optional: exit 7 failed with exit status: 7"
        }));
    }

    #[cfg(unix)]
    #[test]
    fn execute_commands_should_stop_after_fatal_singleton_failure() {
        let (temp, context) = context("fatal-singleton");
        let marker = temp.path().join("worktree/marker");
        let failing = planned_command(
            Some("required"),
            CommandKind::Shell {
                run: "exit 6".to_owned(),
            },
        );
        let next = planned_command(
            None,
            CommandKind::Shell {
                run: format!("touch {}", shell_path(&marker)),
            },
        );
        let plan = plan(context, vec![failing, next]);

        let error = execute_commands(
            &plan,
            CommandExecutionOptions::default(),
            &mut Recorder::default(),
        )
        .expect_err("fatal failure should fail");

        assert!(!marker.exists());
        assert!(
            error
                .to_string()
                .contains("command required: exit 6 failed")
        );
    }

    #[cfg(unix)]
    #[test]
    fn execute_commands_should_buffer_async_output_and_flush_trailing_fragments() {
        let (_temp, context) = context("async-output");
        let first = planned_command(
            Some("first"),
            CommandKind::Shell {
                run: "printf 'out'; printf 'err\\n' >&2".to_owned(),
            },
        )
        .with_async();
        let second = planned_command(
            Some("second"),
            CommandKind::Shell {
                run: "printf 'line\\n'; printf 'tail' >&2".to_owned(),
            },
        )
        .with_async();
        let plan = plan(context, vec![first, second]);
        let mut reporter = Recorder::default();

        execute_commands(&plan, CommandExecutionOptions::default(), &mut reporter)
            .expect("async commands should run");

        assert!(reporter.events.iter().any(|event| {
            matches!(
                event,
                OutputEvent::CommandOutput {
                    label,
                    stream: OutputStream::Stdout,
                    line,
                } if label == "first: printf 'out'; printf 'err\\n' >&2"
                    && line == "out"
            )
        }));
        assert!(reporter.events.iter().any(|event| {
            matches!(
                event,
                OutputEvent::CommandOutput {
                    label,
                    stream: OutputStream::Stderr,
                    line,
                } if label == "second: printf 'line\\n'; printf 'tail' >&2"
                    && line == "tail"
            )
        }));
    }

    #[cfg(unix)]
    #[test]
    fn execute_commands_should_wait_for_all_async_failures() {
        let (temp, context) = context("async-failures");
        let marker = temp.path().join("worktree/marker");
        let allowed = planned_command(
            Some("allowed"),
            CommandKind::Shell {
                run: "exit 3".to_owned(),
            },
        )
        .with_async()
        .with_allow_failure();
        let fatal_one = planned_command(
            Some("fatal one"),
            CommandKind::Shell {
                run: format!("touch {}; exit 4", shell_path(&marker)),
            },
        )
        .with_async();
        let fatal_two = planned_command(
            Some("fatal two"),
            CommandKind::Shell {
                run: "exit 5".to_owned(),
            },
        )
        .with_async();
        let later = planned_command(
            Some("later"),
            CommandKind::Shell {
                run: "exit 0".to_owned(),
            },
        );
        let plan = plan(context, vec![allowed, fatal_one, fatal_two, later]);
        let mut reporter = Recorder::default();

        let error = execute_commands(&plan, CommandExecutionOptions::default(), &mut reporter)
            .expect_err("batch should fail");

        assert!(marker.exists());
        assert_eq!(
            error.to_string(),
            "2 async commands failed: fatal one: touch ".to_owned()
                + marker.to_str().expect("marker path should be utf-8")
                + "; exit 4, fatal two: exit 5"
        );
        assert!(reporter.messages().iter().any(|message| {
            message == "treeboot: warning: command allowed: exit 3 failed with exit status: 3"
        }));
        assert!(
            !reporter
                .messages()
                .iter()
                .any(|message| message == "treeboot: run later: exit 0")
        );
    }

    #[cfg(unix)]
    #[test]
    fn execute_commands_should_allow_spawn_failures_when_configured() {
        let (temp, context) = context("allowed-spawn");
        let marker = temp.path().join("worktree/marker");
        let missing = planned_command(
            Some("optional missing"),
            CommandKind::Direct {
                program: "treeboot-missing-program-for-test".to_owned(),
                args: Vec::new(),
            },
        )
        .with_allow_failure();
        let next = planned_command(
            None,
            CommandKind::Shell {
                run: format!("touch {}", shell_path(&marker)),
            },
        );
        let plan = plan(context, vec![missing, next]);
        let mut reporter = Recorder::default();

        execute_commands(&plan, CommandExecutionOptions::default(), &mut reporter)
            .expect("allowed spawn failure should continue");

        assert!(marker.exists());
        assert!(
            reporter
                .messages()
                .iter()
                .any(|message| message.contains("failed to start:"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn execute_commands_should_wait_for_async_sibling_after_spawn_failure() {
        let (temp, context) = context("async-spawn-failure");
        let marker = temp.path().join("worktree/marker");
        let missing = planned_command(
            Some("missing"),
            CommandKind::Direct {
                program: "treeboot-missing-program-for-test".to_owned(),
                args: Vec::new(),
            },
        )
        .with_async();
        let sibling = planned_command(
            Some("sibling"),
            CommandKind::Shell {
                run: format!("touch {}", shell_path(&marker)),
            },
        )
        .with_async();
        let plan = plan(context, vec![missing, sibling]);

        let error = execute_commands(
            &plan,
            CommandExecutionOptions::default(),
            &mut Recorder::default(),
        )
        .expect_err("batch should fail");

        assert!(marker.exists());
        assert_eq!(
            error.to_string(),
            "1 async command failed: missing: treeboot-missing-program-for-test"
        );
    }

    struct TestCommand {
        inner: PlannedCommand,
    }

    impl TestCommand {
        fn with_async(mut self) -> Self {
            self.inner.async_command = true;
            self
        }

        fn with_allow_failure(mut self) -> Self {
            self.inner.allow_failure = true;
            self
        }

        fn with_cwd(mut self, cwd: PathBuf) -> Self {
            self.inner.cwd_path = cwd;
            self
        }

        fn with_env(mut self, key: &str, value: &str) -> Self {
            self.inner.env.insert(key.to_owned(), value.to_owned());
            self
        }
    }

    impl From<TestCommand> for PlannedCommand {
        fn from(command: TestCommand) -> Self {
            command.inner
        }
    }

    fn planned_command(name: Option<&str>, command: CommandKind) -> TestCommand {
        TestCommand {
            inner: PlannedCommand {
                name: name.map(str::to_owned),
                command,
                cwd: None,
                cwd_path: PathBuf::new(),
                env: BTreeMap::new(),
                async_command: false,
                allow_failure: false,
                declaration: SourceSpan {
                    start: 0,
                    end: 0,
                    line: 1,
                    column: 1,
                },
            },
        }
    }

    fn plan(context: RunContext, commands: Vec<impl Into<PlannedCommand>>) -> RunPlan {
        let commands = commands
            .into_iter()
            .map(Into::into)
            .map(|mut command: PlannedCommand| {
                if command.cwd_path.as_os_str().is_empty() {
                    command.cwd_path = context.worktree_path.clone();
                }
                command
            })
            .collect();

        RunPlan {
            config_path: context.worktree_path.join(".treeboot.toml"),
            files: Vec::new(),
            commands,
            context,
        }
    }

    fn context(name: &str) -> (tempfile::TempDir, RunContext) {
        let temp = tempfile::TempDir::new().expect("tempdir should be created");
        let root = temp.path().join("root");
        let worktree = temp.path().join("worktree");
        std::fs::create_dir_all(&root).expect("root should be created");
        std::fs::create_dir_all(&worktree).expect("worktree should be created");
        let mut environment = BTreeMap::new();
        environment.insert(
            "TREEBOOT_ROOT_PATH".to_owned(),
            root.as_os_str().to_os_string(),
        );
        environment.insert(
            "TREEBOOT_WORKTREE_PATH".to_owned(),
            worktree.as_os_str().to_os_string(),
        );
        environment.insert("TREEBOOT_DEFAULT_BRANCH".to_owned(), OsString::from("main"));

        (
            temp,
            RunContext {
                root_path: root,
                worktree_path: worktree,
                default_branch: format!("main-{name}"),
                environment,
            },
        )
    }

    fn shell_path(path: &Path) -> String {
        path.display().to_string().replace('\'', "'\\''")
    }

    #[derive(Default)]
    struct Recorder {
        events: Vec<OutputEvent>,
    }

    impl Recorder {
        fn messages(&self) -> Vec<String> {
            self.events.iter().map(OutputEvent::message).collect()
        }
    }

    impl Reporter for Recorder {
        fn report(&mut self, event: OutputEvent) -> std::io::Result<()> {
            self.events.push(event);
            Ok(())
        }
    }
}
