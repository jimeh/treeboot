use std::path::Path;

use crate::file_actions::{FileAction, PlannedFileOperationActions};
use crate::file_system::{
    apply_metadata, copy_file_with_metadata_with_policy, create_parent_dir, create_symlink,
    create_target_dir, ensure_preserved_source_symlink_safe, remove_any, remove_file_checked,
    with_writable_parent,
};
use crate::{ActionPlan, Error, FileOperationKind, OutputEvent, Reporter, Result};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct FileExecutionOptions {
    pub(crate) dry_run: bool,
    pub(crate) verbose: bool,
}

pub(crate) fn execute_file_operation_group(
    plan: &ActionPlan,
    group: &PlannedFileOperationActions,
    options: FileExecutionOptions,
    reporter: &mut dyn Reporter,
) -> Result<usize> {
    if group.actions.is_empty() {
        return Ok(0);
    }

    let progress_action_count = group.progress_action_count();
    if !options.verbose {
        report(
            reporter,
            OutputEvent::FileOperationExecutionStarted {
                operation: group.operation,
                source: group.source.clone(),
                target: group.target.clone(),
                action_count: progress_action_count,
            },
        )?;
    }

    for action in &group.actions {
        let progress_action = action.counts();
        if options.dry_run {
            report_dry_run(action, reporter, options.verbose)?;
        } else {
            apply_action(plan, action, reporter, options.verbose)?;
        }

        if !options.verbose && progress_action {
            report(
                reporter,
                OutputEvent::FileOperationActionAdvanced {
                    operation: group.operation,
                    source: group.source.clone(),
                    target: group.target.clone(),
                },
            )?;
        }
    }

    if !options.verbose {
        let summary = group.summary();
        if summary.decision_count() > 0 {
            report(
                reporter,
                OutputEvent::FileOperationFinished {
                    operation: group.operation,
                    source: group.source.clone(),
                    target: group.target.clone(),
                    summary,
                    dry_run: options.dry_run,
                },
            )?;
        }
    }

    Ok(progress_action_count)
}

fn report_dry_run(action: &FileAction, reporter: &mut dyn Reporter, detailed: bool) -> Result<()> {
    if !detailed && !matches!(action, FileAction::Warning { .. }) {
        return Ok(());
    }

    match action {
        FileAction::CreateDirectory {
            operation,
            source,
            target,
            ..
        }
        | FileAction::CopyFile {
            operation,
            source,
            target,
            ..
        }
        | FileAction::CreateSymlink {
            operation,
            source,
            target,
            ..
        } => report(
            reporter,
            OutputEvent::FileWouldApply {
                operation: *operation,
                source: source.clone(),
                target: target.clone(),
            },
        ),
        FileAction::RepairMetadata {
            source,
            target,
            report: true,
            ..
        } => report(
            reporter,
            OutputEvent::FileMetadataWouldApply {
                source: source.clone(),
                target: target.clone(),
            },
        ),
        FileAction::RepairMetadata { report: false, .. } => Ok(()),
        FileAction::Delete { target, .. } => report(
            reporter,
            OutputEvent::FileWouldDelete {
                path: target.clone(),
            },
        ),
        FileAction::Skip {
            operation,
            target,
            reason,
        } => report(
            reporter,
            OutputEvent::FileWouldSkip {
                operation: *operation,
                target: target.clone(),
                reason: reason.clone(),
            },
        ),
        FileAction::Warning { path, reason } => report(
            reporter,
            OutputEvent::FileWarning {
                path: path.clone(),
                reason: reason.clone(),
            },
        ),
    }
}

fn apply_action(
    plan: &ActionPlan,
    action: &FileAction,
    reporter: &mut dyn Reporter,
    detailed: bool,
) -> Result<()> {
    match action {
        FileAction::CreateDirectory {
            operation,
            source,
            target,
            target_path,
        } => {
            with_writable_parent(
                *operation,
                target_path,
                &plan.context().worktree_path,
                || create_target_dir(*operation, target_path, &plan.context().worktree_path),
            )?;
            if detailed {
                report_applied(reporter, *operation, source, target)?;
            }
            Ok(())
        }
        FileAction::CopyFile {
            operation,
            source,
            target,
            source_path,
            target_path,
            metadata_policy,
            replace,
        } => {
            with_writable_parent(
                *operation,
                target_path,
                &plan.context().worktree_path,
                || {
                    create_parent_dir(*operation, target_path, &plan.context().worktree_path)?;
                    if *replace {
                        remove_file_checked(
                            *operation,
                            target_path,
                            &plan.context().worktree_path,
                        )?;
                    }
                    copy_file_with_metadata_with_policy(
                        *operation,
                        source_path,
                        target_path,
                        &plan.context().root_path,
                        &plan.context().worktree_path,
                        *metadata_policy,
                        Some(reporter),
                    )
                },
            )?;
            if detailed {
                report_applied(reporter, *operation, source, target)?;
            }
            Ok(())
        }
        FileAction::RepairMetadata {
            operation,
            source,
            target,
            source_path,
            target_path,
            metadata_policy,
            target_kind,
            report: should_report,
        } => {
            apply_metadata(
                *operation,
                source_path,
                target_path,
                *metadata_policy,
                *target_kind,
                Some(reporter),
            )?;
            if detailed && *should_report {
                report(
                    reporter,
                    OutputEvent::FileMetadataApplied {
                        source: source.clone(),
                        target: target.clone(),
                    },
                )?;
            }
            Ok(())
        }
        FileAction::CreateSymlink {
            operation,
            source,
            target,
            target_path,
            preserved_source_path,
            link_target,
            final_target,
            target_is_dir,
            replace,
        } => {
            if let Some(source_path) = preserved_source_path {
                ensure_preserved_source_symlink_safe(
                    plan,
                    *operation,
                    source_path,
                    target_path,
                    link_target,
                    final_target,
                    *target_is_dir,
                )?;
            }
            with_writable_parent(
                *operation,
                target_path,
                &plan.context().worktree_path,
                || {
                    create_parent_dir(*operation, target_path, &plan.context().worktree_path)?;
                    if *replace {
                        remove_file_checked(
                            *operation,
                            target_path,
                            &plan.context().worktree_path,
                        )?;
                    }
                    create_symlink(
                        *operation,
                        link_target,
                        *target_is_dir,
                        target_path,
                        &plan.context().worktree_path,
                    )
                },
            )?;
            if detailed {
                report_applied(reporter, *operation, source, target)?;
            }
            Ok(())
        }
        FileAction::Delete {
            target,
            target_path,
        } => {
            with_writable_parent(
                FileOperationKind::Sync,
                target_path,
                &plan.context().worktree_path,
                || {
                    remove_any(
                        FileOperationKind::Sync,
                        target_path,
                        &plan.context().worktree_path,
                    )
                },
            )?;
            if detailed {
                report(
                    reporter,
                    OutputEvent::FileDeleted {
                        path: target.clone(),
                    },
                )?;
            }
            Ok(())
        }
        FileAction::Skip {
            operation,
            target,
            reason,
        } => {
            if detailed {
                report(
                    reporter,
                    OutputEvent::FileSkipped {
                        operation: *operation,
                        target: target.clone(),
                        reason: reason.clone(),
                    },
                )?;
            }
            Ok(())
        }
        FileAction::Warning { path, reason } => report(
            reporter,
            OutputEvent::FileWarning {
                path: path.clone(),
                reason: reason.clone(),
            },
        ),
    }
}

fn report_applied(
    reporter: &mut dyn Reporter,
    operation: FileOperationKind,
    source: &Path,
    target: &Path,
) -> Result<()> {
    report(
        reporter,
        OutputEvent::FileApplied {
            operation,
            source: source.to_path_buf(),
            target: target.to_path_buf(),
        },
    )
}

fn report(reporter: &mut dyn Reporter, event: OutputEvent) -> Result<()> {
    reporter
        .report(event)
        .map_err(|source| Error::Output { source })
}
