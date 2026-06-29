use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;
use crate::file_actions::FileAction;
use crate::validation::PlannedFileOperationParts;
use crate::{
    ActionPlan, Error, FileOperationKind, PlanOrigin, PlannedFileOperation, PlannedFileStatus,
    SourceSpan, Worktree,
};

fn span() -> SourceSpan {
    SourceSpan {
        start: 0,
        end: 1,
        line: 1,
        column: 1,
    }
}

fn temp_workspace(name: &str) -> (PathBuf, PathBuf) {
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after Unix epoch")
        .as_nanos();
    let base = std::env::temp_dir().join(format!("treeboot-file-planning-{name}-{id}"));
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

fn operation(
    operation: FileOperationKind,
    root: &Path,
    worktree: &Path,
    source: &str,
    target: &str,
) -> PlannedFileOperation {
    PlannedFileOperation::from_raw_parts_unchecked(PlannedFileOperationParts {
        operation,
        source: PathBuf::from(source),
        target: PathBuf::from(target),
        source_path: root.join(source),
        target_path: worktree.join(target),
        required: false,
        compare: None,
        delete: None,
        symlinks: None,
        ignore: Vec::new(),
        ignore_metadata: Vec::new(),
        status: PlannedFileStatus::Ready,
        declaration: span(),
    })
}

fn plan(root: &Path, worktree: &Path, files: Vec<PlannedFileOperation>) -> ActionPlan {
    ActionPlan::from_parts_unchecked(
        context(root, worktree),
        PlanOrigin::Manifest {
            path: worktree.join(".treeboot.toml"),
        },
        Some(worktree.join(".treeboot.toml")),
        files,
        Vec::new(),
    )
}

#[test]
fn plan_file_operation_group_should_plan_copy_skip_for_existing_target() {
    let (root, worktree) = temp_workspace("copy-skip");
    fs::write(root.join(".env"), "new\n").expect("source should be written");
    fs::write(worktree.join(".env"), "old\n").expect("target should be written");
    let operation = operation(FileOperationKind::Copy, &root, &worktree, ".env", ".env");
    let plan = plan(&root, &worktree, vec![operation.clone()]);

    let group = plan_file_operation_group(&plan, &operation, FilePlanningOptions::default())
        .expect("copy should plan");

    assert_eq!(group.actions.len(), 1);
    assert!(matches!(
        &group.actions[0],
        FileAction::Skip { target, reason, .. }
            if target == Path::new(".env") && reason == "target exists"
    ));
    assert_eq!(fs::read_to_string(worktree.join(".env")).unwrap(), "old\n");
}

#[test]
fn plan_file_operation_group_should_plan_forced_copy_replacement() {
    let (root, worktree) = temp_workspace("copy-force");
    fs::write(root.join(".env"), "new\n").expect("source should be written");
    fs::write(worktree.join(".env"), "old\n").expect("target should be written");
    let operation = operation(FileOperationKind::Copy, &root, &worktree, ".env", ".env");
    let plan = plan(&root, &worktree, vec![operation.clone()]);

    let group = plan_file_operation_group(
        &plan,
        &operation,
        FilePlanningOptions {
            force: true,
            ..FilePlanningOptions::default()
        },
    )
    .expect("forced copy should plan");

    assert!(matches!(
        &group.actions[..],
        [FileAction::CopyFile { target, replace: true, .. }]
            if target == Path::new(".env")
    ));
    assert_eq!(fs::read_to_string(worktree.join(".env")).unwrap(), "old\n");
}

#[test]
fn plan_file_operation_group_should_reject_invalid_ignore_pattern() {
    let (root, worktree) = temp_workspace("invalid-ignore");
    fs::create_dir_all(root.join("shared")).expect("source dir should be created");
    let operation = operation(
        FileOperationKind::Copy,
        &root,
        &worktree,
        "shared",
        "shared",
    )
    .with_ignore(vec!["{a,b".to_owned()]);
    let plan = plan(&root, &worktree, vec![operation.clone()]);

    let error = plan_file_operation_group(&plan, &operation, FilePlanningOptions::default())
        .expect_err("invalid ignore pattern should fail planning");

    assert!(matches!(
        error,
        Error::FileOperationInvalid {
            operation: "copy",
            ..
        }
    ));
}
