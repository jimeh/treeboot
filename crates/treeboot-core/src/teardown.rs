use std::path::PathBuf;

use crate::commands::{CommandExecutionOptions, execute_teardown_commands as execute_commands};
use crate::context;
use crate::{
    Config, EnvironmentInput, Error, OutputEvent, Reporter, Result, TeardownPlan, Worktree,
    WorktreeOptions,
};

/// Options for preparing worktree teardown commands.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TeardownOptions {
    /// Directory from which target worktree discovery starts.
    pub cwd: Option<PathBuf>,
    /// Overrides root-checkout discovery.
    pub root: Option<PathBuf>,
    /// Explicit environment input used for compatibility discovery.
    pub environment: EnvironmentInput,
    /// Uses one specific config file instead of discovery.
    pub config: Option<PathBuf>,
}

/// The prepared outcome of teardown discovery and validation.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TeardownAction {
    /// No discovered config exists, so teardown is a no-op.
    MissingConfig,
    /// The selected config has no teardown commands.
    NoCommands,
    /// A validated teardown plan is ready for approval and execution.
    Ready(TeardownPlan),
}

/// A prepared teardown request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedTeardown {
    context: Worktree,
    action: TeardownAction,
}

impl PreparedTeardown {
    /// Returns the resolved target worktree.
    #[must_use]
    pub const fn context(&self) -> &Worktree {
        &self.context
    }

    /// Returns the prepared teardown outcome.
    #[must_use]
    pub const fn action(&self) -> &TeardownAction {
        &self.action
    }

    /// Returns the ready plan, when teardown commands were configured.
    #[must_use]
    pub const fn plan(&self) -> Option<&TeardownPlan> {
        match &self.action {
            TeardownAction::Ready(plan) => Some(plan),
            TeardownAction::MissingConfig | TeardownAction::NoCommands => None,
        }
    }
}

/// Options for executing an already prepared teardown plan.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TeardownExecuteOptions {
    /// Reports commands without spawning them.
    pub dry_run: bool,
}

/// Result summary for teardown execution.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TeardownReport {
    /// Number of teardown commands reported or executed.
    pub command_count: usize,
}

/// Discovers, parses, and validates teardown commands without executing them.
///
/// Discovery and no-op output is emitted during preparation. Callers can then
/// obtain approval before executing the immutable returned plan.
///
/// # Errors
///
/// Returns an error for discovery, config loading, teardown validation, output,
/// or root-checkout targeting failures.
pub fn prepare_teardown(
    options: TeardownOptions,
    reporter: &mut dyn Reporter,
) -> Result<PreparedTeardown> {
    let context = context::resolve(&WorktreeOptions {
        cwd: options.cwd,
        root: options.root,
        environment: options.environment,
    })?;

    if context.is_root() {
        return Err(Error::RootWorktreeTeardown);
    }

    let Some(path) = Config::discover_path(&context, options.config.as_deref())? else {
        report(reporter, OutputEvent::NoConfigDetected)?;
        return Ok(PreparedTeardown {
            context,
            action: TeardownAction::MissingConfig,
        });
    };

    let config = Config::load(&path, &context)?;
    report(reporter, OutputEvent::ConfigDetected { path: path.clone() })?;
    let plan = TeardownPlan::from_manifest(&path, &config, &context)?;
    if plan.commands().is_empty() {
        report(reporter, OutputEvent::NoTeardownCommandsConfigured)?;
        return Ok(PreparedTeardown {
            context,
            action: TeardownAction::NoCommands,
        });
    }

    Ok(PreparedTeardown {
        context,
        action: TeardownAction::Ready(plan),
    })
}

/// Executes an already prepared teardown plan.
///
/// This function never removes or otherwise mutates the Git worktree itself.
///
/// # Errors
///
/// Returns an error when command execution or output reporting fails.
pub fn execute_teardown(
    plan: &TeardownPlan,
    options: TeardownExecuteOptions,
    reporter: &mut dyn Reporter,
) -> Result<TeardownReport> {
    execute_commands(
        plan.context(),
        plan.planned_commands(),
        CommandExecutionOptions {
            dry_run: options.dry_run,
        },
        reporter,
    )?;

    Ok(TeardownReport {
        command_count: plan.commands().len(),
    })
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
    use std::path::Path;

    use tempfile::TempDir;

    use super::*;

    #[derive(Default)]
    struct VecReporter {
        events: Vec<OutputEvent>,
    }

    impl Reporter for VecReporter {
        fn report(&mut self, event: OutputEvent) -> std::io::Result<()> {
            self.events.push(event);
            Ok(())
        }
    }

    fn fixture(content: &str) -> (TempDir, Worktree, Config, PathBuf) {
        let temp = TempDir::new().expect("tempdir should be created");
        let root = temp.path().join("root");
        let worktree = temp.path().join("worktree");
        std::fs::create_dir_all(&root).expect("root should be created");
        std::fs::create_dir_all(&worktree).expect("worktree should be created");
        let context = Worktree::from_parts(
            root.clone(),
            worktree.clone(),
            "main".to_owned(),
            BTreeMap::from([
                ("TREEBOOT_ROOT_PATH".to_owned(), OsString::from(root)),
                (
                    "TREEBOOT_WORKTREE_PATH".to_owned(),
                    OsString::from(&worktree),
                ),
            ]),
        );
        let path = worktree.join(".treeboot.toml");
        let config = Config::parse(&path, content, &context).expect("config should parse");
        (temp, context, config, path)
    }

    #[test]
    fn execute_teardown_should_emit_only_teardown_dry_run_events() {
        let (_temp, context, config, path) =
            fixture(r#"teardown_commands = ["echo first", "echo second"]"#);
        let plan =
            TeardownPlan::from_manifest(&path, &config, &context).expect("plan should build");
        let mut reporter = VecReporter::default();

        let report = execute_teardown(
            &plan,
            TeardownExecuteOptions { dry_run: true },
            &mut reporter,
        )
        .expect("dry run should succeed");

        assert_eq!(report.command_count, 2);
        assert_eq!(
            reporter.events,
            vec![
                OutputEvent::TeardownCommandWouldRun {
                    label: "echo first".to_owned()
                },
                OutputEvent::TeardownCommandWouldRun {
                    label: "echo second".to_owned()
                },
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn execute_teardown_should_run_commands_without_bootstrap_work() {
        let (_temp, context, config, path) = fixture(
            r#"
commands = ["touch bootstrap-marker"]
teardown_commands = ["touch teardown-marker"]
"#,
        );
        let plan =
            TeardownPlan::from_manifest(&path, &config, &context).expect("plan should build");
        let mut reporter = VecReporter::default();

        execute_teardown(&plan, TeardownExecuteOptions::default(), &mut reporter)
            .expect("teardown should run");

        assert!(!context.worktree_path.join("bootstrap-marker").exists());
        assert!(context.worktree_path.join("teardown-marker").exists());
        assert_eq!(
            reporter.events,
            vec![OutputEvent::TeardownCommandStarted {
                label: "touch teardown-marker".to_owned()
            }]
        );
    }

    #[test]
    fn teardown_plan_should_reject_owned_environment_override() {
        let (_temp, context, config, path) = fixture(
            r#"
teardown_commands = [
  { run = "echo no", env = { TREEBOOT_ROOT_PATH = "elsewhere" } },
]
"#,
        );

        let error = TeardownPlan::from_manifest(Path::new(&path), &config, &context)
            .expect_err("owned variable should be rejected");

        assert!(
            error
                .to_string()
                .contains("overrides treeboot-owned variable")
        );
    }

    #[cfg(unix)]
    #[test]
    fn execute_teardown_should_revalidate_live_cwd_before_spawn() {
        use crate::test_support::symlink_dir;

        let (temp, context, config, path) =
            fixture(r#"teardown_commands = [{ run = "touch marker", cwd = "cwd" }]"#);
        let original = context.worktree_path.join("original");
        let outside = temp.path().join("outside");
        std::fs::create_dir_all(&original).expect("original cwd should be created");
        std::fs::create_dir_all(&outside).expect("outside cwd should be created");
        let link = context.worktree_path.join("cwd");
        symlink_dir(&original, &link).expect("cwd symlink should be created");
        let reparsed = Config::parse(
            &path,
            r#"teardown_commands = [{ run = "touch marker", cwd = "cwd" }]"#,
            &context,
        )
        .expect("config should parse after cwd exists");
        let plan =
            TeardownPlan::from_manifest(&path, &reparsed, &context).expect("plan should build");
        std::fs::remove_file(&link).expect("cwd symlink should be removed");
        symlink_dir(&outside, &link).expect("cwd symlink should be retargeted");
        let mut reporter = VecReporter::default();

        let error = execute_teardown(&plan, TeardownExecuteOptions::default(), &mut reporter)
            .expect_err("retargeted cwd should fail");

        assert!(
            error
                .to_string()
                .contains("command cwd resolves outside worktree")
        );
        assert!(!outside.join("marker").exists());
        assert_eq!(config.teardown_commands.len(), 1);
    }
}
