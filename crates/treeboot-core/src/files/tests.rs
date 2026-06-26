use std::collections::BTreeMap;
use std::ffi::OsString;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::*;
use crate::{PlanOrigin, SourceSpan, Worktree};

#[derive(Default)]
struct VecReporter {
    events: Vec<OutputEvent>,
    messages: Vec<String>,
    planning_finished_counts: Vec<usize>,
    execution_started_counts: Vec<usize>,
    action_advanced_count: usize,
    summary_count: usize,
}

impl Reporter for VecReporter {
    fn report(&mut self, event: OutputEvent) -> std::io::Result<()> {
        let message = event.message();
        if !message.is_empty() {
            self.messages.push(message);
        }
        self.events.push(event);
        Ok(())
    }

    fn file_operation_planning_finished(
        &mut self,
        operation: FileOperationKind,
        source: &Path,
        target: &Path,
        action_count: usize,
    ) -> std::io::Result<()> {
        let _ = (operation, source, target);
        self.planning_finished_counts.push(action_count);
        Ok(())
    }

    fn file_operation_execution_started(
        &mut self,
        operation: FileOperationKind,
        source: &Path,
        target: &Path,
        action_count: usize,
    ) -> std::io::Result<()> {
        let _ = (operation, source, target);
        self.execution_started_counts.push(action_count);
        Ok(())
    }

    fn file_operation_action_advanced(
        &mut self,
        operation: FileOperationKind,
        source: &Path,
        target: &Path,
    ) -> std::io::Result<()> {
        let _ = (operation, source, target);
        self.action_advanced_count += 1;
        Ok(())
    }

    fn file_operation_finished(
        &mut self,
        operation: FileOperationKind,
        source: &Path,
        target: &Path,
        summary: &FileOperationSummary,
        dry_run: bool,
    ) -> std::io::Result<()> {
        self.summary_count += 1;
        self.messages
            .push(summary.message(operation, source, target, dry_run));
        Ok(())
    }
}

impl VecReporter {
    fn messages(&self) -> Vec<String> {
        self.messages.clone()
    }
}

#[derive(Debug, Clone, Copy)]
enum FailingCallback {
    PlanningStarted,
    PlanningFinished,
    ExecutionStarted,
    ActionAdvanced,
    Finished,
}

struct FailingCallbackReporter {
    fail_on: FailingCallback,
}

impl FailingCallbackReporter {
    fn fail() -> std::io::Result<()> {
        Err(std::io::Error::other("reporter callback failed"))
    }
}

impl Reporter for FailingCallbackReporter {
    fn report(&mut self, _event: OutputEvent) -> std::io::Result<()> {
        Ok(())
    }

    fn file_operation_planning_started(
        &mut self,
        operation: FileOperationKind,
        source: &Path,
        target: &Path,
    ) -> std::io::Result<()> {
        let _ = (operation, source, target);
        match self.fail_on {
            FailingCallback::PlanningStarted => Self::fail(),
            _ => Ok(()),
        }
    }

    fn file_operation_planning_finished(
        &mut self,
        operation: FileOperationKind,
        source: &Path,
        target: &Path,
        action_count: usize,
    ) -> std::io::Result<()> {
        let _ = (operation, source, target, action_count);
        match self.fail_on {
            FailingCallback::PlanningFinished => Self::fail(),
            _ => Ok(()),
        }
    }

    fn file_operation_execution_started(
        &mut self,
        operation: FileOperationKind,
        source: &Path,
        target: &Path,
        action_count: usize,
    ) -> std::io::Result<()> {
        let _ = (operation, source, target, action_count);
        match self.fail_on {
            FailingCallback::ExecutionStarted => Self::fail(),
            _ => Ok(()),
        }
    }

    fn file_operation_action_advanced(
        &mut self,
        operation: FileOperationKind,
        source: &Path,
        target: &Path,
    ) -> std::io::Result<()> {
        let _ = (operation, source, target);
        match self.fail_on {
            FailingCallback::ActionAdvanced => Self::fail(),
            _ => Ok(()),
        }
    }

    fn file_operation_finished(
        &mut self,
        operation: FileOperationKind,
        source: &Path,
        target: &Path,
        summary: &FileOperationSummary,
        dry_run: bool,
    ) -> std::io::Result<()> {
        let _ = (operation, source, target, summary, dry_run);
        match self.fail_on {
            FailingCallback::Finished => Self::fail(),
            _ => Ok(()),
        }
    }
}

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
    let base = std::env::temp_dir().join(format!("treeboot-files-{name}-{id}"));
    let root = base.join("root");
    let worktree = base.join("worktree");

    fs::create_dir_all(&root).expect("root should be created");
    fs::create_dir_all(&worktree).expect("worktree should be created");

    (root, worktree)
}

#[cfg(unix)]
fn short_temp_workspace(name: &str) -> (PathBuf, PathBuf) {
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after Unix epoch")
        .as_nanos();
    let base = PathBuf::from(format!("/tmp/tb-{name}-{}-{id}", std::process::id()));
    let root = base.join("r");
    let worktree = base.join("w");

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
    PlannedFileOperation {
        operation,
        source: PathBuf::from(source),
        target: PathBuf::from(target),
        source_path: root.join(source),
        target_path: worktree.join(target),
        required: false,
        compare: None,
        delete: None,
        symlinks: None,
        ignore_metadata: Vec::new(),
        status: PlannedFileStatus::Ready,
        declaration: span(),
    }
}

fn sync_operation(
    root: &Path,
    worktree: &Path,
    source: &str,
    target: &str,
) -> PlannedFileOperation {
    operation(FileOperationKind::Sync, root, worktree, source, target)
}

fn run_plan(root: &Path, worktree: &Path, files: Vec<PlannedFileOperation>) -> ActionPlan {
    ActionPlan {
        context: context(root, worktree),
        origin: PlanOrigin::Manifest {
            path: worktree.join(".treeboot.toml"),
        },
        config_path: Some(worktree.join(".treeboot.toml")),
        files,
        commands: Vec::new(),
    }
}

#[test]
fn apply_file_operations_should_copy_missing_directory_tree() {
    let (root, worktree) = temp_workspace("missing-directory-copy");
    let source_dir = root.join("shared/nested");
    fs::create_dir_all(&source_dir).expect("source dir should be created");
    fs::write(source_dir.join("config"), "value\n").expect("source should be written");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            "shared",
            "shared",
        )],
    );
    let mut reporter = VecReporter::default();

    let report = apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("directory tree should copy");

    let copied = fs::read_to_string(worktree.join("shared/nested/config"))
        .expect("nested target should be readable");
    assert_eq!(copied, "value\n");
    assert_eq!(report.action_count, 3);
    assert_eq!(
        reporter.messages(),
        ["treeboot: copy shared -> shared (3 changed)"]
    );
}

#[test]
fn apply_file_operations_should_map_callback_failures_to_output_errors() {
    for fail_on in [
        FailingCallback::PlanningStarted,
        FailingCallback::PlanningFinished,
        FailingCallback::ExecutionStarted,
        FailingCallback::ActionAdvanced,
        FailingCallback::Finished,
    ] {
        let (root, worktree) = temp_workspace(&format!("callback-failure-{fail_on:?}"));
        fs::write(root.join(".env"), "TOKEN=1\n").expect("source should be written");
        let plan = run_plan(
            &root,
            &worktree,
            vec![operation(
                FileOperationKind::Copy,
                &root,
                &worktree,
                ".env",
                ".env",
            )],
        );
        let mut reporter = FailingCallbackReporter { fail_on };

        let error = apply_file_operations(
            &plan,
            FileApplyOptions {
                dry_run: true,
                ..FileApplyOptions::default()
            },
            &mut reporter,
        )
        .expect_err("callback failure should fail apply");

        assert!(
            matches!(error, Error::Output { .. }),
            "{fail_on:?} should map to Error::Output"
        );
        assert!(
            !worktree.join(".env").exists(),
            "{fail_on:?} should not mutate during dry-run"
        );
    }
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_copy_read_only_directory_children_before_metadata() {
    use std::os::unix::fs::PermissionsExt;

    let (root, worktree) = temp_workspace("read-only-directory-copy");
    let source = root.join("shared");
    let target = worktree.join("shared");
    fs::create_dir_all(&source).expect("source dir should be created");
    fs::write(source.join("config"), "value\n").expect("source child should be written");
    let mut source_permissions = fs::metadata(&source)
        .expect("source metadata should be readable")
        .permissions();
    source_permissions.set_mode(0o555);
    fs::set_permissions(&source, source_permissions).expect("source mode should be set");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            "shared",
            "shared",
        )],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("read-only directory should copy after children");

    let copied =
        fs::read_to_string(target.join("config")).expect("target child should be readable");
    let mode = fs::metadata(&target)
        .expect("target metadata should be readable")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(copied, "value\n");
    assert_eq!(mode, 0o555);
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_force_copy_child_inside_existing_read_only_directory() {
    use std::os::unix::fs::PermissionsExt;

    let (root, worktree) = temp_workspace("read-only-directory-force-copy-child");
    let source = root.join("shared");
    let target = worktree.join("shared");
    fs::create_dir_all(&source).expect("source dir should be created");
    fs::create_dir_all(&target).expect("target dir should be created");
    fs::write(source.join("config"), "new\n").expect("source child should be written");
    fs::write(target.join("config"), "old\n").expect("target child should be written");
    let mut source_permissions = fs::metadata(&source)
        .expect("source metadata should be readable")
        .permissions();
    source_permissions.set_mode(0o555);
    fs::set_permissions(&source, source_permissions).expect("source mode should be set");
    let mut target_permissions = fs::metadata(&target)
        .expect("target metadata should be readable")
        .permissions();
    target_permissions.set_mode(0o555);
    fs::set_permissions(&target, target_permissions).expect("target mode should be set");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            "shared",
            "shared",
        )],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(
        &plan,
        FileApplyOptions {
            force: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect("force copy should replace child inside read-only directory");

    let copied =
        fs::read_to_string(target.join("config")).expect("target child should be readable");
    let mode = fs::metadata(&target)
        .expect("target metadata should be readable")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(copied, "new\n");
    assert_eq!(mode, 0o555);
}

#[test]
fn apply_file_operations_verbose_should_report_concrete_directory_actions() {
    let (root, worktree) = temp_workspace("verbose-directory-copy");
    let source_dir = root.join("shared/nested");
    fs::create_dir_all(&source_dir).expect("source dir should be created");
    fs::write(source_dir.join("config"), "value\n").expect("source should be written");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            "shared",
            "shared",
        )],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(
        &plan,
        FileApplyOptions {
            verbose: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect("directory tree should copy");

    assert!(reporter.events.iter().any(|event| {
        matches!(
            event,
            OutputEvent::FileApplied {
                operation: FileOperationKind::Copy,
                target,
                ..
            } if target == Path::new("shared/nested/config")
        )
    }));
    assert_eq!(reporter.summary_count, 0);
}

#[test]
fn apply_file_operations_should_copy_missing_directory_files_only() {
    let (root, worktree) = temp_workspace("directory-copy");
    let source_dir = root.join("shared");
    let target_dir = worktree.join("shared");
    fs::create_dir_all(&source_dir).expect("source dir should be created");
    fs::create_dir_all(&target_dir).expect("target dir should be created");
    fs::write(source_dir.join("existing"), "new\n").expect("source should be written");
    fs::write(source_dir.join("missing"), "value\n").expect("source should be written");
    fs::write(target_dir.join("existing"), "old\n").expect("target should be written");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            "shared",
            "shared",
        )],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("directory copy should apply");

    let existing = fs::read_to_string(target_dir.join("existing"))
        .expect("existing target should be readable");
    let missing =
        fs::read_to_string(target_dir.join("missing")).expect("missing target should be copied");
    assert_eq!(existing, "old\n");
    assert_eq!(missing, "value\n");
}

#[test]
fn apply_file_operations_should_force_copy_directory_files_without_deleting_extras() {
    let (root, worktree) = temp_workspace("force-directory-copy");
    let source_dir = root.join("shared");
    let target_dir = worktree.join("shared");
    fs::create_dir_all(&source_dir).expect("source dir should be created");
    fs::create_dir_all(&target_dir).expect("target dir should be created");
    fs::write(source_dir.join("existing"), "new\n").expect("source should be written");
    fs::write(target_dir.join("existing"), "old\n").expect("target should be written");
    fs::write(target_dir.join("extra"), "keep\n").expect("extra target should be written");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            "shared",
            "shared",
        )],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(
        &plan,
        FileApplyOptions {
            force: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect("forced directory copy should apply");

    let existing = fs::read_to_string(target_dir.join("existing"))
        .expect("existing target should be readable");
    let extra = fs::read_to_string(target_dir.join("extra")).expect("extra target should remain");
    assert_eq!(existing, "new\n");
    assert_eq!(extra, "keep\n");
}

#[test]
fn apply_file_operations_should_reject_existing_directory_in_strict_before_children() {
    let (root, worktree) = temp_workspace("strict-directory-copy");
    let source_dir = root.join("shared");
    let target_dir = worktree.join("shared");
    fs::create_dir_all(&source_dir).expect("source dir should be created");
    fs::create_dir_all(&target_dir).expect("target dir should be created");
    fs::write(source_dir.join("missing"), "value\n").expect("source should be written");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            "shared",
            "shared",
        )],
    );
    let mut reporter = VecReporter::default();

    let error = apply_file_operations(
        &plan,
        FileApplyOptions {
            strict: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect_err("strict directory conflict should fail");

    assert!(error.to_string().contains("target directory exists"));
    assert!(!target_dir.join("missing").exists());
}

#[test]
fn apply_file_operations_should_reject_file_to_directory_target() {
    let (root, worktree) = temp_workspace("file-to-directory");
    fs::write(root.join(".env"), "TOKEN=1\n").expect("source should be written");
    fs::create_dir_all(worktree.join(".env")).expect("target dir should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            ".env",
            ".env",
        )],
    );
    let mut reporter = VecReporter::default();

    let error = apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect_err("file to directory should fail");

    assert!(error.to_string().contains("target is a directory"));
}

#[test]
fn apply_file_operations_should_reject_file_to_directory_target_even_with_force() {
    let (root, worktree) = temp_workspace("force-file-to-directory");
    fs::write(root.join(".env"), "TOKEN=1\n").expect("source should be written");
    fs::create_dir_all(worktree.join(".env")).expect("target dir should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            ".env",
            ".env",
        )],
    );
    let mut reporter = VecReporter::default();

    let error = apply_file_operations(
        &plan,
        FileApplyOptions {
            force: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect_err("force should not replace directory with file");

    assert!(error.to_string().contains("target is a directory"));
    assert!(worktree.join(".env").is_dir());
}

#[test]
fn apply_file_operations_should_reject_file_to_directory_target_in_dry_run() {
    let (root, worktree) = temp_workspace("dry-run-file-to-directory");
    fs::write(root.join(".env"), "TOKEN=1\n").expect("source should be written");
    fs::create_dir_all(worktree.join(".env")).expect("target dir should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            ".env",
            ".env",
        )],
    );
    let mut reporter = VecReporter::default();

    let error = apply_file_operations(
        &plan,
        FileApplyOptions {
            dry_run: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect_err("dry-run should report file-to-directory conflict");

    assert!(error.to_string().contains("target is a directory"));
    assert!(reporter.messages().is_empty());
}

#[test]
fn apply_file_operations_should_reject_directory_to_file_target() {
    let (root, worktree) = temp_workspace("directory-to-file");
    fs::create_dir_all(root.join("shared")).expect("source dir should be created");
    fs::write(worktree.join("shared"), "old\n").expect("target file should be written");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            "shared",
            "shared",
        )],
    );
    let mut reporter = VecReporter::default();

    let error = apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect_err("directory to file should fail");

    assert!(error.to_string().contains("target is a file or symlink"));
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_reject_directory_to_symlink_target() {
    let (root, worktree) = temp_workspace("directory-to-symlink");
    fs::create_dir_all(root.join("shared")).expect("source dir should be created");
    fs::write(worktree.join("real-target"), "old\n").expect("target file should be written");
    std::os::unix::fs::symlink("real-target", worktree.join("shared"))
        .expect("target symlink should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            "shared",
            "shared",
        )],
    );
    let mut reporter = VecReporter::default();

    let error = apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect_err("directory to symlink should fail");

    assert!(error.to_string().contains("target is a file or symlink"));
    assert_eq!(
        fs::read_link(worktree.join("shared")).expect("target should remain a symlink"),
        PathBuf::from("real-target")
    );
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_reject_unsupported_source_file_type() {
    use std::os::unix::net::UnixListener;

    let (root, worktree) = short_temp_workspace("us");
    let socket_path = root.join("socket");
    let _listener = UnixListener::bind(&socket_path).expect("source socket should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            "socket",
            "socket",
        )],
    );
    let mut reporter = VecReporter::default();

    let error = apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect_err("unsupported source should fail");

    assert!(
        error
            .to_string()
            .contains("source file type is unsupported")
    );
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_reject_unsupported_directory_target_file_type() {
    use std::os::unix::net::UnixListener;

    let (root, worktree) = short_temp_workspace("ud");
    fs::create_dir_all(root.join("shared")).expect("source dir should be created");
    let socket_path = worktree.join("shared");
    let _listener = UnixListener::bind(&socket_path).expect("target socket should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            "shared",
            "shared",
        )],
    );
    let mut reporter = VecReporter::default();

    let error = apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect_err("unsupported target should fail");

    assert!(
        error
            .to_string()
            .contains("target file type is unsupported")
    );
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_reject_unsupported_sync_file_target_type() {
    use std::os::unix::net::UnixListener;

    let (root, worktree) = short_temp_workspace("ut");
    fs::write(root.join(".env"), "TOKEN=1\n").expect("source should be written");
    let socket_path = worktree.join(".env");
    let _listener = UnixListener::bind(&socket_path).expect("target socket should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(&root, &worktree, ".env", ".env")],
    );
    let mut reporter = VecReporter::default();

    let error = apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect_err("unsupported sync target should fail");

    assert!(
        error
            .to_string()
            .contains("target file type is unsupported")
    );
}

#[test]
fn apply_file_operations_should_skip_optional_missing_sources() {
    let (root, worktree) = temp_workspace("missing-source");
    let mut missing = operation(FileOperationKind::Copy, &root, &worktree, ".env", ".env");
    missing.status = PlannedFileStatus::SkippedMissingSource;
    let plan = run_plan(&root, &worktree, vec![missing]);
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("missing optional source should skip");

    assert_eq!(
        reporter.messages(),
        ["treeboot: skip copy .env; missing source"]
    );
}

#[test]
fn apply_file_operations_should_skip_optional_missing_sources_with_strict_or_force() {
    for options in [
        FileApplyOptions {
            strict: true,
            ..FileApplyOptions::default()
        },
        FileApplyOptions {
            force: true,
            ..FileApplyOptions::default()
        },
    ] {
        let (root, worktree) = temp_workspace("missing-source-mode");
        let mut missing = operation(FileOperationKind::Copy, &root, &worktree, ".env", ".env");
        missing.status = PlannedFileStatus::SkippedMissingSource;
        let plan = run_plan(&root, &worktree, vec![missing]);
        let mut reporter = VecReporter::default();

        apply_file_operations(&plan, options, &mut reporter)
            .expect("missing optional source should skip");

        assert_eq!(
            reporter.messages(),
            ["treeboot: skip copy .env; missing source"]
        );
    }
}

#[test]
fn apply_file_operations_should_report_optional_missing_source_in_dry_run() {
    let (root, worktree) = temp_workspace("missing-source-dry-run");
    let mut missing = operation(FileOperationKind::Copy, &root, &worktree, ".env", ".env");
    missing.status = PlannedFileStatus::SkippedMissingSource;
    let plan = run_plan(&root, &worktree, vec![missing]);
    let mut reporter = VecReporter::default();

    apply_file_operations(
        &plan,
        FileApplyOptions {
            dry_run: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect("dry-run missing optional source should skip");

    assert_eq!(
        reporter.messages(),
        ["treeboot: would skip copy .env; missing source"]
    );
}

#[test]
fn apply_file_operations_should_sync_missing_file() {
    let (root, worktree) = temp_workspace("sync-missing-file");
    fs::write(root.join(".env"), "TOKEN=1\n").expect("source should be written");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(&root, &worktree, ".env", ".env")],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("sync should create missing file");

    let synced = fs::read_to_string(worktree.join(".env")).expect("target should be readable");
    assert_eq!(synced, "TOKEN=1\n");
    assert_eq!(reporter.messages(), ["treeboot: sync .env -> .env"]);
}

#[test]
fn apply_file_operations_should_leave_unchanged_metadata_sync_silent() {
    let (root, worktree) = temp_workspace("sync-unchanged");
    let source = root.join(".env");
    let target = worktree.join(".env");
    fs::write(&source, "TOKEN=1\n").expect("source should be written");
    copy_file_with_metadata(FileOperationKind::Sync, &source, &target, &root, &worktree)
        .expect("target should be seeded");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(&root, &worktree, ".env", ".env")],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("unchanged sync should succeed");

    assert!(reporter.messages().is_empty());
}

#[test]
fn apply_file_operations_should_leave_unchanged_sync_silent_in_dry_run() {
    let (root, worktree) = temp_workspace("sync-unchanged-dry-run");
    let source = root.join(".env");
    let target = worktree.join(".env");
    fs::write(&source, "TOKEN=1\n").expect("source should be written");
    copy_file_with_metadata(FileOperationKind::Sync, &source, &target, &root, &worktree)
        .expect("target should be seeded");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(&root, &worktree, ".env", ".env")],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(
        &plan,
        FileApplyOptions {
            dry_run: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect("unchanged dry-run sync should succeed");

    assert!(reporter.messages().is_empty());
}

#[test]
fn apply_file_operations_should_preserve_copied_file_modified_time() {
    let (root, worktree) = temp_workspace("copy-mtime");
    let source = root.join(".env");
    let target = worktree.join(".env");
    fs::write(&source, "TOKEN=1\n").expect("source should be written");
    let source_mtime = UNIX_EPOCH + Duration::from_secs(123);
    File::options()
        .write(true)
        .open(&source)
        .and_then(|file| file.set_times(FileTimes::new().set_modified(source_mtime)))
        .expect("source mtime should be set");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            ".env",
            ".env",
        )],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("copy should preserve file metadata");

    let target_mtime = fs::metadata(&target)
        .expect("target metadata should be readable")
        .modified()
        .expect("target mtime should be readable");
    assert_eq!(target_mtime, source_mtime);
}

#[test]
fn apply_file_operations_should_repair_sync_file_modified_time() {
    let (root, worktree) = temp_workspace("sync-mtime-repair");
    let source = root.join(".env");
    let target = worktree.join(".env");
    fs::write(&source, "TOKEN=1\n").expect("source should be written");
    fs::write(&target, "TOKEN=1\n").expect("target should be written");
    let source_mtime = UNIX_EPOCH + Duration::from_secs(200);
    let target_mtime = UNIX_EPOCH + Duration::from_secs(100);
    File::options()
        .write(true)
        .open(&source)
        .and_then(|file| file.set_times(FileTimes::new().set_modified(source_mtime)))
        .expect("source mtime should be set");
    File::options()
        .write(true)
        .open(&target)
        .and_then(|file| file.set_times(FileTimes::new().set_modified(target_mtime)))
        .expect("target mtime should be set");
    let mut sync = sync_operation(&root, &worktree, ".env", ".env");
    sync.compare = Some(SyncCompare::Checksum);
    let plan = run_plan(&root, &worktree, vec![sync]);
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("sync should repair mtime-only drift");

    let repaired = fs::metadata(&target)
        .expect("target metadata should be readable")
        .modified()
        .expect("target mtime should be readable");
    assert_eq!(repaired, source_mtime);
    assert_eq!(
        reporter.messages(),
        ["treeboot: sync metadata .env -> .env"]
    );
}

#[test]
fn apply_file_operations_should_report_metadata_repair_in_dry_run_without_mutation() {
    let (root, worktree) = temp_workspace("sync-mtime-dry-run");
    let source = root.join(".env");
    let target = worktree.join(".env");
    fs::write(&source, "TOKEN=1\n").expect("source should be written");
    fs::write(&target, "TOKEN=1\n").expect("target should be written");
    let source_mtime = UNIX_EPOCH + Duration::from_secs(200);
    let target_mtime = UNIX_EPOCH + Duration::from_secs(100);
    File::options()
        .write(true)
        .open(&source)
        .and_then(|file| file.set_times(FileTimes::new().set_modified(source_mtime)))
        .expect("source mtime should be set");
    File::options()
        .write(true)
        .open(&target)
        .and_then(|file| file.set_times(FileTimes::new().set_modified(target_mtime)))
        .expect("target mtime should be set");
    let mut sync = sync_operation(&root, &worktree, ".env", ".env");
    sync.compare = Some(SyncCompare::Checksum);
    let plan = run_plan(&root, &worktree, vec![sync]);
    let mut reporter = VecReporter::default();

    let report = apply_file_operations(
        &plan,
        FileApplyOptions {
            dry_run: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect("dry-run sync should report metadata repair");

    let unchanged = fs::metadata(&target)
        .expect("target metadata should be readable")
        .modified()
        .expect("target mtime should be readable");
    assert_eq!(unchanged, target_mtime);
    assert_eq!(report.action_count, 1);
    assert_eq!(
        reporter.messages(),
        ["treeboot: would sync metadata .env -> .env"]
    );
}

#[test]
fn apply_file_operations_should_update_changed_metadata_sync_file() {
    let (root, worktree) = temp_workspace("sync-metadata-update");
    fs::write(root.join(".env"), "new\n").expect("source should be written");
    fs::write(worktree.join(".env"), "old\n").expect("target should be written");
    let times = FileTimes::new().set_modified(UNIX_EPOCH);
    File::options()
        .write(true)
        .open(worktree.join(".env"))
        .and_then(|file| file.set_times(times))
        .expect("target mtime should be set");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(&root, &worktree, ".env", ".env")],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("changed sync should update target");

    let synced = fs::read_to_string(worktree.join(".env")).expect("target should be readable");
    assert_eq!(synced, "new\n");
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_repair_sync_directory_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let (root, worktree) = temp_workspace("sync-directory-permission-repair");
    let source = root.join("shared");
    let target = worktree.join("shared");
    fs::create_dir_all(&source).expect("source dir should be created");
    fs::create_dir_all(&target).expect("target dir should be created");
    let mut source_permissions = fs::metadata(&source)
        .expect("source metadata should be readable")
        .permissions();
    source_permissions.set_mode(0o700);
    fs::set_permissions(&source, source_permissions).expect("source mode should be set");
    let mut target_permissions = fs::metadata(&target)
        .expect("target metadata should be readable")
        .permissions();
    target_permissions.set_mode(0o755);
    fs::set_permissions(&target, target_permissions).expect("target mode should be set");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(&root, &worktree, "shared", "shared")],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("sync should repair directory permissions");

    let mode = fs::metadata(&target)
        .expect("target metadata should be readable")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o700);
    assert_eq!(
        reporter.messages(),
        ["treeboot: sync metadata shared -> shared"]
    );
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_repair_directory_metadata_after_child_updates() {
    use std::os::unix::fs::PermissionsExt;

    let (root, worktree) = temp_workspace("sync-directory-metadata-after-child");
    let source = root.join("shared");
    let target = worktree.join("shared");
    fs::create_dir_all(&source).expect("source dir should be created");
    fs::create_dir_all(&target).expect("target dir should be created");
    fs::write(source.join("config"), "new\n").expect("source file should be written");
    fs::write(target.join("config"), "old\n").expect("target file should be written");
    let mut source_permissions = fs::metadata(&source)
        .expect("source metadata should be readable")
        .permissions();
    source_permissions.set_mode(0o555);
    fs::set_permissions(&source, source_permissions).expect("source mode should be set");
    let mut target_permissions = fs::metadata(&target)
        .expect("target metadata should be readable")
        .permissions();
    target_permissions.set_mode(0o755);
    fs::set_permissions(&target, target_permissions).expect("target mode should be set");
    let mut sync = sync_operation(&root, &worktree, "shared", "shared");
    sync.compare = Some(SyncCompare::Checksum);
    let plan = run_plan(&root, &worktree, vec![sync]);
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("sync should update children before directory metadata");

    let synced =
        fs::read_to_string(target.join("config")).expect("target child should be readable");
    let mode = fs::metadata(&target)
        .expect("target metadata should be readable")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(synced, "new\n");
    assert_eq!(mode, 0o555);
    assert_eq!(
        reporter.messages(),
        ["treeboot: sync shared -> shared (2 changed)"]
    );
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_sync_child_inside_existing_read_only_directory() {
    use std::os::unix::fs::PermissionsExt;

    let (root, worktree) = temp_workspace("read-only-directory-sync-child");
    let source = root.join("shared");
    let target = worktree.join("shared");
    fs::create_dir_all(&source).expect("source dir should be created");
    fs::create_dir_all(&target).expect("target dir should be created");
    fs::write(source.join("config"), "new\n").expect("source child should be written");
    fs::write(target.join("config"), "old\n").expect("target child should be written");
    let mut source_permissions = fs::metadata(&source)
        .expect("source metadata should be readable")
        .permissions();
    source_permissions.set_mode(0o555);
    fs::set_permissions(&source, source_permissions).expect("source mode should be set");
    let mut target_permissions = fs::metadata(&target)
        .expect("target metadata should be readable")
        .permissions();
    target_permissions.set_mode(0o555);
    fs::set_permissions(&target, target_permissions).expect("target mode should be set");
    let mut sync = sync_operation(&root, &worktree, "shared", "shared");
    sync.compare = Some(SyncCompare::Checksum);
    let plan = run_plan(&root, &worktree, vec![sync]);
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("sync should replace child inside read-only directory");

    let synced =
        fs::read_to_string(target.join("config")).expect("target child should be readable");
    let mode = fs::metadata(&target)
        .expect("target metadata should be readable")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(synced, "new\n");
    assert_eq!(mode, 0o555);
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_ignore_sync_directory_permissions_when_configured() {
    use std::os::unix::fs::PermissionsExt;

    let (root, worktree) = temp_workspace("sync-directory-permission-ignore");
    let source = root.join("shared");
    let target = worktree.join("shared");
    fs::create_dir_all(&source).expect("source dir should be created");
    fs::create_dir_all(&target).expect("target dir should be created");
    let mut source_permissions = fs::metadata(&source)
        .expect("source metadata should be readable")
        .permissions();
    source_permissions.set_mode(0o700);
    fs::set_permissions(&source, source_permissions).expect("source mode should be set");
    let mut target_permissions = fs::metadata(&target)
        .expect("target metadata should be readable")
        .permissions();
    target_permissions.set_mode(0o755);
    fs::set_permissions(&target, target_permissions).expect("target mode should be set");
    let mut sync = sync_operation(&root, &worktree, "shared", "shared");
    sync.ignore_metadata = vec![MetadataField::Permissions];
    let plan = run_plan(&root, &worktree, vec![sync]);
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("ignored directory metadata drift should not repair");

    let mode = fs::metadata(&target)
        .expect("target metadata should be readable")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o755);
    assert!(reporter.messages().is_empty());
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_replace_target_symlink_with_sync_file() {
    let (root, worktree) = temp_workspace("sync-file-over-symlink");
    fs::write(root.join(".env"), "new\n").expect("source should be written");
    fs::write(worktree.join("old"), "old\n").expect("old target should be written");
    std::os::unix::fs::symlink("old", worktree.join(".env"))
        .expect("target symlink should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(&root, &worktree, ".env", ".env")],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("sync file should replace target symlink");

    let synced = fs::read_to_string(worktree.join(".env")).expect("target should be readable");
    assert_eq!(synced, "new\n");
}

#[test]
fn apply_file_operations_should_reject_sync_file_to_directory_target() {
    let (root, worktree) = temp_workspace("sync-file-to-directory");
    fs::write(root.join(".env"), "new\n").expect("source should be written");
    fs::create_dir_all(worktree.join(".env")).expect("target dir should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(&root, &worktree, ".env", ".env")],
    );
    let mut reporter = VecReporter::default();

    let error = apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect_err("sync file to directory should fail");

    assert!(error.to_string().contains("target is a directory"));
}

#[test]
fn apply_file_operations_should_reject_sync_directory_to_file_target() {
    let (root, worktree) = temp_workspace("sync-directory-to-file");
    fs::create_dir_all(root.join("shared")).expect("source dir should be created");
    fs::write(worktree.join("shared"), "old\n").expect("target file should be written");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(&root, &worktree, "shared", "shared")],
    );
    let mut reporter = VecReporter::default();

    let error = apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect_err("sync directory to file should fail");

    assert!(error.to_string().contains("target is a file or symlink"));
}

#[test]
fn apply_file_operations_should_update_checksum_sync_when_metadata_matches() {
    let (root, worktree) = temp_workspace("sync-checksum-update");
    let source = root.join(".env");
    let target = worktree.join(".env");
    fs::write(&source, "ABC\n").expect("source should be written");
    fs::write(&target, "XYZ\n").expect("target should be written");
    let modified = fs::metadata(&source)
        .expect("source metadata should be readable")
        .modified()
        .expect("source mtime should be readable");
    let times = FileTimes::new().set_modified(modified);
    File::options()
        .write(true)
        .open(&target)
        .and_then(|file| file.set_times(times))
        .expect("target mtime should be aligned");
    let mut sync = sync_operation(&root, &worktree, ".env", ".env");
    sync.compare = Some(SyncCompare::Checksum);
    let plan = run_plan(&root, &worktree, vec![sync]);
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("checksum sync should update changed content");

    let synced = fs::read_to_string(target).expect("target should be readable");
    assert_eq!(synced, "ABC\n");
}

#[test]
fn apply_file_operations_should_leave_unchanged_checksum_sync_silent() {
    let (root, worktree) = temp_workspace("sync-checksum-unchanged");
    let source = root.join(".env");
    let target = worktree.join(".env");
    fs::write(&source, "ABC\n").expect("source should be written");
    fs::write(&target, "ABC\n").expect("target should be written");
    let modified = fs::metadata(&source)
        .expect("source metadata should be readable")
        .modified()
        .expect("source mtime should be readable");
    let times = FileTimes::new().set_modified(modified);
    File::options()
        .write(true)
        .open(&target)
        .and_then(|file| file.set_times(times))
        .expect("target mtime should be aligned");
    let mut sync = sync_operation(&root, &worktree, ".env", ".env");
    sync.compare = Some(SyncCompare::Checksum);
    let plan = run_plan(&root, &worktree, vec![sync]);
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("unchanged checksum sync should succeed");

    assert!(reporter.messages().is_empty());
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_repair_sync_file_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let (root, worktree) = temp_workspace("sync-permission-repair");
    let source = root.join(".env");
    let target = worktree.join(".env");
    fs::write(&source, "ABC\n").expect("source should be written");
    fs::write(&target, "ABC\n").expect("target should be written");
    let modified = fs::metadata(&source)
        .expect("source metadata should be readable")
        .modified()
        .expect("source mtime should be readable");
    File::options()
        .write(true)
        .open(&target)
        .and_then(|file| file.set_times(FileTimes::new().set_modified(modified)))
        .expect("target mtime should match source");
    let mut source_permissions = fs::metadata(&source)
        .expect("source metadata should be readable")
        .permissions();
    source_permissions.set_mode(0o600);
    fs::set_permissions(&source, source_permissions).expect("source mode should be set");
    let mut target_permissions = fs::metadata(&target)
        .expect("target metadata should be readable")
        .permissions();
    target_permissions.set_mode(0o644);
    fs::set_permissions(&target, target_permissions).expect("target mode should be set");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(&root, &worktree, ".env", ".env")],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("metadata drift should repair");

    let mode = fs::metadata(&target)
        .expect("target metadata should be readable")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
    assert_eq!(
        reporter.messages(),
        ["treeboot: sync metadata .env -> .env"]
    );
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_restore_ignored_read_only_directory_permissions_after_sync() {
    use std::os::unix::fs::PermissionsExt;

    let (root, worktree) = temp_workspace("read-only-directory-ignore-perms");
    let source = root.join("shared");
    let target = worktree.join("shared");
    fs::create_dir_all(&source).expect("source dir should be created");
    fs::create_dir_all(&target).expect("target dir should be created");
    fs::write(source.join("config"), "new\n").expect("source child should be written");
    fs::write(target.join("config"), "old\n").expect("target child should be written");
    let mut source_permissions = fs::metadata(&source)
        .expect("source metadata should be readable")
        .permissions();
    source_permissions.set_mode(0o755);
    fs::set_permissions(&source, source_permissions).expect("source mode should be set");
    let mut target_permissions = fs::metadata(&target)
        .expect("target metadata should be readable")
        .permissions();
    target_permissions.set_mode(0o555);
    fs::set_permissions(&target, target_permissions).expect("target mode should be set");
    let mut sync = sync_operation(&root, &worktree, "shared", "shared");
    sync.compare = Some(SyncCompare::Checksum);
    sync.ignore_metadata = vec![MetadataField::Permissions];
    let plan = run_plan(&root, &worktree, vec![sync]);
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("sync should restore ignored target directory permissions");

    let synced =
        fs::read_to_string(target.join("config")).expect("target child should be readable");
    let mode = fs::metadata(&target)
        .expect("target metadata should be readable")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(synced, "new\n");
    assert_eq!(mode, 0o555);
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_repair_write_only_target_file_metadata() {
    use std::os::unix::fs::PermissionsExt;

    let (root, worktree) = temp_workspace("sync-write-only-target-repair");
    let source = root.join(".env");
    let target = worktree.join(".env");
    fs::write(&source, "ABC\n").expect("source should be written");
    fs::write(&target, "ABC\n").expect("target should be written");
    let mtime = UNIX_EPOCH + Duration::from_secs(200);
    File::options()
        .write(true)
        .open(&source)
        .and_then(|file| file.set_times(FileTimes::new().set_modified(mtime)))
        .expect("source mtime should be set");
    File::options()
        .write(true)
        .open(&target)
        .and_then(|file| file.set_times(FileTimes::new().set_modified(mtime)))
        .expect("target mtime should be set");
    let mut source_permissions = fs::metadata(&source)
        .expect("source metadata should be readable")
        .permissions();
    source_permissions.set_mode(0o600);
    fs::set_permissions(&source, source_permissions).expect("source mode should be set");
    let mut target_permissions = fs::metadata(&target)
        .expect("target metadata should be readable")
        .permissions();
    target_permissions.set_mode(0o200);
    fs::set_permissions(&target, target_permissions).expect("target mode should be set");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(&root, &worktree, ".env", ".env")],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("write-only target metadata should repair");

    let metadata = fs::metadata(&target).expect("target metadata should be readable");
    let mode = metadata.permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    assert_eq!(
        reporter.messages(),
        ["treeboot: sync metadata .env -> .env"]
    );
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_repair_read_only_target_file_metadata() {
    use std::os::unix::fs::PermissionsExt;

    let (root, worktree) = temp_workspace("sync-read-only-target-repair");
    let source = root.join(".env");
    let target = worktree.join(".env");
    fs::write(&source, "ABC\n").expect("source should be written");
    fs::write(&target, "ABC\n").expect("target should be written");
    let source_mtime = UNIX_EPOCH + Duration::from_secs(200);
    let target_mtime = UNIX_EPOCH + Duration::from_secs(100);
    File::options()
        .write(true)
        .open(&source)
        .and_then(|file| file.set_times(FileTimes::new().set_modified(source_mtime)))
        .expect("source mtime should be set");
    File::options()
        .write(true)
        .open(&target)
        .and_then(|file| file.set_times(FileTimes::new().set_modified(target_mtime)))
        .expect("target mtime should be set");
    let mut source_permissions = fs::metadata(&source)
        .expect("source metadata should be readable")
        .permissions();
    source_permissions.set_mode(0o600);
    fs::set_permissions(&source, source_permissions).expect("source mode should be set");
    let mut target_permissions = fs::metadata(&target)
        .expect("target metadata should be readable")
        .permissions();
    target_permissions.set_mode(0o400);
    fs::set_permissions(&target, target_permissions).expect("target mode should be set");
    let mut sync = sync_operation(&root, &worktree, ".env", ".env");
    sync.compare = Some(SyncCompare::Checksum);
    let plan = run_plan(&root, &worktree, vec![sync]);
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("read-only target metadata should repair");

    let metadata = fs::metadata(&target).expect("target metadata should be readable");
    let repaired = metadata
        .modified()
        .expect("target mtime should be readable");
    let mode = metadata.permissions().mode() & 0o777;
    assert_eq!(repaired, source_mtime);
    assert_eq!(mode, 0o600);
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_ignore_sync_file_permissions_when_configured() {
    use std::os::unix::fs::PermissionsExt;

    let (root, worktree) = temp_workspace("sync-permission-ignore");
    let source = root.join(".env");
    let target = worktree.join(".env");
    fs::write(&source, "ABC\n").expect("source should be written");
    fs::write(&target, "ABC\n").expect("target should be written");
    let modified = fs::metadata(&source)
        .expect("source metadata should be readable")
        .modified()
        .expect("source mtime should be readable");
    File::options()
        .write(true)
        .open(&target)
        .and_then(|file| file.set_times(FileTimes::new().set_modified(modified)))
        .expect("target mtime should match source");
    let mut source_permissions = fs::metadata(&source)
        .expect("source metadata should be readable")
        .permissions();
    source_permissions.set_mode(0o600);
    fs::set_permissions(&source, source_permissions).expect("source mode should be set");
    let mut target_permissions = fs::metadata(&target)
        .expect("target metadata should be readable")
        .permissions();
    target_permissions.set_mode(0o644);
    fs::set_permissions(&target, target_permissions).expect("target mode should be set");
    let mut sync = sync_operation(&root, &worktree, ".env", ".env");
    sync.ignore_metadata = vec![MetadataField::Permissions];
    let plan = run_plan(&root, &worktree, vec![sync]);
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("ignored metadata drift should not repair");

    let mode = fs::metadata(&target)
        .expect("target metadata should be readable")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o644);
    assert!(reporter.messages().is_empty());
}

#[test]
fn apply_file_operations_should_update_checksum_sync_when_size_differs() {
    let (root, worktree) = temp_workspace("sync-checksum-size");
    fs::write(root.join(".env"), "longer\n").expect("source should be written");
    fs::write(worktree.join(".env"), "old\n").expect("target should be written");
    let mut sync = sync_operation(&root, &worktree, ".env", ".env");
    sync.compare = Some(SyncCompare::Checksum);
    let plan = run_plan(&root, &worktree, vec![sync]);
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("checksum sync should update size change");

    let synced = fs::read_to_string(worktree.join(".env")).expect("target should be readable");
    assert_eq!(synced, "longer\n");
}

#[test]
fn apply_file_operations_should_force_sync_file_like_default() {
    let (root, worktree) = temp_workspace("force-sync-file");
    fs::write(root.join(".env"), "new\n").expect("source should be written");
    fs::write(worktree.join(".env"), "old\n").expect("target should be written");
    let times = FileTimes::new().set_modified(UNIX_EPOCH);
    File::options()
        .write(true)
        .open(worktree.join(".env"))
        .and_then(|file| file.set_times(times))
        .expect("target mtime should be set");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(&root, &worktree, ".env", ".env")],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(
        &plan,
        FileApplyOptions {
            force: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect("forced sync should update target");

    let synced = fs::read_to_string(worktree.join(".env")).expect("target should be readable");
    assert_eq!(synced, "new\n");
}

#[test]
fn apply_file_operations_should_preserve_sync_directory_extras_by_default() {
    let (root, worktree) = temp_workspace("sync-no-delete");
    fs::create_dir_all(root.join("shared")).expect("source dir should be created");
    fs::create_dir_all(worktree.join("shared")).expect("target dir should be created");
    fs::write(root.join("shared/config"), "new\n").expect("source should be written");
    fs::write(worktree.join("shared/extra"), "keep\n").expect("extra should be written");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(&root, &worktree, "shared", "shared")],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("sync should preserve extras by default");

    assert!(worktree.join("shared/extra").exists());
}

#[test]
fn apply_file_operations_should_delete_target_only_entries_when_sync_delete_is_true() {
    let (root, worktree) = temp_workspace("sync-delete");
    fs::create_dir_all(root.join("shared")).expect("source dir should be created");
    fs::create_dir_all(worktree.join("shared/extra-dir"))
        .expect("target extra dir should be created");
    fs::write(root.join("shared/config"), "new\n").expect("source should be written");
    fs::write(worktree.join("shared/extra"), "remove\n").expect("extra should be written");
    fs::write(worktree.join("shared/extra-dir/file"), "remove\n")
        .expect("nested extra should be written");
    let mut sync = sync_operation(&root, &worktree, "shared", "shared");
    sync.delete = Some(true);
    let plan = run_plan(&root, &worktree, vec![sync]);
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("sync delete should remove target-only entries");

    assert!(!worktree.join("shared/extra").exists());
    assert!(!worktree.join("shared/extra-dir").exists());
    assert_eq!(
        reporter.messages(),
        ["treeboot: sync shared -> shared (1 changed, 2 deleted)"]
    );
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_delete_before_read_only_directory_metadata() {
    use std::os::unix::fs::PermissionsExt;

    let (root, worktree) = temp_workspace("sync-delete-read-only-directory");
    let source = root.join("shared");
    let target = worktree.join("shared");
    fs::create_dir_all(&source).expect("source dir should be created");
    fs::create_dir_all(&target).expect("target dir should be created");
    fs::write(target.join("extra"), "remove\n").expect("extra should be written");
    let mut source_permissions = fs::metadata(&source)
        .expect("source metadata should be readable")
        .permissions();
    source_permissions.set_mode(0o555);
    fs::set_permissions(&source, source_permissions).expect("source mode should be set");
    let mut target_permissions = fs::metadata(&target)
        .expect("target metadata should be readable")
        .permissions();
    target_permissions.set_mode(0o755);
    fs::set_permissions(&target, target_permissions).expect("target mode should be set");
    let mut sync = sync_operation(&root, &worktree, "shared", "shared");
    sync.delete = Some(true);
    let plan = run_plan(&root, &worktree, vec![sync]);
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("sync should delete extras before read-only directory metadata");

    let mode = fs::metadata(&target)
        .expect("target metadata should be readable")
        .permissions()
        .mode()
        & 0o777;
    assert!(!target.join("extra").exists());
    assert_eq!(mode, 0o555);
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_delete_inside_existing_read_only_directory() {
    use std::os::unix::fs::PermissionsExt;

    let (root, worktree) = temp_workspace("read-only-directory-sync-delete-child");
    let source = root.join("shared");
    let target = worktree.join("shared");
    fs::create_dir_all(&source).expect("source dir should be created");
    fs::create_dir_all(&target).expect("target dir should be created");
    fs::write(target.join("extra"), "remove\n").expect("extra should be written");
    let mut source_permissions = fs::metadata(&source)
        .expect("source metadata should be readable")
        .permissions();
    source_permissions.set_mode(0o555);
    fs::set_permissions(&source, source_permissions).expect("source mode should be set");
    let mut target_permissions = fs::metadata(&target)
        .expect("target metadata should be readable")
        .permissions();
    target_permissions.set_mode(0o555);
    fs::set_permissions(&target, target_permissions).expect("target mode should be set");
    let mut sync = sync_operation(&root, &worktree, "shared", "shared");
    sync.delete = Some(true);
    let plan = run_plan(&root, &worktree, vec![sync]);
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("sync should delete child inside read-only directory");

    let mode = fs::metadata(&target)
        .expect("target metadata should be readable")
        .permissions()
        .mode()
        & 0o777;
    assert!(!target.join("extra").exists());
    assert_eq!(mode, 0o555);
}

#[test]
fn apply_file_operations_should_report_sync_delete_in_dry_run_without_mutation() {
    let (root, worktree) = temp_workspace("sync-delete-dry-run");
    fs::create_dir_all(root.join("shared")).expect("source dir should be created");
    fs::create_dir_all(worktree.join("shared")).expect("target dir should be created");
    fs::write(worktree.join("shared/extra"), "keep\n").expect("extra should be written");
    let mut sync = sync_operation(&root, &worktree, "shared", "shared");
    sync.delete = Some(true);
    let plan = run_plan(&root, &worktree, vec![sync]);
    let mut reporter = VecReporter::default();

    apply_file_operations(
        &plan,
        FileApplyOptions {
            dry_run: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect("dry-run sync delete should plan");

    assert!(worktree.join("shared/extra").exists());
    assert_eq!(
        reporter.messages(),
        ["treeboot: would sync shared -> shared (1 delete)"]
    );
}

#[test]
fn apply_file_operations_should_report_sync_create_in_dry_run_without_mutation() {
    let (root, worktree) = temp_workspace("sync-create-dry-run");
    fs::write(root.join(".env"), "new\n").expect("source should be written");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(&root, &worktree, ".env", ".env")],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(
        &plan,
        FileApplyOptions {
            dry_run: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect("dry-run sync create should plan");

    assert!(!worktree.join(".env").exists());
    assert_eq!(reporter.messages(), ["treeboot: would sync .env -> .env"]);
}

#[test]
fn apply_file_operations_should_report_sync_directory_create_in_dry_run() {
    let (root, worktree) = temp_workspace("sync-directory-create-dry-run");
    fs::create_dir_all(root.join("shared/nested")).expect("source dir should be created");
    fs::write(root.join("shared/nested/config"), "new\n").expect("source should be written");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(&root, &worktree, "shared", "shared")],
    );
    let mut reporter = VecReporter::default();

    let report = apply_file_operations(
        &plan,
        FileApplyOptions {
            dry_run: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect("dry-run sync directory create should plan");

    assert!(!worktree.join("shared").exists());
    assert_eq!(report.action_count, 3);
    assert_eq!(
        reporter.messages(),
        ["treeboot: would sync shared -> shared (3 changes)"]
    );
}

#[test]
fn apply_file_operations_should_report_sync_update_in_dry_run_without_mutation() {
    let (root, worktree) = temp_workspace("sync-update-dry-run");
    fs::write(root.join(".env"), "new-value\n").expect("source should be written");
    fs::write(worktree.join(".env"), "old\n").expect("target should be written");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(&root, &worktree, ".env", ".env")],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(
        &plan,
        FileApplyOptions {
            dry_run: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect("dry-run sync update should plan");

    let existing =
        fs::read_to_string(worktree.join(".env")).expect("target should remain readable");
    assert_eq!(existing, "old\n");
    assert_eq!(reporter.messages(), ["treeboot: would sync .env -> .env"]);
}

#[test]
fn apply_file_operations_should_report_copy_skip_in_dry_run_without_mutation() {
    let (root, worktree) = temp_workspace("copy-skip-dry-run");
    fs::write(root.join(".env"), "new\n").expect("source should be written");
    fs::write(worktree.join(".env"), "old\n").expect("target should be written");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            ".env",
            ".env",
        )],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(
        &plan,
        FileApplyOptions {
            dry_run: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect("dry-run copy skip should plan");

    let existing =
        fs::read_to_string(worktree.join(".env")).expect("target should remain readable");
    assert_eq!(existing, "old\n");
    assert_eq!(
        reporter.messages(),
        ["treeboot: would skip copy .env; target exists"]
    );
}

#[test]
fn apply_file_operations_should_report_directory_create_in_dry_run_without_mutation() {
    let (root, worktree) = temp_workspace("directory-create-dry-run");
    fs::create_dir_all(root.join("shared")).expect("source dir should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            "shared",
            "shared",
        )],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(
        &plan,
        FileApplyOptions {
            dry_run: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect("dry-run directory copy should plan");

    assert!(!worktree.join("shared").exists());
    assert_eq!(
        reporter.messages(),
        ["treeboot: would copy shared -> shared (1 change)"]
    );
}

#[test]
fn apply_file_operations_should_delete_nested_target_only_entries() {
    let (root, worktree) = temp_workspace("sync-nested-delete");
    fs::create_dir_all(root.join("shared/nested")).expect("source dir should be created");
    fs::create_dir_all(worktree.join("shared/nested")).expect("target dir should be created");
    fs::write(root.join("shared/nested/config"), "keep\n").expect("source file should be written");
    fs::write(worktree.join("shared/nested/old"), "remove\n")
        .expect("nested extra should be written");
    let mut sync = sync_operation(&root, &worktree, "shared", "shared");
    sync.delete = Some(true);
    let plan = run_plan(&root, &worktree, vec![sync]);
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("sync delete should remove nested target-only entries");

    assert!(!worktree.join("shared/nested/old").exists());
    assert!(worktree.join("shared/nested/config").exists());
}

#[test]
fn apply_file_operations_should_leave_dry_run_unmutated() {
    let (root, worktree) = temp_workspace("dry-run");
    fs::write(root.join(".env"), "TOKEN=1\n").expect("source should be written");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            ".env",
            ".env",
        )],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(
        &plan,
        FileApplyOptions {
            dry_run: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect("dry-run should plan");

    assert!(!worktree.join(".env").exists());
    assert_eq!(reporter.messages(), ["treeboot: would copy .env -> .env"]);
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_force_copy_file_over_existing_symlink() {
    let (root, worktree) = temp_workspace("force-copy-over-symlink");
    fs::write(root.join(".env"), "new\n").expect("source should be written");
    fs::write(worktree.join("old"), "old\n").expect("old target should be written");
    std::os::unix::fs::symlink("old", worktree.join(".env"))
        .expect("target symlink should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            ".env",
            ".env",
        )],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(
        &plan,
        FileApplyOptions {
            force: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect("force copy should replace target symlink");

    let copied = fs::read_to_string(worktree.join(".env")).expect("target should be readable");
    assert_eq!(copied, "new\n");
    assert!(
        !fs::symlink_metadata(worktree.join(".env"))
            .expect("target metadata should be readable")
            .file_type()
            .is_symlink()
    );
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_reject_symlink_target_parent_before_copy() {
    let (root, worktree) = temp_workspace("copy-symlink-parent");
    let outside = worktree
        .parent()
        .expect("worktree should have parent")
        .join("outside");
    fs::write(root.join(".env"), "TOKEN=1\n").expect("source should be written");
    fs::create_dir_all(&outside).expect("outside dir should be created");
    std::os::unix::fs::symlink(&outside, worktree.join("linked"))
        .expect("target parent symlink should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            ".env",
            "linked/.env",
        )],
    );
    let mut reporter = VecReporter::default();

    let error = apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect_err("copy through symlink parent should fail");

    assert!(error.to_string().contains("target parent is a symlink"));
    assert!(!outside.join(".env").exists());
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_reject_source_that_resolves_outside_root_at_apply() {
    let (root, worktree) = temp_workspace("copy-symlink-source-parent");
    let outside = root
        .parent()
        .expect("root should have parent")
        .join("outside-source");
    fs::create_dir_all(&outside).expect("outside source dir should be created");
    fs::write(outside.join(".env"), "TOKEN=1\n").expect("outside source should be written");
    std::os::unix::fs::symlink(&outside, root.join("linked"))
        .expect("source parent symlink should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            "linked/.env",
            ".env",
        )],
    );
    let mut reporter = VecReporter::default();

    let error = apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect_err("copy from outside-root source should fail");

    assert!(
        error
            .to_string()
            .contains("source resolves outside root during apply")
    );
    assert!(!worktree.join(".env").exists());
}

#[cfg(unix)]
#[test]
fn remove_any_should_reject_symlink_target_parent_before_delete() {
    let (_root, worktree) = temp_workspace("delete-symlink-parent");
    let outside = worktree
        .parent()
        .expect("worktree should have parent")
        .join("outside-delete");
    fs::create_dir_all(&outside).expect("outside dir should be created");
    fs::write(outside.join("extra"), "keep\n").expect("outside file should be written");
    std::os::unix::fs::symlink(&outside, worktree.join("linked"))
        .expect("target parent symlink should be created");

    let error = remove_any(
        FileOperationKind::Sync,
        &worktree.join("linked/extra"),
        &worktree,
    )
    .expect_err("delete through symlink parent should fail");

    assert!(error.to_string().contains("target parent is a symlink"));
    assert_eq!(
        fs::read_to_string(outside.join("extra")).expect("outside file should remain readable"),
        "keep\n"
    );
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_copy_read_only_file() {
    use std::os::unix::fs::PermissionsExt;

    let (root, worktree) = temp_workspace("read-only-copy");
    let source = root.join(".env");
    let target = worktree.join(".env");
    fs::write(&source, "TOKEN=1\n").expect("source should be written");
    let mut permissions = fs::metadata(&source)
        .expect("source metadata should be readable")
        .permissions();
    permissions.set_mode(0o444);
    fs::set_permissions(&source, permissions).expect("source should become read-only");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            ".env",
            ".env",
        )],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("read-only source should copy");

    let copied = fs::read_to_string(&target).expect("target should be readable");
    let mode = fs::metadata(&target)
        .expect("target metadata should be readable")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(copied, "TOKEN=1\n");
    assert_eq!(mode, 0o444);
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_copy_file_without_owner_write() {
    use std::os::unix::fs::PermissionsExt;

    let (root, worktree) = temp_workspace("owner-write-copy");
    let source = root.join(".env");
    let target = worktree.join(".env");
    fs::write(&source, "TOKEN=1\n").expect("source should be written");
    let mut permissions = fs::metadata(&source)
        .expect("source metadata should be readable")
        .permissions();
    permissions.set_mode(0o420);
    fs::set_permissions(&source, permissions).expect("source permissions should change");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            ".env",
            ".env",
        )],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("source without owner write should copy");

    let copied = fs::read_to_string(&target).expect("target should be readable");
    let mode = fs::metadata(&target)
        .expect("target metadata should be readable")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(copied, "TOKEN=1\n");
    assert_eq!(mode, 0o420);
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_force_symlink_over_existing_symlink() {
    let (root, worktree) = temp_workspace("force-symlink-over-symlink");
    fs::write(root.join("tool"), "tool\n").expect("source should be written");
    fs::write(worktree.join("old"), "old\n").expect("old target should be written");
    std::os::unix::fs::symlink("old", worktree.join(".tool"))
        .expect("target symlink should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Symlink,
            &root,
            &worktree,
            "tool",
            ".tool",
        )],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(
        &plan,
        FileApplyOptions {
            force: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect("forced symlink should replace existing symlink");

    let link = fs::read_link(worktree.join(".tool")).expect("target should be symlink");
    assert_ne!(link, PathBuf::from("old"));
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_force_symlink_over_existing_file() {
    let (root, worktree) = temp_workspace("force-symlink");
    fs::write(root.join("tool"), "tool\n").expect("source should be written");
    fs::write(worktree.join(".tool"), "old\n").expect("target should be written");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Symlink,
            &root,
            &worktree,
            "tool",
            ".tool",
        )],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(
        &plan,
        FileApplyOptions {
            force: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect("forced symlink should apply");

    let link = fs::read_link(worktree.join(".tool")).expect("target should be symlink");
    assert!(!link.is_absolute());
}

#[test]
fn apply_file_operations_should_report_symlink_create_in_dry_run() {
    let (root, worktree) = temp_workspace("symlink-create-dry-run");
    fs::write(root.join("tool"), "tool\n").expect("source should be written");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Symlink,
            &root,
            &worktree,
            "tool",
            ".tool",
        )],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(
        &plan,
        FileApplyOptions {
            dry_run: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect("dry-run symlink should plan");

    assert!(!worktree.join(".tool").exists());
    assert_eq!(
        reporter.messages(),
        ["treeboot: would symlink tool -> .tool"]
    );
}

#[test]
fn apply_file_operations_should_skip_existing_symlink_target_by_default() {
    let (root, worktree) = temp_workspace("skip-symlink");
    fs::write(root.join("tool"), "tool\n").expect("source should be written");
    fs::write(worktree.join(".tool"), "old\n").expect("target should be written");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Symlink,
            &root,
            &worktree,
            "tool",
            ".tool",
        )],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("existing target should skip");

    assert_eq!(
        reporter.messages(),
        ["treeboot: skip symlink .tool; target exists"]
    );
}

#[test]
fn apply_file_operations_should_reject_existing_symlink_target_in_strict() {
    let (root, worktree) = temp_workspace("strict-symlink");
    fs::write(root.join("tool"), "tool\n").expect("source should be written");
    fs::write(worktree.join(".tool"), "old\n").expect("target should be written");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Symlink,
            &root,
            &worktree,
            "tool",
            ".tool",
        )],
    );
    let mut reporter = VecReporter::default();

    let error = apply_file_operations(
        &plan,
        FileApplyOptions {
            strict: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect_err("strict symlink target should fail");

    assert!(error.to_string().contains("target exists"));
}

#[test]
fn apply_file_operations_should_reject_symlink_to_existing_directory_even_with_force() {
    let (root, worktree) = temp_workspace("symlink-existing-dir");
    fs::write(root.join("tool"), "tool\n").expect("source should be written");
    fs::create_dir_all(worktree.join(".tool")).expect("target dir should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Symlink,
            &root,
            &worktree,
            "tool",
            ".tool",
        )],
    );
    let mut reporter = VecReporter::default();

    let error = apply_file_operations(
        &plan,
        FileApplyOptions {
            force: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect_err("symlink over directory should fail");

    assert!(error.to_string().contains("target is a directory"));
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_reject_sync_symlink_to_existing_directory() {
    let (root, worktree) = temp_workspace("sync-symlink-existing-dir");
    fs::write(root.join("tool"), "tool\n").expect("source target should be written");
    std::os::unix::fs::symlink("tool", root.join("link"))
        .expect("source symlink should be created");
    fs::create_dir_all(worktree.join("link")).expect("target dir should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(&root, &worktree, "link", "link")],
    );
    let mut reporter = VecReporter::default();

    let error = apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect_err("sync symlink over directory should fail");

    assert!(error.to_string().contains("target is a directory"));
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_preserve_copied_source_symlinks() {
    let (root, worktree) = temp_workspace("preserved-symlink");
    let source_dir = root.join("shared");
    fs::create_dir_all(&source_dir).expect("source dir should be created");
    fs::write(source_dir.join("config"), "value\n").expect("source should be written");
    std::os::unix::fs::symlink("config", source_dir.join("link"))
        .expect("source symlink should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            "shared",
            "shared",
        )],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("directory copy should apply");

    let link = fs::read_link(worktree.join("shared/link")).expect("copied symlink should exist");
    assert_eq!(link, PathBuf::from("config"));
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_warn_for_preserved_symlink_to_uncopied_target() {
    let (root, worktree) = temp_workspace("preserved-symlink-warning");
    let source_dir = root.join("shared");
    fs::create_dir_all(&source_dir).expect("source dir should be created");
    fs::write(source_dir.join("config"), "value\n").expect("source should be written");
    std::os::unix::fs::symlink("config", source_dir.join("link"))
        .expect("source symlink should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            "shared/link",
            "shared/link",
        )],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("copied symlink should apply");

    assert!(reporter.events.iter().any(|event| matches!(
        event,
        OutputEvent::FileWarning { reason, .. }
            if reason == "symlink target does not exist"
    )));
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_report_symlink_warning_in_dry_run() {
    let (root, worktree) = temp_workspace("preserved-symlink-warning-dry-run");
    let source_dir = root.join("shared");
    fs::create_dir_all(&source_dir).expect("source dir should be created");
    fs::write(source_dir.join("config"), "value\n").expect("source should be written");
    std::os::unix::fs::symlink("config", source_dir.join("link"))
        .expect("source symlink should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![operation(
            FileOperationKind::Copy,
            &root,
            &worktree,
            "shared/link",
            "shared/link",
        )],
    );
    let mut reporter = VecReporter::default();

    let report = apply_file_operations(
        &plan,
        FileApplyOptions {
            dry_run: true,
            ..FileApplyOptions::default()
        },
        &mut reporter,
    )
    .expect("dry-run copied symlink should plan");

    assert_eq!(report.action_count, 1);
    assert_eq!(
        reporter.messages(),
        [
            "treeboot: warning: shared/link symlink target does not exist",
            "treeboot: would copy shared/link -> shared/link"
        ]
    );
    assert_eq!(reporter.planning_finished_counts, [1]);
    assert_eq!(reporter.execution_started_counts, [1]);
    assert_eq!(reporter.action_advanced_count, 1);
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_sync_preserved_source_symlinks() {
    let (root, worktree) = temp_workspace("sync-preserved-symlink");
    let source_dir = root.join("shared");
    fs::create_dir_all(&source_dir).expect("source dir should be created");
    fs::write(source_dir.join("config"), "value\n").expect("source should be written");
    std::os::unix::fs::symlink("config", source_dir.join("link"))
        .expect("source symlink should be created");
    fs::create_dir_all(worktree.join("shared")).expect("target dir should be created");
    fs::write(worktree.join("shared/link"), "old\n").expect("target should be written");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(&root, &worktree, "shared", "shared")],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("sync should preserve source symlink");

    let link = fs::read_link(worktree.join("shared/link")).expect("synced symlink should exist");
    assert_eq!(link, PathBuf::from("config"));
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_sync_top_level_source_symlink() {
    let (root, worktree) = temp_workspace("sync-top-level-symlink");
    fs::write(root.join("config"), "value\n").expect("source target should be written");
    std::os::unix::fs::symlink("config", root.join("link"))
        .expect("source symlink should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(&root, &worktree, "link", "link")],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("sync should create top-level symlink");

    let link = fs::read_link(worktree.join("link")).expect("synced symlink should exist");
    assert_eq!(link, PathBuf::from("config"));
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_warn_for_sync_symlink_to_uncopied_target() {
    let (root, worktree) = temp_workspace("sync-symlink-warning");
    let source_dir = root.join("shared");
    fs::create_dir_all(&source_dir).expect("source dir should be created");
    fs::write(source_dir.join("config"), "value\n").expect("source should be written");
    std::os::unix::fs::symlink("config", source_dir.join("link"))
        .expect("source symlink should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(
            &root,
            &worktree,
            "shared/link",
            "shared/link",
        )],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("synced symlink should apply");

    assert!(reporter.events.iter().any(|event| matches!(
        event,
        OutputEvent::FileWarning { reason, .. }
            if reason == "symlink target does not exist"
    )));
}

#[cfg(unix)]
#[test]
fn apply_file_operations_should_update_changed_sync_symlink_target() {
    let (root, worktree) = temp_workspace("sync-changed-symlink");
    fs::write(root.join("config"), "value\n").expect("source target should be written");
    fs::write(root.join("other"), "value\n").expect("other source target should be written");
    std::os::unix::fs::symlink("config", root.join("link"))
        .expect("source symlink should be created");
    std::os::unix::fs::symlink("../root/other", worktree.join("link"))
        .expect("target symlink should be created");
    let plan = run_plan(
        &root,
        &worktree,
        vec![sync_operation(&root, &worktree, "link", "link")],
    );
    let mut reporter = VecReporter::default();

    apply_file_operations(&plan, FileApplyOptions::default(), &mut reporter)
        .expect("sync should update changed symlink");

    let link = fs::read_link(worktree.join("link")).expect("synced symlink should exist");
    assert_eq!(link, PathBuf::from("config"));
}

#[cfg(unix)]
#[test]
fn preserved_source_link_should_track_directory_target_type() {
    let (root, worktree) = temp_workspace("preserved-directory-symlink");
    let source_dir = root.join("shared");
    fs::create_dir_all(source_dir.join("dir")).expect("source dir should be created");
    std::os::unix::fs::symlink("dir", source_dir.join("link"))
        .expect("source symlink should be created");
    let plan = run_plan(&root, &worktree, Vec::new());

    let (_, final_target, target_is_dir) = preserved_source_link(
        &plan,
        FileOperationKind::Copy,
        &source_dir.join("link"),
        &worktree.join("shared/link"),
    )
    .expect("preserved symlink should plan");

    assert_eq!(final_target, worktree.join("shared/dir"));
    assert!(target_is_dir);
}

/// A [`Read`] adapter that hands back bytes in a scripted sequence of short
/// reads, simulating filesystems that return fewer bytes than requested even
/// when more data remains. Once the `chunks` script is exhausted it fills
/// whatever the caller's buffer allows.
struct ChunkedRead {
    data: Vec<u8>,
    chunks: Vec<usize>,
    pos: usize,
    chunk_index: usize,
}

impl ChunkedRead {
    fn new(data: Vec<u8>, chunks: Vec<usize>) -> Self {
        Self {
            data,
            chunks,
            pos: 0,
            chunk_index: 0,
        }
    }
}

impl Read for ChunkedRead {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.pos == self.data.len() {
            return Ok(0);
        }

        let chunk = self
            .chunks
            .get(self.chunk_index)
            .copied()
            .unwrap_or(buffer.len())
            .max(1);
        self.chunk_index += 1;

        let read = chunk.min(buffer.len()).min(self.data.len() - self.pos);
        buffer[..read].copy_from_slice(&self.data[self.pos..self.pos + read]);
        self.pos += read;
        Ok(read)
    }
}

/// A [`Read`] adapter that returns [`io::ErrorKind::Interrupted`] a fixed number
/// of times before yielding its data, used to confirm interrupted reads are
/// retried rather than surfaced as failures.
struct InterruptingReader {
    data: Vec<u8>,
    pos: usize,
    pending_interrupts: usize,
}

impl InterruptingReader {
    fn new(data: Vec<u8>, pending_interrupts: usize) -> Self {
        Self {
            data,
            pos: 0,
            pending_interrupts,
        }
    }
}

impl Read for InterruptingReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.pending_interrupts > 0 {
            self.pending_interrupts -= 1;
            return Err(io::Error::from(io::ErrorKind::Interrupted));
        }

        let read = buffer.len().min(self.data.len() - self.pos);
        buffer[..read].copy_from_slice(&self.data[self.pos..self.pos + read]);
        self.pos += read;
        Ok(read)
    }
}

/// A [`Read`] adapter that always fails, used to check error attribution.
struct FailingReader;

impl Read for FailingReader {
    fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::other("read failed"))
    }
}

#[test]
fn reader_contents_changed_should_ignore_short_read_boundaries() {
    // Span more than one 8 KiB read buffer with mismatched short-read scripts so
    // the two handles desync per call; identical content must still compare
    // unchanged. Fails if the comparison loop reads with raw `Read::read`.
    let data: Vec<u8> = (0..(8192 + 137)).map(|i| (i % 251) as u8).collect();
    let mut source = ChunkedRead::new(data.clone(), vec![1, 8, 3, 2, 13]);
    let mut target = ChunkedRead::new(data, vec![5, 1, 1, 16, 4]);

    let changed = reader_contents_changed(&mut source, &mut target)
        .expect("identical readers should compare cleanly");

    assert!(!changed);
}

#[test]
fn reader_contents_changed_should_detect_equal_size_differences() {
    let source_data: Vec<u8> = (0..4096).map(|i| (i % 251) as u8).collect();
    let mut target_data = source_data.clone();
    *target_data.last_mut().expect("data is non-empty") ^= 0xFF;
    let mut source = ChunkedRead::new(source_data, vec![1, 8, 3]);
    let mut target = ChunkedRead::new(target_data, vec![5, 1, 1]);

    let changed = reader_contents_changed(&mut source, &mut target)
        .expect("equal-length readers should compare cleanly");

    assert!(changed);
}

#[test]
fn read_full_chunk_should_fill_buffer_across_short_reads() {
    let data: Vec<u8> = (0..100).map(|i| i as u8).collect();
    let mut reader = ChunkedRead::new(data.clone(), vec![3, 7, 11]);
    let mut buffer = [0u8; 100];

    let read = read_full_chunk(&mut reader, &mut buffer, ContentInput::Source)
        .expect("read_full_chunk should fill the buffer");

    assert_eq!(read, 100);
    assert_eq!(buffer, data.as_slice());
}

#[test]
fn read_full_chunk_should_return_short_count_at_eof() {
    let data: Vec<u8> = (0..10).map(|i| i as u8).collect();
    let mut reader = ChunkedRead::new(data, vec![3]);
    let mut buffer = [0u8; 64];

    let read = read_full_chunk(&mut reader, &mut buffer, ContentInput::Source)
        .expect("read_full_chunk should stop at EOF");

    assert_eq!(read, 10);
}

#[test]
fn read_full_chunk_should_retry_interrupted_reads() {
    let data: Vec<u8> = (0..32).map(|i| i as u8).collect();
    let mut reader = InterruptingReader::new(data.clone(), 2);
    let mut buffer = [0u8; 32];

    let read = read_full_chunk(&mut reader, &mut buffer, ContentInput::Source)
        .expect("interrupted reads should be retried");

    assert_eq!(read, 32);
    assert_eq!(buffer, data.as_slice());
}

#[test]
fn read_full_chunk_should_tag_errors_with_input_side() {
    let mut reader = FailingReader;
    let mut buffer = [0u8; 8];

    let error = read_full_chunk(&mut reader, &mut buffer, ContentInput::Target)
        .expect_err("hard read error should propagate");

    assert_eq!(error.input, ContentInput::Target);
}
