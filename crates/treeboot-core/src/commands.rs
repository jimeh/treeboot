use std::io;
use std::path::PathBuf;
use std::process::{Command, ExitStatus};

#[cfg(test)]
use crate::validation::PlannedCommandParts;
use crate::{
    ActionPlan, CommandKind, Error, OutputEvent, PlannedCommand, Reporter, Result, Worktree, paths,
};

/// Options that affect command execution.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct CommandExecutionOptions {
    /// Prints planned commands without spawning processes.
    pub dry_run: bool,
}

pub(crate) fn execute_commands(
    plan: &ActionPlan,
    options: CommandExecutionOptions,
    reporter: &mut dyn Reporter,
) -> Result<()> {
    for command in plan.commands() {
        if options.dry_run {
            report(
                reporter,
                OutputEvent::CommandWouldRun {
                    label: command_label(command),
                },
            )?;
        } else {
            run_sequential(command, plan.context(), reporter)?;
        }
    }

    Ok(())
}

fn run_sequential(
    command: &PlannedCommand,
    context: &Worktree,
    reporter: &mut dyn Reporter,
) -> Result<()> {
    let label = command_label(command);
    report(
        reporter,
        OutputEvent::CommandStarted {
            label: label.clone(),
        },
    )?;

    let status = match build_command(command, context).and_then(|mut process| process.status()) {
        Ok(status) => status,
        Err(source) => {
            if command.allow_failure() {
                report_allowed_failure(reporter, label, format!("failed to start: {source}"))?;
                return Ok(());
            }

            return Err(Error::CommandIo { label, source });
        }
    };

    handle_exit_status(command.allow_failure(), label, status, reporter)
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

fn build_command(command: &PlannedCommand, context: &Worktree) -> io::Result<Command> {
    let cwd_path = resolve_command_cwd(command, context)?;
    let mut process = match command.command() {
        CommandKind::Shell { run } => build_shell_command(run),
        CommandKind::Direct { program, args } => {
            let mut process = Command::new(program);
            process.args(args);
            process
        }
    };

    process
        .current_dir(cwd_path)
        .envs(&context.environment)
        .envs(command.env());
    Ok(process)
}

fn resolve_command_cwd(command: &PlannedCommand, context: &Worktree) -> io::Result<PathBuf> {
    let worktree_path = paths::canonicalize(&context.worktree_path)?;
    let declared_cwd = command.cwd().unwrap_or(context.worktree_path.as_path());
    let resolved_cwd = paths::resolve_path(&context.worktree_path, declared_cwd)
        .map_err(|source| io::Error::new(io::ErrorKind::InvalidInput, source.reason()))?;
    let cwd_path = paths::canonicalize(&resolved_cwd)?;

    if !paths::is_within(&cwd_path, &worktree_path) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "command cwd resolves outside worktree",
        ));
    }

    Ok(cwd_path)
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
    let invocation = invocation_label(command.command());

    if let Some(name) = command.name() {
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::test_support::symlink_dir;
    use crate::{ActionPlan, SourceSpan};

    #[test]
    fn command_label_should_include_name_and_invocation() {
        let command = planned_command(
            Some("Install packages"),
            CommandKind::Shell {
                run: "npm install".to_owned(),
            },
        );

        assert_eq!(
            command_label(&command.planned()),
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

        assert_eq!(command_label(&command.planned()), "cargo test --locked");
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
    fn execute_commands_should_reject_live_cwd_symlink_escape_without_spawning() {
        let (temp, context) = context("cwd-symlink-escape");
        let outside = temp.path().join("root/shared");
        let cwd = temp.path().join("worktree/escape");
        let marker = outside.join("marker");
        std::fs::create_dir_all(&outside).expect("outside dir should be created");
        let command = planned_command(
            None,
            CommandKind::Direct {
                program: "sh".to_owned(),
                args: vec!["-c".to_owned(), format!("touch {}", shell_path(&marker))],
            },
        )
        .with_cwd(cwd.clone());
        let plan = plan(context, vec![command]);
        symlink_dir(&outside, &cwd).expect("cwd symlink should be created after planning");

        let error = execute_commands(
            &plan,
            CommandExecutionOptions::default(),
            &mut Recorder::default(),
        )
        .expect_err("live cwd escape should fail");

        assert!(!marker.exists());
        assert!(
            error
                .to_string()
                .contains("command cwd resolves outside worktree")
        );
    }

    #[cfg(unix)]
    #[test]
    fn execute_commands_should_allow_live_cwd_escape_failure_and_continue() {
        let (temp, context) = context("allowed-cwd-symlink-escape");
        let outside = temp.path().join("root/shared");
        let cwd = temp.path().join("worktree/escape");
        let escaped_marker = outside.join("escaped-marker");
        let later_marker = temp.path().join("worktree/later-marker");
        std::fs::create_dir_all(&outside).expect("outside dir should be created");
        let escaping = planned_command(
            Some("optional escape"),
            CommandKind::Shell {
                run: format!("touch {}", shell_path(&escaped_marker)),
            },
        )
        .with_cwd(cwd.clone())
        .with_allow_failure();
        let later = planned_command(
            Some("later"),
            CommandKind::Shell {
                run: format!("touch {}", shell_path(&later_marker)),
            },
        );
        let plan = plan(context, vec![escaping, later]);
        symlink_dir(&outside, &cwd).expect("cwd symlink should be created after planning");
        let mut reporter = Recorder::default();

        execute_commands(&plan, CommandExecutionOptions::default(), &mut reporter)
            .expect("allowed cwd failure should continue");

        assert!(!escaped_marker.exists());
        assert!(later_marker.exists());
        assert!(reporter.messages().iter().any(|message| {
            message.contains("failed to start: command cwd resolves outside worktree")
        }));
    }

    #[cfg(unix)]
    #[test]
    fn execute_commands_should_use_in_worktree_cwd_created_after_planning() {
        let (temp, context) = context("cwd-created-after-plan");
        let cwd = temp.path().join("worktree/generated");
        let marker = cwd.join("pwd-marker");
        let command = planned_command(
            None,
            CommandKind::Shell {
                run: format!("pwd > {}", shell_path(&marker)),
            },
        )
        .with_cwd(cwd.clone());
        let plan = plan(context, vec![command]);
        std::fs::create_dir_all(&cwd).expect("cwd should be created after planning");

        execute_commands(
            &plan,
            CommandExecutionOptions::default(),
            &mut Recorder::default(),
        )
        .expect("fresh in-worktree cwd should run");

        assert_eq!(
            std::fs::read_to_string(marker)
                .expect("pwd marker should be readable")
                .trim(),
            paths::canonicalize(&cwd)
                .expect("cwd should canonicalize")
                .display()
                .to_string()
        );
    }

    #[test]
    fn execute_commands_should_reject_cwd_symlink_retargeted_after_planning() {
        let (temp, context) = context("retargeted-cwd-symlink");
        let safe = temp.path().join("worktree/safe");
        let outside = temp.path().join("root/shared");
        let cwd = temp.path().join("worktree/escape");
        let marker = outside.join("marker");
        std::fs::create_dir_all(&safe).expect("safe dir should be created");
        std::fs::create_dir_all(&outside).expect("outside dir should be created");
        symlink_dir(&safe, &cwd).expect("safe cwd symlink should be created before planning");
        let command = planned_command(
            None,
            CommandKind::Shell {
                run: format!("echo ran > {}", shell_path(&marker)),
            },
        )
        .with_cwd(cwd.clone());
        let plan = plan(context, vec![command]);
        remove_dir_symlink(&cwd).expect("safe cwd symlink should be removed");
        symlink_dir(&outside, &cwd).expect("cwd symlink should be retargeted after planning");

        let error = execute_commands(
            &plan,
            CommandExecutionOptions::default(),
            &mut Recorder::default(),
        )
        .expect_err("retargeted cwd escape should fail");

        assert!(!marker.exists());
        assert!(
            error
                .to_string()
                .contains("command cwd resolves outside worktree")
        );
    }

    #[test]
    fn execute_commands_should_run_in_declared_in_worktree_symlink_cwd() {
        let (temp, context) = context("in-worktree-symlink-cwd");
        let real_cwd = temp.path().join("worktree/real-cwd");
        let declared_cwd = temp.path().join("worktree/linked-cwd");
        let marker = real_cwd.join("cwd-marker");
        std::fs::create_dir_all(&real_cwd).expect("real cwd should be created");
        symlink_dir(&real_cwd, &declared_cwd).expect("in-worktree cwd symlink should be created");
        let command = planned_command(
            None,
            CommandKind::Shell {
                run: "echo ran > cwd-marker".to_owned(),
            },
        )
        .with_cwd(declared_cwd);
        let plan = plan(context, vec![command]);

        execute_commands(
            &plan,
            CommandExecutionOptions::default(),
            &mut Recorder::default(),
        )
        .expect("in-worktree symlink cwd should run");

        assert!(
            std::fs::read_to_string(marker)
                .expect("cwd marker should be readable")
                .contains("ran")
        );
    }

    #[test]
    fn execute_commands_should_treat_dangling_cwd_symlink_as_start_failure() {
        let (temp, context) = context("dangling-cwd-symlink");
        let cwd = temp.path().join("worktree/dangling");
        let marker = temp.path().join("worktree/marker");
        symlink_dir(temp.path().join("worktree/missing"), &cwd)
            .expect("dangling cwd symlink should be created");
        let command = planned_command(
            None,
            CommandKind::Shell {
                run: format!("echo ran > {}", shell_path(&marker)),
            },
        )
        .with_cwd(cwd);
        let plan = plan(context, vec![command]);

        let resolver_error = resolve_command_cwd(&plan.commands()[0], plan.context())
            .expect_err("resolver should reject dangling cwd");
        assert_eq!(resolver_error.kind(), io::ErrorKind::NotFound);

        let error = execute_commands(
            &plan,
            CommandExecutionOptions::default(),
            &mut Recorder::default(),
        )
        .expect_err("dangling cwd should fail to start");

        assert!(!marker.exists());
        assert!(matches!(error, Error::CommandIo { .. }));
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
    fn execute_commands_should_report_each_dry_run_command_without_spawning() {
        let (temp, context) = context("dry-run-sequential");
        let first_marker = temp.path().join("worktree/first");
        let second_marker = temp.path().join("worktree/second");
        let first = planned_command(
            Some("first"),
            CommandKind::Shell {
                run: format!("touch {}", shell_path(&first_marker)),
            },
        );
        let second = planned_command(
            Some("second"),
            CommandKind::Shell {
                run: format!("touch {}", shell_path(&second_marker)),
            },
        );
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
    fn execute_commands_should_run_commands_in_declaration_order() {
        let (temp, context) = context("sequential-order");
        let marker = temp.path().join("worktree/order");
        let first = planned_command(
            Some("first"),
            CommandKind::Shell {
                run: format!("printf 'a' >> {}", shell_path(&marker)),
            },
        );
        let second = planned_command(
            Some("second"),
            CommandKind::Shell {
                run: format!("printf 'b' >> {}", shell_path(&marker)),
            },
        );
        let plan = plan(context, vec![first, second]);

        execute_commands(
            &plan,
            CommandExecutionOptions::default(),
            &mut Recorder::default(),
        )
        .expect("commands should run");

        assert_eq!(
            std::fs::read_to_string(marker).expect("marker should be readable"),
            "ab"
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
    fn execute_commands_should_stop_after_fatal_spawn_failure() {
        let (temp, context) = context("fatal-spawn");
        let marker = temp.path().join("worktree/marker");
        let missing = planned_command(
            Some("missing"),
            CommandKind::Direct {
                program: "treeboot-missing-program-for-test".to_owned(),
                args: Vec::new(),
            },
        );
        let later = planned_command(
            Some("later"),
            CommandKind::Shell {
                run: format!("touch {}", shell_path(&marker)),
            },
        );
        let plan = plan(context, vec![missing, later]);

        let error = execute_commands(
            &plan,
            CommandExecutionOptions::default(),
            &mut Recorder::default(),
        )
        .expect_err("spawn failure should fail");

        assert!(!marker.exists());
        assert!(error.to_string().contains("failed to run command missing:"));
    }

    #[test]
    fn execute_commands_should_fail_when_dry_run_reporting_fails() {
        let (_temp, context) = context("dry-run-report-error");
        let command = planned_command(
            None,
            CommandKind::Direct {
                program: "echo".to_owned(),
                args: vec!["planned".to_owned()],
            },
        );
        let plan = plan(context, vec![command]);

        let error = execute_commands(
            &plan,
            CommandExecutionOptions { dry_run: true },
            &mut FailingReporter,
        )
        .expect_err("report failure should propagate");

        assert!(matches!(error, Error::Output { .. }));
    }

    #[test]
    fn execute_commands_should_fail_when_start_reporting_fails() {
        let (_temp, context) = context("start-report-error");
        let command = planned_command(
            None,
            CommandKind::Direct {
                program: "echo".to_owned(),
                args: vec!["running".to_owned()],
            },
        );
        let plan = plan(context, vec![command]);

        let error = execute_commands(
            &plan,
            CommandExecutionOptions::default(),
            &mut FailingReporter,
        )
        .expect_err("report failure should propagate");

        assert!(matches!(error, Error::Output { .. }));
    }

    struct TestCommand {
        parts: PlannedCommandParts,
    }

    impl TestCommand {
        fn planned(&self) -> PlannedCommand {
            PlannedCommand::from_raw_parts_unchecked(self.parts.clone())
        }

        #[cfg(unix)]
        fn with_allow_failure(mut self) -> Self {
            self.parts.allow_failure = true;
            self
        }

        fn with_cwd(mut self, cwd: PathBuf) -> Self {
            self.parts.cwd = Some(cwd.clone());
            self.parts.cwd_path =
                paths::normalize_maybe_existing(&cwd).expect("test command cwd should normalize");
            self
        }

        #[cfg(unix)]
        fn with_env(mut self, key: &str, value: &str) -> Self {
            self.parts.env.insert(key.to_owned(), value.to_owned());
            self
        }
    }

    impl From<TestCommand> for PlannedCommand {
        fn from(command: TestCommand) -> Self {
            PlannedCommand::from_raw_parts_unchecked(command.parts)
        }
    }

    fn planned_command(name: Option<&str>, command: CommandKind) -> TestCommand {
        TestCommand {
            parts: PlannedCommandParts {
                name: name.map(str::to_owned),
                command,
                cwd: None,
                cwd_path: PathBuf::new(),
                env: BTreeMap::new(),
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

    fn plan(context: Worktree, commands: Vec<TestCommand>) -> ActionPlan {
        let commands = commands
            .into_iter()
            .map(|mut command| {
                if command.parts.cwd_path.as_os_str().is_empty() {
                    command.parts.cwd_path = context.worktree_path.clone();
                }
                PlannedCommand::from(command)
            })
            .collect();

        ActionPlan::from_parts_unchecked(
            context.clone(),
            crate::PlanOrigin::Manifest {
                path: context.worktree_path.join(".treeboot.toml"),
            },
            Some(context.worktree_path.join(".treeboot.toml")),
            Vec::new(),
            commands,
        )
    }

    fn context(name: &str) -> (tempfile::TempDir, Worktree) {
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
            Worktree {
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

    #[cfg(unix)]
    fn remove_dir_symlink(path: &Path) -> io::Result<()> {
        std::fs::remove_file(path)
    }

    #[cfg(windows)]
    fn remove_dir_symlink(path: &Path) -> io::Result<()> {
        std::fs::remove_dir(path)
    }

    #[derive(Default)]
    struct Recorder {
        events: Vec<OutputEvent>,
    }

    impl Recorder {
        #[cfg(unix)]
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

    struct FailingReporter;

    impl Reporter for FailingReporter {
        fn report(&mut self, _event: OutputEvent) -> std::io::Result<()> {
            Err(std::io::Error::other("report failed"))
        }
    }
}
