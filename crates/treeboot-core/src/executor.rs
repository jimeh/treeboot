use crate::commands::{CommandExecutionOptions, execute_commands};
use crate::files::{FileApplyOptions, apply_file_operations};
use crate::{ActionPlan, Reporter, Result};

/// Options that control action plan execution.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExecuteOptions {
    /// Rejects strict-mode file-operation conflicts.
    pub strict: bool,
    /// Replaces existing file-operation targets where supported.
    pub force: bool,
    /// Prints planned work without changing files or running commands.
    pub dry_run: bool,
    /// Applies file operations only.
    pub skip_commands: bool,
}

/// Result summary for action plan execution.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExecutionReport {
    /// Number of file actions applied or reported.
    pub file_action_count: usize,
}

/// Executes validated action plans.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Executor {
    options: ExecuteOptions,
}

impl Executor {
    /// Creates an executor from execution options.
    #[must_use]
    pub const fn new(options: ExecuteOptions) -> Self {
        Self { options }
    }

    /// Applies the file-operation portion of a plan.
    ///
    /// # Errors
    ///
    /// Returns an error if file operation application or output reporting
    /// fails.
    pub fn execute_files(
        &self,
        plan: &ActionPlan,
        reporter: &mut dyn Reporter,
    ) -> Result<ExecutionReport> {
        let report = apply_file_operations(
            plan,
            FileApplyOptions {
                strict: self.options.strict,
                force: self.options.force,
                dry_run: self.options.dry_run,
            },
            reporter,
        )?;

        Ok(ExecutionReport {
            file_action_count: report.action_count,
        })
    }

    /// Executes the command portion of a plan.
    ///
    /// # Errors
    ///
    /// Returns an error if command execution or output reporting fails.
    pub fn execute_commands(&self, plan: &ActionPlan, reporter: &mut dyn Reporter) -> Result<()> {
        execute_commands(
            plan,
            CommandExecutionOptions {
                dry_run: self.options.dry_run,
            },
            reporter,
        )
    }

    /// Executes a complete action plan.
    ///
    /// # Errors
    ///
    /// Returns an error if file operation application, command execution, or
    /// output reporting fails.
    pub fn execute(
        &self,
        plan: &ActionPlan,
        reporter: &mut dyn Reporter,
    ) -> Result<ExecutionReport> {
        let report = self.execute_files(plan, reporter)?;

        if !self.options.skip_commands {
            self.execute_commands(plan, reporter)?;
        }

        Ok(report)
    }
}
