use std::ffi::{OsStr, OsString};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::process::CommandExt;
#[cfg(unix)]
use std::process::Child;

#[cfg(windows)]
use process_wrap::std::{ChildWrapper, CommandWrap, JobObject};

use serde::Serialize;
use thiserror::Error;

use crate::CommandTemplate;

#[cfg(unix)]
type ManagedChild = Child;
#[cfg(windows)]
type ManagedChild = Box<dyn ChildWrapper>;

/// Input supplied to one candidate invocation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum StdinMode {
    /// Connects the child to an empty input stream.
    #[default]
    Empty,
    /// Writes the provided bytes to a pipe connected to the child.
    ///
    /// A candidate closing the pipe before consuming every byte is normal
    /// completion, not a runner error.
    Piped(Vec<u8>),
    /// Requests a terminal-backed input stream containing the provided bytes.
    Terminal(Vec<u8>),
}

/// One native environment change for a candidate invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentChange {
    name: OsString,
    value: Option<OsString>,
}

impl EnvironmentChange {
    /// Returns the native environment variable name.
    pub fn name(&self) -> &OsStr {
        &self.name
    }

    /// Returns the value to set, or `None` when the variable must be removed.
    pub fn value(&self) -> Option<&OsStr> {
        self.value.as_deref()
    }
}

/// A single candidate invocation built by a conformance case.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Invocation {
    args: Vec<OsString>,
    current_dir: Option<PathBuf>,
    environment: Vec<EnvironmentChange>,
    stdin: StdinMode,
    timeout: Option<Duration>,
    capture_limit: Option<usize>,
}

impl Invocation {
    /// Creates an invocation with no case arguments or environment changes.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends one native argument.
    #[must_use]
    pub fn arg(mut self, argument: impl Into<OsString>) -> Self {
        self.args.push(argument.into());
        self
    }

    /// Appends native arguments in order.
    #[must_use]
    pub fn args<I, S>(mut self, arguments: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.args.extend(arguments.into_iter().map(Into::into));
        self
    }

    /// Sets the candidate working directory.
    #[must_use]
    pub fn current_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.current_dir = Some(path.into());
        self
    }

    /// Sets one environment value for this invocation.
    #[must_use]
    pub fn env(mut self, name: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.environment.push(EnvironmentChange {
            name: name.into(),
            value: Some(value.into()),
        });
        self
    }

    /// Removes one inherited environment value for this invocation.
    #[must_use]
    pub fn env_remove(mut self, name: impl Into<OsString>) -> Self {
        self.environment.push(EnvironmentChange {
            name: name.into(),
            value: None,
        });
        self
    }

    /// Sets the child input mode.
    #[must_use]
    pub fn stdin(mut self, stdin: StdinMode) -> Self {
        self.stdin = stdin;
        self
    }

    /// Sets the maximum duration for this invocation.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Limits retained stdout and stderr bytes independently.
    ///
    /// Runners must continue draining both streams and return
    /// [`RunnerError::OutputLimitExceeded`] if either stream exceeds this
    /// limit.
    #[must_use]
    pub fn capture_limit(mut self, bytes_per_stream: usize) -> Self {
        self.capture_limit = Some(bytes_per_stream);
        self
    }

    /// Returns the case arguments.
    pub fn arguments(&self) -> &[OsString] {
        &self.args
    }

    /// Returns the requested working directory.
    pub fn working_directory(&self) -> Option<&Path> {
        self.current_dir.as_deref()
    }

    /// Returns the environment changes in application order.
    pub fn environment(&self) -> &[EnvironmentChange] {
        &self.environment
    }

    /// Returns the requested input mode.
    pub fn stdin_mode(&self) -> &StdinMode {
        &self.stdin
    }

    /// Returns the invocation timeout.
    pub fn timeout_value(&self) -> Option<Duration> {
        self.timeout
    }

    /// Returns the maximum bytes retained from each output stream.
    pub fn capture_limit_value(&self) -> Option<usize> {
        self.capture_limit
    }
}

/// How a candidate process ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Termination {
    /// The process exited with a platform exit code.
    Exited {
        /// Process exit code.
        code: i32,
    },
    /// The process ended without a portable exit code.
    Signaled,
    /// The runner killed the process after its timeout elapsed.
    TimedOut,
}

impl Termination {
    /// Returns whether the candidate exited successfully.
    pub fn success(self) -> bool {
        matches!(self, Self::Exited { code: 0 })
    }

    /// Returns the portable exit code when one exists.
    pub fn code(self) -> Option<i32> {
        match self {
            Self::Exited { code } => Some(code),
            Self::Signaled | Self::TimedOut => None,
        }
    }
}

/// Captured result of one candidate invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvocationResult {
    termination: Termination,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    duration: Duration,
}

impl InvocationResult {
    /// Creates an owned invocation result, primarily for custom runners.
    pub fn new(
        termination: Termination,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        duration: Duration,
    ) -> Self {
        Self {
            termination,
            stdout,
            stderr,
            duration,
        }
    }

    /// Returns how the process ended.
    pub fn termination(&self) -> Termination {
        self.termination
    }

    /// Returns captured standard output bytes.
    pub fn stdout(&self) -> &[u8] {
        &self.stdout
    }

    /// Returns captured standard error bytes.
    pub fn stderr(&self) -> &[u8] {
        &self.stderr
    }

    /// Returns elapsed wall-clock time.
    pub fn duration(&self) -> Duration {
        self.duration
    }
}

/// Capabilities offered by a runner adapter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct RunnerCapabilities {
    /// Whether the runner can attach terminal-backed input to the candidate.
    pub terminal_input: bool,
    /// Whether generated completion scripts can reinvoke the candidate from a
    /// shell on the fixture host.
    pub completion_script_execution: bool,
}

/// Executes candidate invocations built by conformance cases.
pub trait Runner: Send + Sync {
    /// Returns the candidate command represented by this adapter.
    fn command(&self) -> &CommandTemplate;

    /// Returns optional execution capabilities.
    fn capabilities(&self) -> RunnerCapabilities {
        RunnerCapabilities::default()
    }

    /// Executes one invocation and returns its captured output.
    ///
    /// # Errors
    ///
    /// Returns [`RunnerError`] when the candidate cannot be launched or the
    /// adapter cannot complete process I/O. A candidate closing piped stdin
    /// before consuming every byte is normal completion, not an I/O failure.
    /// Adapters must honor [`Invocation::capture_limit`] while continuing to
    /// drain stdout and stderr.
    fn run(&self, invocation: &Invocation) -> Result<InvocationResult, RunnerError>;
}

/// Runs a candidate as a child process on the local host.
#[derive(Debug, Clone)]
pub struct LocalProcessRunner {
    command: CommandTemplate,
}

impl LocalProcessRunner {
    /// Creates a local runner for a candidate command.
    pub fn new(command: CommandTemplate) -> Self {
        Self { command }
    }
}

impl Runner for LocalProcessRunner {
    fn command(&self) -> &CommandTemplate {
        &self.command
    }

    fn capabilities(&self) -> RunnerCapabilities {
        RunnerCapabilities {
            terminal_input: false,
            completion_script_execution: true,
        }
    }

    fn run(&self, invocation: &Invocation) -> Result<InvocationResult, RunnerError> {
        if matches!(invocation.stdin, StdinMode::Terminal(_)) {
            return Err(RunnerError::UnsupportedCapability {
                capability: "terminal input",
            });
        }

        let mut command = Command::new(self.command.program());
        command
            .args(self.command.prefix_args())
            .args(&invocation.args)
            .stdin(if matches!(invocation.stdin, StdinMode::Piped(_)) {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        command.process_group(0);
        if let Some(current_dir) = &invocation.current_dir {
            command.current_dir(current_dir);
        }
        for change in &invocation.environment {
            match &change.value {
                Some(value) => {
                    command.env(&change.name, value);
                }
                None => {
                    command.env_remove(&change.name);
                }
            }
        }

        let started = Instant::now();
        let mut child = spawn_child(command).map_err(|source| RunnerError::Launch {
            program: self.command.program().to_string_lossy().into_owned(),
            source,
        })?;
        let stdout =
            take_stdout(&mut child).ok_or(RunnerError::MissingPipe { stream: "stdout" })?;
        let stderr =
            take_stderr(&mut child).ok_or(RunnerError::MissingPipe { stream: "stderr" })?;
        let stdout_reader = read_pipe(stdout, invocation.capture_limit);
        let stderr_reader = read_pipe(stderr, invocation.capture_limit);

        let stdin_writer = if let StdinMode::Piped(input) = &invocation.stdin {
            let mut stdin =
                take_stdin(&mut child).ok_or(RunnerError::MissingPipe { stream: "stdin" })?;
            let input = input.clone();
            Some(thread::spawn(move || stdin.write_all(&input)))
        } else {
            None
        };

        let lifecycle = wait_for_invocation(
            &mut child,
            &stdout_reader,
            &stderr_reader,
            stdin_writer.as_ref(),
            invocation
                .timeout
                .and_then(|timeout| started.checked_add(timeout)),
        )?;
        let (termination, stdout, stderr) = match lifecycle {
            InvocationLifecycle::Completed(status) => {
                let stdout = stdout_reader.finish("stdout");
                let stderr = stderr_reader.finish("stderr");
                if let Some(stdin_writer) = stdin_writer {
                    join_stdin(stdin_writer)?;
                }
                (termination_from_status(status), stdout?, stderr?)
            }
            InvocationLifecycle::TimedOut => {
                let stdout = stdout_reader.finish_after_timeout("stdout");
                let stderr = stderr_reader.finish_after_timeout("stderr");
                (Termination::TimedOut, stdout?, stderr?)
            }
        };

        Ok(InvocationResult::new(
            termination,
            stdout,
            stderr,
            started.elapsed(),
        ))
    }
}

#[cfg(unix)]
fn spawn_child(mut command: Command) -> std::io::Result<ManagedChild> {
    command.spawn()
}

#[cfg(windows)]
fn spawn_child(command: Command) -> std::io::Result<ManagedChild> {
    let mut command = CommandWrap::from(command);
    command.wrap(JobObject);
    command.spawn()
}

#[cfg(unix)]
fn take_stdin(child: &mut ManagedChild) -> Option<ChildStdin> {
    child.stdin.take()
}

#[cfg(windows)]
fn take_stdin(child: &mut ManagedChild) -> Option<ChildStdin> {
    child.stdin().take()
}

#[cfg(unix)]
fn take_stdout(child: &mut ManagedChild) -> Option<ChildStdout> {
    child.stdout.take()
}

#[cfg(windows)]
fn take_stdout(child: &mut ManagedChild) -> Option<ChildStdout> {
    child.stdout().take()
}

#[cfg(unix)]
fn take_stderr(child: &mut ManagedChild) -> Option<ChildStderr> {
    child.stderr.take()
}

#[cfg(windows)]
fn take_stderr(child: &mut ManagedChild) -> Option<ChildStderr> {
    child.stderr().take()
}

fn join_stdin(handle: thread::JoinHandle<std::io::Result<()>>) -> Result<(), RunnerError> {
    let result = handle.join().map_err(|_| RunnerError::WriterPanicked)?;
    match result {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        Err(source) => Err(RunnerError::WriteStdin { source }),
    }
}

struct PipeCapture {
    output: Arc<Mutex<CapturedOutput>>,
    handle: thread::JoinHandle<std::io::Result<()>>,
    capture_limit: Option<usize>,
}

#[derive(Clone, Default)]
struct CapturedOutput {
    bytes: Vec<u8>,
    limit_exceeded: bool,
}

impl PipeCapture {
    fn is_finished(&self) -> bool {
        self.handle.is_finished()
    }

    fn finish(self, stream: &'static str) -> Result<Vec<u8>, RunnerError> {
        let Self {
            output,
            handle,
            capture_limit,
        } = self;
        handle
            .join()
            .map_err(|_| RunnerError::ReaderPanicked { stream })?
            .map_err(|source| RunnerError::ReadPipe { stream, source })?;
        captured_result(snapshot_output(&output), stream, capture_limit)
    }

    fn finish_after_timeout(self, stream: &'static str) -> Result<Vec<u8>, RunnerError> {
        let drain_deadline = Instant::now() + Duration::from_millis(100);
        while !self.handle.is_finished() && Instant::now() < drain_deadline {
            thread::sleep(Duration::from_millis(1));
        }
        if self.handle.is_finished() {
            let Self {
                output,
                handle,
                capture_limit,
            } = self;
            handle
                .join()
                .map_err(|_| RunnerError::ReaderPanicked { stream })?
                .map_err(|source| RunnerError::ReadPipe { stream, source })?;
            return captured_result(snapshot_output(&output), stream, capture_limit);
        }
        let output = self.snapshot();
        captured_result(output, stream, self.capture_limit)
    }

    fn snapshot(&self) -> CapturedOutput {
        self.output
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

fn captured_result(
    output: CapturedOutput,
    stream: &'static str,
    capture_limit: Option<usize>,
) -> Result<Vec<u8>, RunnerError> {
    if output.limit_exceeded {
        return Err(RunnerError::OutputLimitExceeded {
            stream,
            limit: capture_limit.unwrap_or(output.bytes.len()),
        });
    }
    Ok(output.bytes)
}

fn snapshot_output(output: &Mutex<CapturedOutput>) -> CapturedOutput {
    output
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .to_owned()
}

fn read_pipe(mut pipe: impl Read + Send + 'static, capture_limit: Option<usize>) -> PipeCapture {
    let output = Arc::new(Mutex::new(CapturedOutput::default()));
    let writer = Arc::clone(&output);
    let handle = thread::spawn(move || {
        let mut buffer = [0_u8; 8192];
        loop {
            let count = pipe.read(&mut buffer)?;
            if count == 0 {
                return Ok(());
            }
            let mut output = writer
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let retained = capture_limit.map_or(count, |limit| {
                let remaining = limit.saturating_sub(output.bytes.len());
                if count > remaining {
                    output.limit_exceeded = true;
                }
                count.min(remaining)
            });
            output.bytes.extend_from_slice(&buffer[..retained]);
        }
    });
    PipeCapture {
        output,
        handle,
        capture_limit,
    }
}

enum InvocationLifecycle {
    Completed(ExitStatus),
    TimedOut,
}

fn wait_for_invocation(
    child: &mut ManagedChild,
    stdout: &PipeCapture,
    stderr: &PipeCapture,
    stdin: Option<&thread::JoinHandle<std::io::Result<()>>>,
    deadline: Option<Instant>,
) -> Result<InvocationLifecycle, RunnerError> {
    let mut status = None;
    loop {
        if status.is_none() {
            status = child
                .try_wait()
                .map_err(|source| RunnerError::Wait { source })?;
        }
        let stdin_finished = stdin.is_none_or(thread::JoinHandle::is_finished);
        if let Some(status) = status
            && stdout.is_finished()
            && stderr.is_finished()
            && stdin_finished
        {
            return Ok(InvocationLifecycle::Completed(status));
        }
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            terminate_child_tree(child, status.is_some())?;
            if status.is_none() {
                child
                    .wait()
                    .map_err(|source| RunnerError::Wait { source })?;
            }
            return Ok(InvocationLifecycle::TimedOut);
        }
        thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(unix)]
fn terminate_child_tree(child: &mut ManagedChild, leader_exited: bool) -> Result<(), RunnerError> {
    use nix::errno::Errno;
    use nix::sys::signal::{Signal, killpg};
    use nix::unistd::Pid;

    match killpg(Pid::from_raw(child.id() as i32), Signal::SIGKILL) {
        Ok(()) => Ok(()),
        Err(Errno::ESRCH) if leader_exited => Ok(()),
        Err(error) => child.kill().map_err(|source| RunnerError::Kill {
            source: if source.kind() == std::io::ErrorKind::InvalidInput {
                std::io::Error::from_raw_os_error(error as i32)
            } else {
                source
            },
        }),
    }
}

#[cfg(windows)]
fn terminate_child_tree(child: &mut ManagedChild, _leader_exited: bool) -> Result<(), RunnerError> {
    child
        .start_kill()
        .map_err(|source| RunnerError::Kill { source })
}

fn termination_from_status(status: ExitStatus) -> Termination {
    status
        .code()
        .map_or(Termination::Signaled, |code| Termination::Exited { code })
}

/// Failure returned by a runner adapter.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RunnerError {
    /// Candidate launch failed.
    #[error("failed to launch candidate program {program}: {source}")]
    Launch {
        /// Lossy program representation used in the diagnostic.
        program: String,
        /// Operating-system launch failure.
        #[source]
        source: std::io::Error,
    },
    /// A requested runner capability is unavailable.
    #[error("runner does not support {capability}")]
    UnsupportedCapability {
        /// Capability required by the invocation.
        capability: &'static str,
    },
    /// A configured process pipe was unavailable.
    #[error("candidate {stream} pipe was unavailable")]
    MissingPipe {
        /// Missing stream name.
        stream: &'static str,
    },
    /// Writing candidate input failed.
    #[error("failed to write candidate stdin: {source}")]
    WriteStdin {
        /// Input write failure.
        #[source]
        source: std::io::Error,
    },
    /// Waiting for the candidate failed.
    #[error("failed while waiting for candidate: {source}")]
    Wait {
        /// Process wait failure.
        #[source]
        source: std::io::Error,
    },
    /// Killing a timed-out candidate failed.
    #[error("failed to terminate timed-out candidate: {source}")]
    Kill {
        /// Process termination failure.
        #[source]
        source: std::io::Error,
    },
    /// Reading captured output failed.
    #[error("failed to read candidate {stream}: {source}")]
    ReadPipe {
        /// Stream name.
        stream: &'static str,
        /// Output read failure.
        #[source]
        source: std::io::Error,
    },
    /// Captured output exceeded an invocation's configured per-stream limit.
    #[error("candidate {stream} exceeded the {limit}-byte output capture limit")]
    OutputLimitExceeded {
        /// Stream that exceeded the limit.
        stream: &'static str,
        /// Maximum retained bytes for that stream.
        limit: usize,
    },
    /// A background output reader panicked.
    #[error("candidate {stream} reader panicked")]
    ReaderPanicked {
        /// Stream name.
        stream: &'static str,
    },
    /// The background input writer panicked.
    #[error("candidate stdin writer panicked")]
    WriterPanicked,
}

#[cfg(test)]
mod tests {
    use std::io::{self, Read};

    use super::{RunnerError, read_pipe};

    struct FailingReader;

    impl Read for FailingReader {
        fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("injected pipe read failure"))
        }
    }

    #[test]
    fn timed_out_capture_should_preserve_completed_pipe_read_failure() {
        let error = read_pipe(FailingReader, None)
            .finish_after_timeout("stdout")
            .expect_err("a completed pipe read failure should not become a timeout result");

        assert!(matches!(
            error,
            RunnerError::ReadPipe {
                stream: "stdout",
                source,
            } if source.kind() == io::ErrorKind::Other
        ));
    }
}
