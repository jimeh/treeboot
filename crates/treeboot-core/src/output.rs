use std::path::PathBuf;

use crate::FileOperationKind;

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
                operation_name(*operation),
                source.display(),
                target.display()
            ),
            Self::FileWouldApply {
                operation,
                source,
                target,
            } => format!(
                "treeboot: would {} {} -> {}",
                operation_name(*operation),
                source.display(),
                target.display()
            ),
            Self::FileSkipped {
                operation,
                target,
                reason,
            } => format!(
                "treeboot: skip {} {}; {}",
                operation_name(*operation),
                target.display(),
                reason
            ),
            Self::FileWouldSkip {
                operation,
                target,
                reason,
            } => format!(
                "treeboot: would skip {} {}; {}",
                operation_name(*operation),
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
            Self::InitCreated { path } => {
                format!("treeboot: created {}", path.display())
            }
        }
    }
}

fn operation_name(operation: FileOperationKind) -> &'static str {
    match operation {
        FileOperationKind::Copy => "copy",
        FileOperationKind::Symlink => "symlink",
        FileOperationKind::Sync => "sync",
    }
}

/// Receives structured output events from core operations.
pub trait Reporter {
    /// Handles one output event.
    fn report(&mut self, event: OutputEvent) -> std::io::Result<()>;
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
    fn message_should_format_root_worktree_detected() {
        let event = OutputEvent::RootWorktreeDetected;

        assert_eq!(event.message(), "treeboot: This is not a work tree");
    }
}
