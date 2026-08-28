use crate::file_actions::PlannedFileOperationActions;
use crate::file_actions::add_symlink_warnings;
use crate::file_execution::{
    ExecutionWarningInjector, FileExecutionOptions, FileExecutionReport,
    execute_file_operation_group_detailed_with_warning_injector,
};
use crate::file_planning::{FilePlanningOptions, plan_file_operation_group};
use crate::{ActionPlan, Error, OutputEvent, Reporter, Result};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct FileApplyOptions {
    pub(crate) strict: bool,
    pub(crate) force: bool,
    pub(crate) dry_run: bool,
    pub(crate) verbose: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct FileApplyReport {
    pub(crate) action_count: usize,
    pub(crate) execution_warnings: Vec<crate::BootstrapWarning>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PreparedFileOperations {
    pub(crate) groups: Vec<PlannedFileOperationActions>,
}

pub(crate) fn prepare_file_operations(
    plan: &ActionPlan,
    options: FileApplyOptions,
    reporter: &mut dyn Reporter,
) -> Result<PreparedFileOperations> {
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

    Ok(PreparedFileOperations { groups })
}

pub(crate) fn execute_prepared_file_operations(
    plan: &ActionPlan,
    prepared: &PreparedFileOperations,
    options: FileApplyOptions,
    reporter: &mut dyn Reporter,
) -> Result<FileApplyReport> {
    execute_prepared_file_operations_with_warning_injector(plan, prepared, options, reporter, None)
}

pub(crate) fn execute_prepared_file_operations_with_warning_injector(
    plan: &ActionPlan,
    prepared: &PreparedFileOperations,
    options: FileApplyOptions,
    reporter: &mut dyn Reporter,
    warning_injector: Option<ExecutionWarningInjector>,
) -> Result<FileApplyReport> {
    let mut action_count = 0;
    let mut execution_warnings = Vec::new();
    for group in &prepared.groups {
        let FileExecutionReport {
            action_count: group_action_count,
            execution_warnings: group_warnings,
        } = execute_file_operation_group_detailed_with_warning_injector(
            plan,
            group,
            FileExecutionOptions {
                dry_run: options.dry_run,
                verbose: options.verbose,
            },
            reporter,
            warning_injector,
        )?;
        action_count += group_action_count;
        execution_warnings.extend(group_warnings);
    }

    Ok(FileApplyReport {
        action_count,
        execution_warnings,
    })
}

pub(crate) fn apply_file_operations(
    plan: &ActionPlan,
    options: FileApplyOptions,
    reporter: &mut dyn Reporter,
) -> Result<FileApplyReport> {
    let prepared = prepare_file_operations(plan, options, reporter)?;
    // Plan every group before mutating so planning failures happen before
    // side effects and cross-operation symlink warnings can see all targets.
    execute_prepared_file_operations(plan, &prepared, options, reporter)
}

fn report(reporter: &mut dyn Reporter, event: OutputEvent) -> Result<()> {
    reporter
        .report(event)
        .map_err(|source| Error::Output { source })
}

#[cfg(test)]
mod tests;
