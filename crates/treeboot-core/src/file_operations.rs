use crate::file_actions::add_symlink_warnings;
use crate::file_execution::{FileExecutionOptions, execute_file_operation_group};
use crate::file_planning::{FilePlanningOptions, plan_file_operation_group};
use crate::{ActionPlan, Error, OutputEvent, Reporter, Result};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct FileApplyOptions {
    pub(crate) strict: bool,
    pub(crate) force: bool,
    pub(crate) dry_run: bool,
    pub(crate) verbose: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct FileApplyReport {
    pub(crate) action_count: usize,
}

pub(crate) fn apply_file_operations(
    plan: &ActionPlan,
    options: FileApplyOptions,
    reporter: &mut dyn Reporter,
) -> Result<FileApplyReport> {
    let mut groups = Vec::new();
    for operation in plan.files() {
        if !options.verbose {
            report(
                reporter,
                OutputEvent::FileOperationPlanningStarted {
                    operation: operation.operation(),
                    source: operation.source().to_path_buf(),
                    target: operation.target().to_path_buf(),
                },
            )?;
        }

        let group = plan_file_operation_group(
            plan,
            operation,
            FilePlanningOptions {
                strict: options.strict,
                force: options.force,
            },
        )?;

        if !options.verbose {
            report(
                reporter,
                OutputEvent::FileOperationPlanningFinished {
                    operation: group.operation,
                    source: group.source.clone(),
                    target: group.target.clone(),
                    action_count: group.progress_action_count(),
                },
            )?;
        }

        groups.push(group);
    }
    add_symlink_warnings(&mut groups);

    // Plan every group before mutating so planning failures happen before
    // side effects and cross-operation symlink warnings can see all targets.
    let mut action_count = 0;
    for group in &groups {
        action_count += execute_file_operation_group(
            plan,
            group,
            FileExecutionOptions {
                dry_run: options.dry_run,
                verbose: options.verbose,
            },
            reporter,
        )?;
    }

    Ok(FileApplyReport { action_count })
}

fn report(reporter: &mut dyn Reporter, event: OutputEvent) -> Result<()> {
    reporter
        .report(event)
        .map_err(|source| Error::Output { source })
}

#[cfg(test)]
mod tests;
