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
        include: Vec::new(),
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

fn action_targets(actions: &[FileAction]) -> Vec<String> {
    actions
        .iter()
        .map(|action| display_target(action).replace('\\', "/"))
        .collect()
}

fn display_target(action: &FileAction) -> String {
    match action {
        FileAction::CopyFile { target, .. } => format!("copy {}", target.display()),
        FileAction::CreateDirectory { target, .. } => format!("mkdir {}", target.display()),
        FileAction::CreateSymlink { target, .. } => format!("symlink {}", target.display()),
        FileAction::RepairMetadata { target, .. } => format!("metadata {}", target.display()),
        FileAction::Delete { target, .. } => format!("delete {}", target.display()),
        FileAction::Skip { target, .. } => format!("skip {}", target.display()),
        FileAction::Warning { path, .. } => format!("warning {}", path.display()),
    }
}

#[test]
fn include_should_narrow_directory_copy_to_matching_paths() {
    let (root, worktree) = temp_workspace("include-narrow");
    fs::create_dir_all(root.join("shared/docs")).expect("docs should be created");
    fs::create_dir_all(root.join("shared/src")).expect("src should be created");
    fs::write(root.join("shared/docs/guide.md"), "guide\n").expect("file should be written");
    fs::write(root.join("shared/src/main.rs"), "fn main() {}\n").expect("file should be written");
    fs::write(root.join("shared/README"), "readme\n").expect("file should be written");
    let operation = operation(
        FileOperationKind::Copy,
        &root,
        &worktree,
        "shared",
        "shared",
    )
    .with_include(vec!["docs/**".to_owned()]);
    let plan = plan(&root, &worktree, vec![operation.clone()]);

    let group = plan_file_operation_group(&plan, &operation, FilePlanningOptions::default())
        .expect("include copy should plan");

    let targets = action_targets(&group.actions);
    assert!(targets.contains(&"copy shared/docs/guide.md".to_owned()));
    assert!(targets.contains(&"mkdir shared/docs".to_owned()));
    assert!(targets.contains(&"mkdir shared".to_owned()));
    assert!(
        !targets
            .iter()
            .any(|t| t.contains("src") || t.contains("README")),
        "non-included paths should get no actions: {targets:?}"
    );
}

#[test]
fn include_directory_match_should_include_whole_subtree() {
    let (root, worktree) = temp_workspace("include-subtree");
    fs::create_dir_all(root.join("shared/docs/nested")).expect("nested should be created");
    fs::write(root.join("shared/docs/nested/deep.md"), "deep\n").expect("file should be written");
    let operation = operation(
        FileOperationKind::Copy,
        &root,
        &worktree,
        "shared",
        "shared",
    )
    .with_include(vec!["docs".to_owned()]);
    let plan = plan(&root, &worktree, vec![operation.clone()]);

    let group = plan_file_operation_group(&plan, &operation, FilePlanningOptions::default())
        .expect("include copy should plan");

    assert!(action_targets(&group.actions).contains(&"copy shared/docs/nested/deep.md".to_owned()));
}

#[test]
fn include_should_create_included_empty_directory() {
    let (root, worktree) = temp_workspace("include-empty-dir");
    fs::create_dir_all(root.join("shared/empty")).expect("empty dir should be created");
    fs::write(root.join("shared/other.txt"), "other\n").expect("file should be written");
    let operation = operation(
        FileOperationKind::Copy,
        &root,
        &worktree,
        "shared",
        "shared",
    )
    .with_include(vec!["empty/".to_owned()]);
    let plan = plan(&root, &worktree, vec![operation.clone()]);

    let group = plan_file_operation_group(&plan, &operation, FilePlanningOptions::default())
        .expect("include copy should plan");

    let targets = action_targets(&group.actions);
    assert!(targets.contains(&"mkdir shared/empty".to_owned()));
    assert!(!targets.iter().any(|t| t.contains("other.txt")));
}

#[test]
fn include_should_not_filter_top_level_file_source() {
    let (root, worktree) = temp_workspace("include-file-source");
    fs::write(root.join(".env"), "TOKEN=1\n").expect("source should be written");
    let operation = operation(FileOperationKind::Copy, &root, &worktree, ".env", ".env")
        .with_include(vec!["docs/**".to_owned()]);
    let plan = plan(&root, &worktree, vec![operation.clone()]);

    let group = plan_file_operation_group(&plan, &operation, FilePlanningOptions::default())
        .expect("file source copy should plan");

    assert!(matches!(
        &group.actions[..],
        [FileAction::CopyFile { target, .. }] if target == Path::new(".env")
    ));
}

#[test]
fn include_and_ignore_should_gate_independently() {
    let (root, worktree) = temp_workspace("include-ignore-gates");
    fs::create_dir_all(root.join("shared/docs")).expect("docs should be created");
    fs::create_dir_all(root.join("shared/src")).expect("src should be created");
    fs::write(root.join("shared/docs/keep.md"), "keep\n").expect("file should be written");
    fs::write(root.join("shared/docs/drop.log"), "drop\n").expect("file should be written");
    fs::write(root.join("shared/src/rescued.md"), "rescued\n").expect("file should be written");
    let operation = operation(
        FileOperationKind::Copy,
        &root,
        &worktree,
        "shared",
        "shared",
    )
    .with_include(vec!["docs/**".to_owned()])
    // The negation re-includes for the ignore gate, but src/rescued.md still
    // fails the include gate and must not be copied.
    .with_ignore(vec!["**/*.log".to_owned(), "!src/rescued.md".to_owned()]);
    let plan = plan(&root, &worktree, vec![operation.clone()]);

    let group = plan_file_operation_group(&plan, &operation, FilePlanningOptions::default())
        .expect("include copy should plan");

    let targets = action_targets(&group.actions);
    assert!(targets.contains(&"copy shared/docs/keep.md".to_owned()));
    assert!(!targets.iter().any(|t| t.contains("drop.log")));
    assert!(!targets.iter().any(|t| t.contains("rescued.md")));
}

#[test]
fn include_should_reject_invalid_glob_patterns() {
    let (root, worktree) = temp_workspace("include-invalid-glob");
    fs::create_dir_all(root.join("shared")).expect("source dir should be created");
    let operation = operation(
        FileOperationKind::Copy,
        &root,
        &worktree,
        "shared",
        "shared",
    )
    .with_include(vec!["{a,b".to_owned()]);
    let plan = plan(&root, &worktree, vec![operation.clone()]);

    let error = plan_file_operation_group(&plan, &operation, FilePlanningOptions::default())
        .expect_err("invalid include pattern should fail planning");

    assert!(error.to_string().contains("invalid include pattern"));
}

#[test]
fn include_should_reject_inert_patterns_during_planning() {
    let (root, worktree) = temp_workspace("include-inert-patterns");
    fs::create_dir_all(root.join("shared")).expect("source dir should be created");

    for (pattern, message) in [
        ("!docs", "negation"),
        ("", "blank"),
        ("# comment", "comment"),
    ] {
        let operation = operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            "shared",
            "shared",
        )
        .with_include(vec![pattern.to_owned()]);
        let plan = plan(&root, &worktree, vec![operation.clone()]);

        let error = plan_file_operation_group(&plan, &operation, FilePlanningOptions::default())
            .expect_err("inert include pattern should fail planning");

        assert!(
            error.to_string().contains(message),
            "pattern {pattern:?} should mention {message}: {error}"
        );
    }
}

#[cfg(unix)]
#[test]
fn include_should_not_read_pruned_directories() {
    use std::os::unix::fs::PermissionsExt;

    let (root, worktree) = temp_workspace("include-pruned-unreadable");
    fs::create_dir_all(root.join("shared/docs")).expect("docs should be created");
    fs::create_dir_all(root.join("shared/secret")).expect("secret should be created");
    fs::write(root.join("shared/docs/guide.md"), "guide\n").expect("file should be written");
    fs::set_permissions(
        root.join("shared/secret"),
        fs::Permissions::from_mode(0o000),
    )
    .expect("permissions should change");

    let filtered = operation(
        FileOperationKind::Copy,
        &root,
        &worktree,
        "shared",
        "shared",
    )
    .with_include(vec!["docs/**".to_owned()]);
    let filtered_plan = plan(&root, &worktree, vec![filtered.clone()]);
    let result =
        plan_file_operation_group(&filtered_plan, &filtered, FilePlanningOptions::default());

    let unfiltered = operation(
        FileOperationKind::Copy,
        &root,
        &worktree,
        "shared",
        "shared",
    );
    let unfiltered_plan = plan(&root, &worktree, vec![unfiltered.clone()]);
    let unfiltered_result = plan_file_operation_group(
        &unfiltered_plan,
        &unfiltered,
        FilePlanningOptions::default(),
    );

    fs::set_permissions(
        root.join("shared/secret"),
        fs::Permissions::from_mode(0o755),
    )
    .expect("permissions should restore");

    let group = result.expect("plan should prune the unreadable non-viable directory");
    assert!(action_targets(&group.actions).contains(&"copy shared/docs/guide.md".to_owned()));
    unfiltered_result.expect_err("unfiltered plan should fail reading the directory");
}

#[cfg(unix)]
#[test]
fn sync_should_repair_drifted_ancestor_metadata_for_unchanged_included_descendant() {
    use std::os::unix::fs::PermissionsExt;

    let (root, worktree) = temp_workspace("include-ancestor-metadata");
    fs::create_dir_all(root.join("shared/a/b")).expect("source tree should be created");
    fs::write(root.join("shared/a/b/file"), "same\n").expect("source file should be written");
    fs::create_dir_all(worktree.join("shared/a/b")).expect("target tree should be created");
    fs::write(worktree.join("shared/a/b/file"), "same\n").expect("target file should be written");
    fs::set_permissions(root.join("shared/a"), fs::Permissions::from_mode(0o755))
        .expect("source permissions should change");
    fs::set_permissions(worktree.join("shared/a"), fs::Permissions::from_mode(0o777))
        .expect("target permissions should change");

    let operation = operation(
        FileOperationKind::Sync,
        &root,
        &worktree,
        "shared",
        "shared",
    )
    .with_compare(Some(crate::SyncCompare::Checksum))
    .with_include(vec!["a/b/file".to_owned()]);
    let plan = plan(&root, &worktree, vec![operation.clone()]);

    let group = plan_file_operation_group(&plan, &operation, FilePlanningOptions::default())
        .expect("include sync should plan");

    let targets = action_targets(&group.actions);
    assert!(
        targets.contains(&"metadata shared/a".to_owned()),
        "drifted ancestor should be repaired: {targets:?}"
    );
    assert!(!targets.iter().any(|t| t.starts_with("copy")));
}

#[test]
fn include_matched_but_ignored_files_should_not_materialize_parents() {
    let (root, worktree) = temp_workspace("include-ignored-no-scaffold");
    fs::create_dir_all(root.join("shared/data")).expect("data should be created");
    fs::create_dir_all(root.join("shared/docs")).expect("docs should be created");
    fs::write(root.join("shared/data/cache.log"), "log\n").expect("file should be written");
    fs::write(root.join("shared/docs/guide.md"), "guide\n").expect("file should be written");
    let operation = operation(
        FileOperationKind::Copy,
        &root,
        &worktree,
        "shared",
        "shared",
    )
    // cache.log matches include but is removed by ignore, so it is not in
    // scope and must not materialize its parent directory.
    .with_include(vec!["**/*.log".to_owned(), "docs/**".to_owned()])
    .with_ignore(vec!["**/*.log".to_owned()]);
    let plan = plan(&root, &worktree, vec![operation.clone()]);

    let group = plan_file_operation_group(&plan, &operation, FilePlanningOptions::default())
        .expect("include copy should plan");

    let targets = action_targets(&group.actions);
    assert!(targets.contains(&"copy shared/docs/guide.md".to_owned()));
    assert!(
        !targets.iter().any(|t| t.contains("shared/data")),
        "parent of an include-matched but ignored file should not be created: {targets:?}"
    );
}

#[cfg(unix)]
#[test]
fn include_should_not_traverse_non_viable_ignored_directories_with_negation() {
    use std::os::unix::fs::PermissionsExt;

    let (root, worktree) = temp_workspace("include-ignored-negation-pruned");
    fs::create_dir_all(root.join("shared/docs")).expect("docs should be created");
    fs::create_dir_all(root.join("shared/blocked")).expect("blocked should be created");
    fs::write(root.join("shared/docs/guide.md"), "guide\n").expect("file should be written");
    fs::set_permissions(
        root.join("shared/blocked"),
        fs::Permissions::from_mode(0o000),
    )
    .expect("permissions should change");

    let operation = operation(
        FileOperationKind::Copy,
        &root,
        &worktree,
        "shared",
        "shared",
    )
    .with_include(vec!["docs/**".to_owned()])
    // The negation forces conservative traversal of ignored directories, but
    // the include gate cannot pass under `blocked`, so it must stay pruned.
    .with_ignore(vec!["blocked/".to_owned(), "!blocked/keep".to_owned()]);
    let plan = plan(&root, &worktree, vec![operation.clone()]);

    let result = plan_file_operation_group(&plan, &operation, FilePlanningOptions::default());

    fs::set_permissions(
        root.join("shared/blocked"),
        fs::Permissions::from_mode(0o755),
    )
    .expect("permissions should restore");

    let group = result.expect("non-viable ignored directory should not be traversed");
    assert!(action_targets(&group.actions).contains(&"copy shared/docs/guide.md".to_owned()));
}
