use std::path::{Path, PathBuf};

use crate::FileOperationKind;

/// Counts produced by one top-level file operation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileOperationSummary {
    /// Number of created, updated, or replaced paths.
    pub changed: usize,
    /// Number of skipped paths.
    pub skipped: usize,
    /// Number of deleted target-only paths.
    pub deleted: usize,
    /// Number of warnings emitted.
    pub warnings: usize,
    /// Number of metadata-only sync repairs.
    pub metadata_changed: usize,
    /// Whether the summary represents expanded directory work.
    pub expanded: bool,
    /// Reason for a single skipped top-level operation.
    pub skip_reason: Option<String>,
}

impl FileOperationSummary {
    /// Returns the number of visible action decisions in the summary.
    #[must_use]
    pub const fn decision_count(&self) -> usize {
        self.changed + self.skipped + self.deleted
    }

    /// Formats the summary as a user-facing file-operation line.
    #[must_use]
    pub fn message(
        &self,
        operation: FileOperationKind,
        source: &Path,
        target: &Path,
        dry_run: bool,
    ) -> String {
        format_file_operation_summary(operation, source, target, self, dry_run)
    }

    fn count_details(&self, dry_run: bool) -> Vec<String> {
        let mut details = Vec::new();
        if self.changed > 0 {
            details.push(count_detail(
                self.changed,
                if dry_run { "change" } else { "changed" },
                if dry_run { "changes" } else { "changed" },
            ));
        }
        if self.skipped > 0 {
            details.push(count_detail(
                self.skipped,
                if dry_run { "skip" } else { "skipped" },
                if dry_run { "skips" } else { "skipped" },
            ));
        }
        if self.deleted > 0 {
            details.push(count_detail(
                self.deleted,
                if dry_run { "delete" } else { "deleted" },
                if dry_run { "deletes" } else { "deleted" },
            ));
        }
        details
    }
}

fn count_detail(count: usize, singular: &str, plural: &str) -> String {
    let noun = if count == 1 { singular } else { plural };
    format!("{count} {noun}")
}

/// A structured message produced during a treeboot operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputEvent {
    /// A non-executable script candidate was ignored.
    IgnoredInitScript {
        /// Script candidate path.
        path: PathBuf,
    },

    /// A dry run would execute the given init script.
    WouldRunInitScript {
        /// Script path.
        path: PathBuf,
        /// Root checkout path passed as the script argument.
        root_path: PathBuf,
    },

    /// An init script is about to run.
    RunInitScript {
        /// Script path.
        path: PathBuf,
    },

    /// No script or config was found.
    NoConfigDetected,

    /// The run started from the root checkout instead of a separate worktree.
    RootWorktreeDetected,

    /// A config file was found.
    ConfigDetected {
        /// Config file path.
        path: PathBuf,
    },

    /// A file operation was applied.
    FileApplied {
        /// File operation kind.
        operation: FileOperationKind,
        /// Display source path.
        source: PathBuf,
        /// Display target path.
        target: PathBuf,
    },

    /// A dry run would apply a file operation.
    FileWouldApply {
        /// File operation kind.
        operation: FileOperationKind,
        /// Display source path.
        source: PathBuf,
        /// Display target path.
        target: PathBuf,
    },

    /// A sync operation applied metadata-only changes.
    FileMetadataApplied {
        /// Display source path.
        source: PathBuf,
        /// Display target path.
        target: PathBuf,
    },

    /// A dry run would apply metadata-only sync changes.
    FileMetadataWouldApply {
        /// Display source path.
        source: PathBuf,
        /// Display target path.
        target: PathBuf,
    },

    /// A file operation was skipped.
    FileSkipped {
        /// File operation kind.
        operation: FileOperationKind,
        /// Display target path.
        target: PathBuf,
        /// Reason the operation was skipped.
        reason: String,
    },

    /// A dry run would skip a file operation.
    FileWouldSkip {
        /// File operation kind.
        operation: FileOperationKind,
        /// Display target path.
        target: PathBuf,
        /// Reason the operation would be skipped.
        reason: String,
    },

    /// A sync operation deleted a target-only path.
    FileDeleted {
        /// Deleted path.
        path: PathBuf,
    },

    /// A dry-run sync operation would delete a target-only path.
    FileWouldDelete {
        /// Path that would be deleted.
        path: PathBuf,
    },

    /// A file operation warning was produced.
    FileWarning {
        /// Warning path.
        path: PathBuf,
        /// Human-readable warning detail.
        reason: String,
    },

    /// Ownership metadata could not be preserved.
    OwnershipWarning {
        /// Warning path.
        path: PathBuf,
        /// Human-readable warning detail.
        reason: String,
    },

    /// A command is about to run.
    CommandStarted {
        /// Human-readable command label.
        label: String,
    },

    /// A dry run would execute a command.
    CommandWouldRun {
        /// Human-readable command label.
        label: String,
    },

    /// A command failure was allowed and execution will continue.
    CommandAllowedFailure {
        /// Human-readable command label.
        label: String,
        /// Failure detail.
        reason: String,
    },

    /// An init file was created.
    InitCreated {
        /// Created file path.
        path: PathBuf,
    },
}

impl OutputEvent {
    /// Formats the event as a user-facing line.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::IgnoredInitScript { path } => {
                format!("treeboot: ignore {}; not executable", path.display())
            }
            Self::WouldRunInitScript { path, root_path } => format!(
                "treeboot: would run {} {}",
                path.display(),
                root_path.display()
            ),
            Self::RunInitScript { path } => {
                format!("treeboot: run {}", path.display())
            }
            Self::NoConfigDetected => "treeboot: no config detected".to_owned(),
            Self::RootWorktreeDetected => "treeboot: This is not a work tree".to_owned(),
            Self::ConfigDetected { path } => {
                format!("treeboot: config detected {}", path.display())
            }
            Self::FileApplied {
                operation,
                source,
                target,
            } => format!(
                "treeboot: {} {} -> {}",
                operation.as_str(),
                source.display(),
                target.display()
            ),
            Self::FileWouldApply {
                operation,
                source,
                target,
            } => format!(
                "treeboot: would {} {} -> {}",
                operation.as_str(),
                source.display(),
                target.display()
            ),
            Self::FileMetadataApplied { source, target } => format!(
                "treeboot: sync metadata {} -> {}",
                source.display(),
                target.display()
            ),
            Self::FileMetadataWouldApply { source, target } => format!(
                "treeboot: would sync metadata {} -> {}",
                source.display(),
                target.display()
            ),
            Self::FileSkipped {
                operation,
                target,
                reason,
            } => format!(
                "treeboot: skip {} {}; {}",
                operation.as_str(),
                target.display(),
                reason
            ),
            Self::FileWouldSkip {
                operation,
                target,
                reason,
            } => format!(
                "treeboot: would skip {} {}; {}",
                operation.as_str(),
                target.display(),
                reason
            ),
            Self::FileDeleted { path } => {
                format!("treeboot: delete {}", path.display())
            }
            Self::FileWouldDelete { path } => {
                format!("treeboot: would delete {}", path.display())
            }
            Self::FileWarning { path, reason } => {
                format!("treeboot: warning: {} {}", path.display(), reason)
            }
            Self::OwnershipWarning { path, reason } => format!(
                "treeboot: warning: could not preserve ownership {}: {}",
                path.display(),
                reason
            ),
            Self::CommandStarted { label } => {
                format!("treeboot: run {label}")
            }
            Self::CommandWouldRun { label } => {
                format!("treeboot: would run {label}")
            }
            Self::CommandAllowedFailure { label, reason } => {
                format!("treeboot: warning: command {label} {reason}")
            }
            Self::InitCreated { path } => {
                format!("treeboot: created {}", path.display())
            }
        }
    }
}

fn format_file_operation_summary(
    operation: FileOperationKind,
    source: &Path,
    target: &Path,
    summary: &FileOperationSummary,
    dry_run: bool,
) -> String {
    if summary.decision_count() == 1 {
        if summary.changed == 1 {
            if summary.metadata_changed == 1 {
                if dry_run {
                    return format!(
                        "treeboot: would sync metadata {} -> {}",
                        source.display(),
                        target.display()
                    );
                }

                return format!(
                    "treeboot: sync metadata {} -> {}",
                    source.display(),
                    target.display()
                );
            }

            if !summary.expanded && dry_run {
                return format!(
                    "treeboot: would {} {} -> {}",
                    operation.as_str(),
                    source.display(),
                    target.display()
                );
            }

            if !summary.expanded {
                return format!(
                    "treeboot: {} {} -> {}",
                    operation.as_str(),
                    source.display(),
                    target.display()
                );
            }
        }

        if summary.skipped == 1 {
            let reason = summary.skip_reason.as_deref().unwrap_or("skipped");
            if dry_run {
                return format!(
                    "treeboot: would skip {} {}; {}",
                    operation.as_str(),
                    target.display(),
                    reason
                );
            }

            return format!(
                "treeboot: skip {} {}; {}",
                operation.as_str(),
                target.display(),
                reason
            );
        }
    }

    let details = summary.count_details(dry_run).join(", ");
    let suffix = if details.is_empty() {
        String::new()
    } else {
        format!(" ({details})")
    };
    if dry_run {
        format!(
            "treeboot: would {} {} -> {}{suffix}",
            operation.as_str(),
            source.display(),
            target.display()
        )
    } else {
        format!(
            "treeboot: {} {} -> {}{suffix}",
            operation.as_str(),
            source.display(),
            target.display()
        )
    }
}

/// Receives structured output events from core operations.
pub trait Reporter {
    /// Handles one output event.
    fn report(&mut self, event: OutputEvent) -> std::io::Result<()>;

    /// Handles the start of planning for one top-level file operation.
    fn file_operation_planning_started(
        &mut self,
        operation: FileOperationKind,
        source: &Path,
        target: &Path,
    ) -> std::io::Result<()> {
        let _ = (operation, source, target);
        Ok(())
    }

    /// Handles completion of planning for one top-level file operation.
    fn file_operation_planning_finished(
        &mut self,
        operation: FileOperationKind,
        source: &Path,
        target: &Path,
        action_count: usize,
    ) -> std::io::Result<()> {
        let _ = (operation, source, target, action_count);
        Ok(())
    }

    /// Handles the start of execution for one top-level file operation.
    fn file_operation_execution_started(
        &mut self,
        operation: FileOperationKind,
        source: &Path,
        target: &Path,
        action_count: usize,
    ) -> std::io::Result<()> {
        let _ = (operation, source, target, action_count);
        Ok(())
    }

    /// Handles completion of one concrete file-operation action.
    fn file_operation_action_advanced(
        &mut self,
        operation: FileOperationKind,
        source: &Path,
        target: &Path,
    ) -> std::io::Result<()> {
        let _ = (operation, source, target);
        Ok(())
    }

    /// Handles completion of one top-level compact file operation.
    fn file_operation_finished(
        &mut self,
        operation: FileOperationKind,
        source: &Path,
        target: &Path,
        summary: &FileOperationSummary,
        dry_run: bool,
    ) -> std::io::Result<()> {
        let _ = (operation, source, target, summary, dry_run);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::FileOperationKind;

    #[test]
    fn message_should_format_ignored_init_script() {
        let event = OutputEvent::IgnoredInitScript {
            path: PathBuf::from(".treeboot.sh"),
        };

        assert_eq!(
            event.message(),
            "treeboot: ignore .treeboot.sh; not executable"
        );
    }

    #[test]
    fn message_should_format_dry_run_init_script() {
        let event = OutputEvent::WouldRunInitScript {
            path: PathBuf::from(".treeboot.sh"),
            root_path: PathBuf::from("/repo"),
        };

        assert_eq!(event.message(), "treeboot: would run .treeboot.sh /repo");
    }

    #[test]
    fn message_should_format_config_detected() {
        let event = OutputEvent::ConfigDetected {
            path: PathBuf::from(".treeboot.toml"),
        };

        assert_eq!(event.message(), "treeboot: config detected .treeboot.toml");
    }

    #[test]
    fn message_should_format_file_applied() {
        let event = OutputEvent::FileApplied {
            operation: FileOperationKind::Copy,
            source: PathBuf::from(".env"),
            target: PathBuf::from(".env"),
        };

        assert_eq!(event.message(), "treeboot: copy .env -> .env");
    }

    #[test]
    fn message_should_format_file_would_apply() {
        let event = OutputEvent::FileWouldApply {
            operation: FileOperationKind::Symlink,
            source: PathBuf::from("tool"),
            target: PathBuf::from(".tool"),
        };

        assert_eq!(event.message(), "treeboot: would symlink tool -> .tool");
    }

    #[test]
    fn message_should_format_file_metadata_applied() {
        let event = OutputEvent::FileMetadataApplied {
            source: PathBuf::from("shared/config"),
            target: PathBuf::from(".config"),
        };

        assert_eq!(
            event.message(),
            "treeboot: sync metadata shared/config -> .config"
        );
    }

    #[test]
    fn message_should_format_file_metadata_would_apply() {
        let event = OutputEvent::FileMetadataWouldApply {
            source: PathBuf::from("shared/config"),
            target: PathBuf::from(".config"),
        };

        assert_eq!(
            event.message(),
            "treeboot: would sync metadata shared/config -> .config"
        );
    }

    #[test]
    fn message_should_format_file_skipped() {
        let event = OutputEvent::FileSkipped {
            operation: FileOperationKind::Copy,
            target: PathBuf::from(".env"),
            reason: "target exists".to_owned(),
        };

        assert_eq!(event.message(), "treeboot: skip copy .env; target exists");
    }

    #[test]
    fn message_should_format_file_would_skip() {
        let event = OutputEvent::FileWouldSkip {
            operation: FileOperationKind::Sync,
            target: PathBuf::from("shared"),
            reason: "missing source".to_owned(),
        };

        assert_eq!(
            event.message(),
            "treeboot: would skip sync shared; missing source"
        );
    }

    #[test]
    fn message_should_format_file_deleted() {
        let event = OutputEvent::FileDeleted {
            path: PathBuf::from(".config/old.toml"),
        };

        assert_eq!(event.message(), "treeboot: delete .config/old.toml");
    }

    #[test]
    fn message_should_format_file_would_delete() {
        let event = OutputEvent::FileWouldDelete {
            path: PathBuf::from(".config/old.toml"),
        };

        assert_eq!(event.message(), "treeboot: would delete .config/old.toml");
    }

    #[test]
    fn message_should_format_file_warning() {
        let event = OutputEvent::FileWarning {
            path: PathBuf::from("shared/link"),
            reason: "symlink target does not exist".to_owned(),
        };

        assert_eq!(
            event.message(),
            "treeboot: warning: shared/link symlink target does not exist"
        );
    }

    #[test]
    fn message_should_format_ownership_warning() {
        let event = OutputEvent::OwnershipWarning {
            path: PathBuf::from("shared/config"),
            reason: "operation not permitted".to_owned(),
        };

        assert_eq!(
            event.message(),
            "treeboot: warning: could not preserve ownership shared/config: operation not permitted"
        );
    }

    #[test]
    fn message_should_format_single_file_operation_summary_without_counts() {
        let summary = FileOperationSummary {
            changed: 1,
            ..FileOperationSummary::default()
        };

        assert_eq!(
            summary.message(
                FileOperationKind::Copy,
                Path::new(".env"),
                Path::new(".env"),
                false
            ),
            "treeboot: copy .env -> .env"
        );
    }

    #[test]
    fn message_should_format_expanded_file_operation_summary_with_counts() {
        let summary = FileOperationSummary {
            changed: 4,
            deleted: 1,
            expanded: true,
            ..FileOperationSummary::default()
        };

        assert_eq!(
            summary.message(
                FileOperationKind::Sync,
                Path::new("shared"),
                Path::new("shared"),
                false
            ),
            "treeboot: sync shared -> shared (4 changed, 1 deleted)"
        );
    }

    #[test]
    fn message_should_omit_empty_file_operation_summary_counts() {
        let summary = FileOperationSummary {
            warnings: 1,
            ..FileOperationSummary::default()
        };

        assert_eq!(
            summary.message(
                FileOperationKind::Copy,
                Path::new("shared/link"),
                Path::new("shared/link"),
                false
            ),
            "treeboot: copy shared/link -> shared/link"
        );
    }

    #[test]
    fn message_should_format_single_dry_run_skip_summary() {
        let summary = FileOperationSummary {
            skipped: 1,
            skip_reason: Some("target exists".to_owned()),
            ..FileOperationSummary::default()
        };

        assert_eq!(
            summary.message(
                FileOperationKind::Copy,
                Path::new(".env"),
                Path::new(".env"),
                true
            ),
            "treeboot: would skip copy .env; target exists"
        );
    }

    #[test]
    fn message_should_format_root_worktree_detected() {
        let event = OutputEvent::RootWorktreeDetected;

        assert_eq!(event.message(), "treeboot: This is not a work tree");
    }

    #[test]
    fn message_should_format_command_started() {
        let event = OutputEvent::CommandStarted {
            label: "Install packages: npm install".to_owned(),
        };

        assert_eq!(
            event.message(),
            "treeboot: run Install packages: npm install"
        );
    }

    #[test]
    fn message_should_format_command_allowed_failure() {
        let event = OutputEvent::CommandAllowedFailure {
            label: "lint".to_owned(),
            reason: "failed with exit status: 1".to_owned(),
        };

        assert_eq!(
            event.message(),
            "treeboot: warning: command lint failed with exit status: 1"
        );
    }
}
