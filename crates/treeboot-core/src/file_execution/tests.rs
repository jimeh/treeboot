use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;
use crate::file_actions::{
    FileAction, MetadataPolicy, MetadataTarget, PlannedFileOperationActions,
};
use crate::{ActionPlan, FileOperationKind, OutputEvent, PlanOrigin, Reporter, Worktree};

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

fn temp_workspace(name: &str) -> (PathBuf, PathBuf) {
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after Unix epoch")
        .as_nanos();
    let base = std::env::temp_dir().join(format!("treeboot-file-execution-{name}-{id}"));
    let root = base.join("root");
    let worktree = base.join("worktree");

    fs::create_dir_all(&root).expect("root should be created");
    fs::create_dir_all(&worktree).expect("worktree should be created");

    (root, worktree)
}

fn context(root_path: &Path, worktree_path: &Path) -> Worktree {
    Worktree {
        root_path: root_path.to_path_buf(),
        worktree_path: worktree_path.to_path_buf(),
        default_branch: "main".to_owned(),
        environment: BTreeMap::from([("TREEBOOT_ROOT_PATH".to_owned(), OsString::from(root_path))]),
    }
}

fn plan(root: &Path, worktree: &Path) -> ActionPlan {
    ActionPlan::from_parts_unchecked(
        context(root, worktree),
        PlanOrigin::Manifest {
            path: worktree.join(".treeboot.toml"),
        },
        Some(worktree.join(".treeboot.toml")),
        Vec::new(),
        Vec::new(),
    )
}

fn group(actions: Vec<FileAction>) -> PlannedFileOperationActions {
    PlannedFileOperationActions {
        operation: FileOperationKind::Copy,
        source: PathBuf::from("source"),
        target: PathBuf::from("target"),
        expanded: true,
        actions,
    }
}

#[test]
fn execute_file_operation_group_should_report_verbose_dry_run_actions() {
    let (root, worktree) = temp_workspace("verbose-dry-run");
    let plan = plan(&root, &worktree);
    let actions = group(vec![
        FileAction::CreateDirectory {
            operation: FileOperationKind::Copy,
            source: PathBuf::from("shared"),
            target: PathBuf::from("shared"),
            target_path: worktree.join("shared"),
        },
        FileAction::CopyFile {
            operation: FileOperationKind::Copy,
            source: PathBuf::from(".env"),
            target: PathBuf::from(".env"),
            source_path: root.join(".env"),
            target_path: worktree.join(".env"),
            metadata_policy: MetadataPolicy::default(),
            replace: false,
        },
        FileAction::CreateSymlink {
            operation: FileOperationKind::Symlink,
            source: PathBuf::from("tool"),
            target: PathBuf::from(".tool"),
            target_path: worktree.join(".tool"),
            preserved_source_path: None,
            link_target: PathBuf::from("tool"),
            final_target: root.join("tool"),
            target_is_dir: false,
            replace: false,
        },
        FileAction::RepairMetadata {
            operation: FileOperationKind::Sync,
            source: PathBuf::from("source"),
            target: PathBuf::from("target"),
            source_path: root.join("source"),
            target_path: worktree.join("target"),
            metadata_policy: MetadataPolicy::default(),
            target_kind: MetadataTarget::File,
            report: true,
        },
        FileAction::RepairMetadata {
            operation: FileOperationKind::Sync,
            source: PathBuf::from("quiet-source"),
            target: PathBuf::from("quiet-target"),
            source_path: root.join("quiet-source"),
            target_path: worktree.join("quiet-target"),
            metadata_policy: MetadataPolicy::default(),
            target_kind: MetadataTarget::File,
            report: false,
        },
        FileAction::Delete {
            target: PathBuf::from("old"),
            target_path: worktree.join("old"),
        },
        FileAction::Skip {
            operation: FileOperationKind::Copy,
            target: PathBuf::from("existing"),
            reason: "target exists".to_owned(),
        },
        FileAction::Warning {
            path: PathBuf::from("link"),
            reason: "symlink target does not exist".to_owned(),
        },
    ]);
    let mut reporter = VecReporter::default();

    let action_count = execute_file_operation_group(
        &plan,
        &actions,
        FileExecutionOptions {
            dry_run: true,
            verbose: true,
        },
        &mut reporter,
    )
    .expect("dry-run execution should report");

    assert_eq!(action_count, 6);
    assert_eq!(reporter.events.len(), 7);
    assert!(matches!(
        reporter.events[0],
        OutputEvent::FileWouldApply { .. }
    ));
    assert!(matches!(
        reporter.events[1],
        OutputEvent::FileWouldApply { .. }
    ));
    assert!(matches!(
        reporter.events[2],
        OutputEvent::FileWouldApply { .. }
    ));
    assert!(matches!(
        reporter.events[3],
        OutputEvent::FileMetadataWouldApply { .. }
    ));
    assert!(matches!(
        reporter.events[4],
        OutputEvent::FileWouldDelete { .. }
    ));
    assert!(matches!(
        reporter.events[5],
        OutputEvent::FileWouldSkip { .. }
    ));
    assert!(matches!(
        reporter.events[6],
        OutputEvent::FileWarning { .. }
    ));
}

#[test]
fn execute_file_operation_group_should_apply_verbose_file_actions() {
    let (root, worktree) = temp_workspace("verbose-apply");
    fs::write(root.join(".env"), "TOKEN=1\n").expect("source should be written");
    let plan = plan(&root, &worktree);
    let actions = group(vec![
        FileAction::CreateDirectory {
            operation: FileOperationKind::Copy,
            source: PathBuf::from("shared"),
            target: PathBuf::from("shared"),
            target_path: worktree.join("shared"),
        },
        FileAction::CopyFile {
            operation: FileOperationKind::Copy,
            source: PathBuf::from(".env"),
            target: PathBuf::from(".env"),
            source_path: root.join(".env"),
            target_path: worktree.join(".env"),
            metadata_policy: MetadataPolicy::default(),
            replace: false,
        },
        FileAction::CreateSymlink {
            operation: FileOperationKind::Symlink,
            source: PathBuf::from(".env"),
            target: PathBuf::from(".env.link"),
            target_path: worktree.join(".env.link"),
            preserved_source_path: None,
            link_target: PathBuf::from(".env"),
            final_target: worktree.join(".env"),
            target_is_dir: false,
            replace: false,
        },
    ]);
    let mut reporter = VecReporter::default();

    let action_count = execute_file_operation_group(
        &plan,
        &actions,
        FileExecutionOptions {
            dry_run: false,
            verbose: true,
        },
        &mut reporter,
    )
    .expect("verbose execution should apply actions");

    assert_eq!(action_count, 3);
    assert_eq!(
        fs::read_to_string(worktree.join(".env")).unwrap(),
        "TOKEN=1\n"
    );
    assert_eq!(reporter.events.len(), 3);
    assert!(
        reporter
            .events
            .iter()
            .all(|event| matches!(event, OutputEvent::FileApplied { .. }))
    );
}

#[test]
fn execute_file_operation_group_should_report_verbose_metadata_delete_skip_and_warning() {
    let (root, worktree) = temp_workspace("verbose-other-actions");
    fs::write(root.join("source"), "source\n").expect("source should be written");
    fs::write(worktree.join("target"), "target\n").expect("target should be written");
    fs::write(worktree.join("old"), "old\n").expect("delete target should be written");
    let plan = plan(&root, &worktree);
    let actions = group(vec![
        FileAction::RepairMetadata {
            operation: FileOperationKind::Sync,
            source: PathBuf::from("source"),
            target: PathBuf::from("target"),
            source_path: root.join("source"),
            target_path: worktree.join("target"),
            metadata_policy: MetadataPolicy::default(),
            target_kind: MetadataTarget::File,
            report: true,
        },
        FileAction::RepairMetadata {
            operation: FileOperationKind::Sync,
            source: PathBuf::from("source"),
            target: PathBuf::from("target"),
            source_path: root.join("source"),
            target_path: worktree.join("target"),
            metadata_policy: MetadataPolicy::default(),
            target_kind: MetadataTarget::File,
            report: false,
        },
        FileAction::Delete {
            target: PathBuf::from("old"),
            target_path: worktree.join("old"),
        },
        FileAction::Skip {
            operation: FileOperationKind::Copy,
            target: PathBuf::from("existing"),
            reason: "target exists".to_owned(),
        },
        FileAction::Warning {
            path: PathBuf::from("link"),
            reason: "symlink target does not exist".to_owned(),
        },
    ]);
    let mut reporter = VecReporter::default();

    let action_count = execute_file_operation_group(
        &plan,
        &actions,
        FileExecutionOptions {
            dry_run: false,
            verbose: true,
        },
        &mut reporter,
    )
    .expect("verbose execution should report non-apply actions");

    assert_eq!(action_count, 3);
    assert!(!worktree.join("old").exists());
    assert_eq!(reporter.events.len(), 4);
    assert!(matches!(
        reporter.events[0],
        OutputEvent::FileMetadataApplied { .. }
    ));
    assert!(matches!(
        reporter.events[1],
        OutputEvent::FileDeleted { .. }
    ));
    assert!(matches!(
        reporter.events[2],
        OutputEvent::FileSkipped { .. }
    ));
    assert!(matches!(
        reporter.events[3],
        OutputEvent::FileWarning { .. }
    ));
}

#[cfg(unix)]
#[test]
fn execute_file_operation_group_should_reject_metadata_repair_through_symlink_parent() {
    let (root, worktree) = temp_workspace("metadata-symlink-parent");
    let outside = worktree
        .parent()
        .expect("worktree should have parent")
        .join("outside-metadata-repair");
    fs::create_dir_all(&outside).expect("outside dir should be created");
    fs::write(root.join("source"), "source\n").expect("source should be written");
    fs::write(outside.join("target"), "outside\n").expect("outside target should be written");
    std::os::unix::fs::symlink(&outside, worktree.join("linked"))
        .expect("target parent symlink should be created");
    let plan = plan(&root, &worktree);
    let actions = group(vec![FileAction::RepairMetadata {
        operation: FileOperationKind::Sync,
        source: PathBuf::from("source"),
        target: PathBuf::from("linked/target"),
        source_path: root.join("source"),
        target_path: worktree.join("linked/target"),
        metadata_policy: MetadataPolicy::default(),
        target_kind: MetadataTarget::File,
        report: true,
    }]);
    let mut reporter = VecReporter::default();

    let error = execute_file_operation_group(
        &plan,
        &actions,
        FileExecutionOptions {
            dry_run: false,
            verbose: true,
        },
        &mut reporter,
    )
    .expect_err("metadata repair through a symlink parent should fail");

    assert!(error.to_string().contains("target parent is a symlink"));
    assert_eq!(
        fs::read_to_string(outside.join("target")).expect("outside target should remain readable"),
        "outside\n"
    );
}
