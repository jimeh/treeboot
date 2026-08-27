use std::cell::RefCell;
use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Output};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Once};
use std::time::Duration;

use assert_cmd::assert::Assert;
use serde_json::Value;
use tempfile::TempDir;

use crate::{
    CommandTemplate, Invocation, InvocationResult, LocalProcessRunner, Runner, RunnerCapabilities,
    StdinMode, Termination,
};

const CLEAN_ENVIRONMENT: &[&str] = &[
    "TREEBOOT_ROOT_PATH",
    "CODEX_SOURCE_TREE_PATH",
    "CONDUCTOR_ROOT_PATH",
    "SUPERSET_ROOT_PATH",
    "CONDUCTOR_DEFAULT_BRANCH",
    "TREEBOOT_STRICT",
    "TREEBOOT_DANGEROUSLY_ALLOW_SOURCES_OUTSIDE_ROOT",
    "TREEBOOT_DANGEROUSLY_ALLOW_TARGETS_OUTSIDE_WORKTREE",
];

thread_local! {
    static CASE_CONTEXT: RefCell<Option<Arc<CaseContext>>> = const { RefCell::new(None) };
}

static INSTALL_PANIC_HOOK: Once = Once::new();

pub(crate) fn install_case_panic_hook() {
    INSTALL_PANIC_HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |panic| {
            let is_captured_case = CASE_CONTEXT
                .try_with(|slot| slot.borrow().is_some())
                .unwrap_or(false);
            if !is_captured_case {
                previous(panic);
            }
        }));
    });
}

pub(crate) struct CaseContext {
    runner: Arc<dyn Runner>,
    timeout: Duration,
    failure: Mutex<Option<ExecutionFailure>>,
    candidate_invoked: AtomicBool,
}

#[derive(Debug, Clone)]
pub(crate) enum ExecutionFailure {
    Skipped(String),
    Runner(String),
    TimedOut(String),
}

impl CaseContext {
    pub(crate) fn new(runner: Arc<dyn Runner>, timeout: Duration) -> Self {
        Self {
            runner,
            timeout,
            failure: Mutex::new(None),
            candidate_invoked: AtomicBool::new(false),
        }
    }

    pub(crate) fn take_failure(&self) -> Option<ExecutionFailure> {
        self.failure
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    }

    fn record(&self, failure: ExecutionFailure) {
        *self
            .failure
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(failure);
    }

    pub(crate) fn candidate_invoked(&self) -> bool {
        self.candidate_invoked.load(Ordering::Relaxed)
    }

    fn mark_candidate_invoked(&self) {
        self.candidate_invoked.store(true, Ordering::Relaxed);
    }
}

pub(crate) fn with_context<T>(context: Arc<CaseContext>, run: impl FnOnce() -> T) -> T {
    CASE_CONTEXT.with(|slot| {
        let previous = slot.replace(Some(context));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(run));
        slot.replace(previous);
        match result {
            Ok(value) => value,
            Err(payload) => std::panic::resume_unwind(payload),
        }
    })
}

fn context() -> Arc<CaseContext> {
    CASE_CONTEXT.with(|slot| {
        slot.borrow()
            .as_ref()
            .cloned()
            .expect("conformance case must run through Suite")
    })
}

pub(crate) fn skip(reason: impl Into<String>) -> ! {
    let reason = reason.into();
    context().record(ExecutionFailure::Skipped(reason.clone()));
    panic!("case skipped: {reason}");
}

pub(crate) struct Command {
    invocation: Invocation,
}

impl Command {
    pub(crate) fn arg(&mut self, argument: impl AsRef<OsStr>) -> &mut Self {
        self.invocation =
            std::mem::take(&mut self.invocation).arg(argument.as_ref().to_os_string());
        self
    }

    pub(crate) fn args<I, S>(&mut self, arguments: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.invocation = std::mem::take(&mut self.invocation).args(
            arguments
                .into_iter()
                .map(|value| value.as_ref().to_os_string()),
        );
        self
    }

    pub(crate) fn current_dir(&mut self, path: impl AsRef<Path>) -> &mut Self {
        self.invocation =
            std::mem::take(&mut self.invocation).current_dir(path.as_ref().to_path_buf());
        self
    }

    pub(crate) fn env(&mut self, name: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> &mut Self {
        self.invocation = std::mem::take(&mut self.invocation)
            .env(name.as_ref().to_os_string(), value.as_ref().to_os_string());
        self
    }

    pub(crate) fn write_terminal(&mut self, input: impl Into<Vec<u8>>) -> &mut Self {
        self.invocation =
            std::mem::take(&mut self.invocation).stdin(StdinMode::Terminal(input.into()));
        self
    }

    #[track_caller]
    pub(crate) fn assert(&mut self) -> Assert {
        let output = self.execute().unwrap_or_else(|error| panic!("{error}"));
        Assert::new(output)
    }

    pub(crate) fn output(&mut self) -> io::Result<Output> {
        self.execute().map_err(io::Error::other)
    }

    fn execute(&mut self) -> Result<Output, String> {
        let context = context();
        if self.invocation.timeout_value().is_none() {
            self.invocation = std::mem::take(&mut self.invocation).timeout(context.timeout);
        }
        context.mark_candidate_invoked();
        let result = context.runner.run(&self.invocation).map_err(|error| {
            let message = error.to_string();
            match error {
                crate::RunnerError::UnsupportedCapability { .. } => {
                    context.record(ExecutionFailure::Skipped(message.clone()));
                }
                _ => context.record(ExecutionFailure::Runner(message.clone())),
            }
            message
        })?;
        result_to_output(&context, result, "candidate")
    }
}

fn result_to_output(
    context: &CaseContext,
    result: InvocationResult,
    process: &str,
) -> Result<Output, String> {
    if result.termination() == Termination::TimedOut {
        let message = format!(
            "{process} timed out after {} ms; stdout: {}; stderr: {}",
            result.duration().as_millis(),
            String::from_utf8_lossy(result.stdout()),
            String::from_utf8_lossy(result.stderr())
        );
        context.record(ExecutionFailure::TimedOut(message.clone()));
        return Err(message);
    }

    Ok(Output {
        status: native_status(result.termination()),
        stdout: result.stdout().to_vec(),
        stderr: result.stderr().to_vec(),
    })
}

pub(crate) struct HostProcess {
    runner: LocalProcessRunner,
    invocation: Invocation,
}

impl HostProcess {
    pub(crate) fn arg(&mut self, argument: impl AsRef<OsStr>) -> &mut Self {
        self.invocation =
            std::mem::take(&mut self.invocation).arg(argument.as_ref().to_os_string());
        self
    }

    pub(crate) fn args<I, S>(&mut self, arguments: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.invocation = std::mem::take(&mut self.invocation).args(
            arguments
                .into_iter()
                .map(|value| value.as_ref().to_os_string()),
        );
        self
    }

    pub(crate) fn current_dir(&mut self, path: impl AsRef<Path>) -> &mut Self {
        self.invocation =
            std::mem::take(&mut self.invocation).current_dir(path.as_ref().to_path_buf());
        self
    }

    pub(crate) fn env(&mut self, name: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> &mut Self {
        self.invocation = std::mem::take(&mut self.invocation)
            .env(name.as_ref().to_os_string(), value.as_ref().to_os_string());
        self
    }

    pub(crate) fn output(&mut self) -> io::Result<Output> {
        let context = context();
        if self.invocation.timeout_value().is_none() {
            self.invocation = std::mem::take(&mut self.invocation).timeout(context.timeout);
        }
        let result = self.runner.run(&self.invocation).map_err(|error| {
            let message = format!("host subprocess failed: {error}");
            context.record(ExecutionFailure::Runner(message.clone()));
            io::Error::other(message)
        })?;
        result_to_output(&context, result, "host subprocess").map_err(io::Error::other)
    }
}

pub(crate) fn host_process(program: impl AsRef<OsStr>) -> HostProcess {
    let mut invocation = Invocation::new();
    for name in CLEAN_ENVIRONMENT {
        invocation = invocation.env_remove(*name);
    }
    HostProcess {
        runner: LocalProcessRunner::new(CommandTemplate::new(program.as_ref().to_os_string())),
        invocation,
    }
}

#[cfg(unix)]
fn native_status(termination: Termination) -> ExitStatus {
    use std::os::unix::process::ExitStatusExt;

    match termination {
        Termination::Exited { code } => ExitStatus::from_raw(code << 8),
        Termination::Signaled | Termination::TimedOut => ExitStatus::from_raw(9),
    }
}

#[cfg(windows)]
fn native_status(termination: Termination) -> ExitStatus {
    use std::os::windows::process::ExitStatusExt;

    match termination {
        Termination::Exited { code } => ExitStatus::from_raw(code as u32),
        Termination::Signaled | Termination::TimedOut => ExitStatus::from_raw(u32::MAX),
    }
}

pub(crate) fn treeboot() -> Command {
    let mut invocation = Invocation::new();
    for name in CLEAN_ENVIRONMENT {
        invocation = invocation.env_remove(*name);
    }
    Command { invocation }
}

pub(crate) fn runner_capabilities() -> RunnerCapabilities {
    context().runner.capabilities()
}

pub(crate) fn canonical_path(path: &Path) -> PathBuf {
    dunce::canonicalize(path).expect("path should canonicalize")
}

pub(crate) fn display_path(path: &str) -> String {
    path.split('/').collect::<PathBuf>().display().to_string()
}

pub(crate) fn toml_string_path(path: &Path) -> String {
    toml_string(&path.display().to_string())
}

pub(crate) fn toml_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

pub(crate) fn git(args: &[&str], cwd: &Path) {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("git should run");

    assert!(
        output.status.success(),
        "git {args:?} should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

pub(crate) fn git_repo() -> TempDir {
    let repo = TempDir::new().expect("tempdir should be created");
    git(&["init"], repo.path());
    repo
}

pub(crate) struct GitWorktree {
    root: TempDir,
    _worktree_parent: TempDir,
    worktree_path: PathBuf,
}

impl GitWorktree {
    pub(crate) fn root_path(&self) -> &Path {
        self.root.path()
    }

    pub(crate) fn worktree_path(&self) -> &Path {
        &self.worktree_path
    }
}

pub(crate) fn git_worktree() -> GitWorktree {
    let root = git_repo();
    git(&["config", "user.name", "treeboot"], root.path());
    git(
        &["config", "user.email", "treeboot@example.invalid"],
        root.path(),
    );
    git(&["config", "commit.gpgsign", "false"], root.path());
    write_file(&root.path().join("README.md"), "treeboot test repo\n");
    git(&["add", "README.md"], root.path());
    git(&["commit", "-m", "Initial commit"], root.path());

    let worktree_parent = TempDir::new().expect("worktree parent should be created");
    let worktree_path = worktree_parent.path().join("linked");
    let worktree = worktree_path
        .to_str()
        .expect("worktree path should be valid UTF-8");
    git(
        &["worktree", "add", "-b", "treeboot-test-worktree", worktree],
        root.path(),
    );

    GitWorktree {
        root,
        _worktree_parent: worktree_parent,
        worktree_path,
    }
}

pub(crate) fn write_file(path: &Path, content: &str) {
    std::fs::write(path, content).expect("file should be written");
}

#[cfg(unix)]
pub(crate) fn symlink_file(source: impl AsRef<Path>, target: impl AsRef<Path>) {
    std::os::unix::fs::symlink(source.as_ref(), target.as_ref())
        .expect("file symlink should be created");
}

#[cfg(windows)]
pub(crate) fn symlink_file(source: impl AsRef<Path>, target: impl AsRef<Path>) {
    match std::os::windows::fs::symlink_file(source.as_ref(), target.as_ref()) {
        Ok(()) => {}
        Err(error) if error.raw_os_error() == Some(1314) => skip(
            "requires Windows symbolic-link privilege or Developer Mode to create fixture links",
        ),
        Err(error) => panic!("file symlink should be created: {error}"),
    }
}

#[cfg(unix)]
pub(crate) fn symlink_dir(source: impl AsRef<Path>, target: impl AsRef<Path>) {
    std::os::unix::fs::symlink(source.as_ref(), target.as_ref())
        .expect("directory symlink should be created");
}

#[cfg(windows)]
pub(crate) fn symlink_dir(source: impl AsRef<Path>, target: impl AsRef<Path>) {
    match std::os::windows::fs::symlink_dir(source.as_ref(), target.as_ref()) {
        Ok(()) => {}
        Err(error) if error.raw_os_error() == Some(1314) => skip(
            "requires Windows symbolic-link privilege or Developer Mode to create fixture links",
        ),
        Err(error) => panic!("directory symlink should be created: {error}"),
    }
}

pub(crate) fn parse_json(stdout: Vec<u8>, context: &str) -> Value {
    serde_json::from_slice(&stdout).unwrap_or_else(|error| {
        panic!("{context} JSON should parse: {error}");
    })
}

pub(crate) fn assert_json_object_keys(value: &Value, expected: &[&str]) {
    let object = value.as_object().expect("value should be a JSON object");
    let mut actual = object.keys().map(String::as_str).collect::<Vec<_>>();
    actual.sort_unstable();

    let mut expected = expected.to_vec();
    expected.sort_unstable();

    assert_eq!(actual, expected);
}

pub(crate) fn assert_context_shape(value: &Value) {
    assert_json_object_keys(value, &["default_branch", "root_path", "worktree_path"]);
    assert!(value["root_path"].is_string());
    assert!(value["worktree_path"].is_string());
    assert!(value["default_branch"].is_string());
}

#[cfg(unix)]
pub(crate) fn write_executable_script(path: &Path, content: &str) {
    use std::os::unix::fs::PermissionsExt;

    write_file(path, content);
    let mut permissions = path
        .metadata()
        .expect("script metadata should be readable")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions).expect("script permissions should be set");
}

pub(crate) fn candidate_package_version() -> String {
    let output = treeboot()
        .args(["version", "--json"])
        .output()
        .expect("candidate version query should run");
    let value = parse_json(output.stdout, "candidate version");
    value["version"]
        .as_str()
        .expect("candidate version should be a string")
        .to_owned()
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    #[test]
    fn blocking_host_subprocess_should_record_timeout_without_candidate_invocation() {
        let context = Arc::new(CaseContext::new(
            Arc::new(LocalProcessRunner::new(CommandTemplate::new(
                "unused-candidate",
            ))),
            Duration::from_millis(100),
        ));
        let started = Instant::now();

        with_context(Arc::clone(&context), || {
            host_process(std::env::current_exe().expect("test executable should resolve"))
                .args([
                    "cases::support::tests::blocking_host_subprocess_helper",
                    "--exact",
                    "--ignored",
                ])
                .output()
                .expect_err("blocking host subprocess should time out");
        });

        assert!(matches!(
            context.take_failure(),
            Some(ExecutionFailure::TimedOut(_))
        ));
        assert!(!context.candidate_invoked());
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    #[ignore = "helper process for the bounded host-subprocess regression"]
    fn blocking_host_subprocess_helper() {
        std::thread::sleep(Duration::from_secs(30));
    }
}
