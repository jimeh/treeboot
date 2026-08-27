use std::ffi::{OsStr, OsString};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use serde::Serialize;
use thiserror::Error;

use crate::CommandTemplate;

/// Input supplied to one candidate invocation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum StdinMode {
    /// Connects the child to an empty input stream.
    #[default]
    Empty,
    /// Writes the provided bytes to a pipe connected to the child.
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

    /// Executes one invocation and returns all captured output.
    ///
    /// # Errors
    ///
    /// Returns [`RunnerError`] when the candidate cannot be launched or the
    /// adapter cannot complete process I/O.
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
        let mut child = command.spawn().map_err(|source| RunnerError::Launch {
            program: self.command.program().to_string_lossy().into_owned(),
            source,
        })?;
        let stdout = child
            .stdout
            .take()
            .ok_or(RunnerError::MissingPipe { stream: "stdout" })?;
        let stderr = child
            .stderr
            .take()
            .ok_or(RunnerError::MissingPipe { stream: "stderr" })?;
        let stdout_reader = read_pipe(stdout);
        let stderr_reader = read_pipe(stderr);

        let stdin_writer = if let StdinMode::Piped(input) = &invocation.stdin {
            let mut stdin = child
                .stdin
                .take()
                .ok_or(RunnerError::MissingPipe { stream: "stdin" })?;
            let input = input.clone();
            Some(thread::spawn(move || stdin.write_all(&input)))
        } else {
            None
        };

        let (status, timed_out) = wait_for_child(&mut child, invocation.timeout)?;
        let stdout = join_pipe(stdout_reader, "stdout")?;
        let stderr = join_pipe(stderr_reader, "stderr")?;
        if !timed_out && let Some(stdin_writer) = stdin_writer {
            join_stdin(stdin_writer)?;
        }
        let termination = if timed_out {
            Termination::TimedOut
        } else {
            termination_from_status(status)
        };

        Ok(InvocationResult::new(
            termination,
            stdout,
            stderr,
            started.elapsed(),
        ))
    }
}

fn join_stdin(handle: thread::JoinHandle<std::io::Result<()>>) -> Result<(), RunnerError> {
    handle
        .join()
        .map_err(|_| RunnerError::WriterPanicked)?
        .map_err(|source| RunnerError::WriteStdin { source })
}

fn read_pipe(mut pipe: impl Read + Send + 'static) -> thread::JoinHandle<std::io::Result<Vec<u8>>> {
    thread::spawn(move || {
        let mut output = Vec::new();
        pipe.read_to_end(&mut output)?;
        Ok(output)
    })
}

fn join_pipe(
    handle: thread::JoinHandle<std::io::Result<Vec<u8>>>,
    stream: &'static str,
) -> Result<Vec<u8>, RunnerError> {
    handle
        .join()
        .map_err(|_| RunnerError::ReaderPanicked { stream })?
        .map_err(|source| RunnerError::ReadPipe { stream, source })
}

fn wait_for_child(
    child: &mut Child,
    timeout: Option<Duration>,
) -> Result<(ExitStatus, bool), RunnerError> {
    let started = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|source| RunnerError::Wait { source })?
        {
            return Ok((status, false));
        }
        if timeout.is_some_and(|timeout| started.elapsed() >= timeout) {
            terminate_child_tree(child)?;
            let status = child
                .wait()
                .map_err(|source| RunnerError::Wait { source })?;
            return Ok((status, true));
        }
        thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(unix)]
fn terminate_child_tree(child: &mut Child) -> Result<(), RunnerError> {
    let group = format!("-{}", child.id());
    let group_killed = Command::new("kill")
        .args(["-s", "KILL", "--", &group])
        .status()
        .is_ok_and(|status| status.success());
    if group_killed {
        return Ok(());
    }

    child.kill().map_err(|source| RunnerError::Kill { source })
}

#[cfg(windows)]
fn terminate_child_tree(child: &mut Child) -> Result<(), RunnerError> {
    let pid = child.id().to_string();
    let tree_killed = Command::new("taskkill")
        .args(["/PID", &pid, "/T", "/F"])
        .status()
        .is_ok_and(|status| status.success());
    if tree_killed {
        return Ok(());
    }

    child.kill().map_err(|source| RunnerError::Kill { source })
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
