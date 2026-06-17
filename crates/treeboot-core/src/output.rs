use std::path::PathBuf;

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
            Self::InitCreated { path } => {
                format!("treeboot: created {}", path.display())
            }
        }
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
    fn message_should_format_root_worktree_detected() {
        let event = OutputEvent::RootWorktreeDetected;

        assert_eq!(event.message(), "treeboot: This is not a work tree");
    }
}
