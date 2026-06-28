use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::{FileOperationKind, FileOperationSummary, MetadataField};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FileAction {
    CreateDirectory {
        operation: FileOperationKind,
        source: PathBuf,
        target: PathBuf,
        target_path: PathBuf,
    },
    CopyFile {
        operation: FileOperationKind,
        source: PathBuf,
        target: PathBuf,
        source_path: PathBuf,
        target_path: PathBuf,
        metadata_policy: MetadataPolicy,
        replace: bool,
    },
    RepairMetadata {
        operation: FileOperationKind,
        source: PathBuf,
        target: PathBuf,
        source_path: PathBuf,
        target_path: PathBuf,
        metadata_policy: MetadataPolicy,
        target_kind: MetadataTarget,
        report: bool,
    },
    CreateSymlink {
        operation: FileOperationKind,
        source: PathBuf,
        target: PathBuf,
        target_path: PathBuf,
        preserved_source_path: Option<PathBuf>,
        link_target: PathBuf,
        final_target: PathBuf,
        target_is_dir: bool,
        replace: bool,
    },
    Delete {
        target: PathBuf,
        target_path: PathBuf,
    },
    Skip {
        operation: FileOperationKind,
        target: PathBuf,
        reason: String,
    },
    Warning {
        path: PathBuf,
        reason: String,
    },
}

impl FileAction {
    pub(crate) fn counts(&self) -> bool {
        !matches!(
            self,
            Self::RepairMetadata { report: false, .. } | Self::Warning { .. }
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MetadataPolicy {
    pub(crate) permissions: bool,
    pub(crate) owner: bool,
    pub(crate) group: bool,
}

impl MetadataPolicy {
    pub(crate) fn from_ignored(fields: &[MetadataField]) -> Self {
        Self {
            permissions: !fields.contains(&MetadataField::Permissions),
            owner: !fields.contains(&MetadataField::Owner),
            group: !fields.contains(&MetadataField::Group),
        }
    }
}

impl Default for MetadataPolicy {
    fn default() -> Self {
        Self {
            permissions: true,
            owner: true,
            group: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MetadataTarget {
    File,
    Directory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlannedFileOperationActions {
    pub(crate) operation: FileOperationKind,
    pub(crate) source: PathBuf,
    pub(crate) target: PathBuf,
    pub(crate) expanded: bool,
    pub(crate) actions: Vec<FileAction>,
}

impl PlannedFileOperationActions {
    pub(crate) fn progress_action_count(&self) -> usize {
        self.actions.iter().filter(|action| action.counts()).count()
    }

    pub(crate) fn summary(&self) -> FileOperationSummary {
        summarize_actions(&self.actions, self.expanded)
    }
}

pub(crate) fn add_symlink_warnings(groups: &mut [PlannedFileOperationActions]) {
    let created_paths = groups
        .iter()
        .flat_map(|group| group.actions.iter())
        .filter_map(|action| match action {
            FileAction::CreateDirectory { target_path, .. }
            | FileAction::CopyFile { target_path, .. }
            | FileAction::CreateSymlink { target_path, .. } => Some(target_path.clone()),
            FileAction::RepairMetadata { .. }
            | FileAction::Delete { .. }
            | FileAction::Skip { .. }
            | FileAction::Warning { .. } => None,
        })
        .collect::<BTreeSet<_>>();
    for group in groups {
        let warnings = group
            .actions
            .iter()
            .filter_map(|action| match action {
                FileAction::CreateSymlink {
                    target,
                    final_target,
                    ..
                } if !final_target.exists() && !created_paths.contains(final_target) => {
                    Some(FileAction::Warning {
                        path: target.clone(),
                        reason: "symlink target does not exist".to_owned(),
                    })
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        group.actions.extend(warnings);
    }
}

fn summarize_actions(actions: &[FileAction], expanded: bool) -> FileOperationSummary {
    let mut summary = FileOperationSummary {
        expanded,
        ..FileOperationSummary::default()
    };

    for action in actions {
        match action {
            FileAction::CreateDirectory { .. }
            | FileAction::CopyFile { .. }
            | FileAction::CreateSymlink { .. } => summary.changed += 1,
            FileAction::RepairMetadata { report: true, .. } => {
                summary.changed += 1;
                summary.metadata_changed += 1;
            }
            FileAction::RepairMetadata { report: false, .. } => {}
            FileAction::Delete { .. } => summary.deleted += 1,
            FileAction::Skip { reason, .. } => {
                summary.skipped += 1;
                if summary.skip_reason.is_none() {
                    summary.skip_reason = Some(reason.clone());
                }
            }
            FileAction::Warning { .. } => summary.warnings += 1,
        }
    }

    summary
}
