use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;
use treeboot_core::{
    ActionPlan, Config, Environment, Error, ExecuteOptions, Executor, FileOperation,
    FileOperationKind, OutputEvent, PlanOrigin, Reporter, RunPlanOptions, SourceSpan, SymlinkMode,
    Worktree, WorktreeOptions,
};

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

struct GitWorktree {
    root: TempDir,
    _worktree_parent: TempDir,
    worktree_path: PathBuf,
}

impl GitWorktree {
    fn root_path(&self) -> &Path {
        self.root.path()
    }

    fn worktree_path(&self) -> &Path {
        &self.worktree_path
    }
}

fn git(args: &[&str], cwd: &Path) {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("git should run");

    assert!(
        output.status.success(),
        "git {args:?} should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_worktree() -> GitWorktree {
    let root = TempDir::new().expect("root should be created");
    git(&["init"], root.path());
    git(&["config", "user.name", "treeboot"], root.path());
    git(
        &["config", "user.email", "treeboot@example.invalid"],
        root.path(),
    );
    git(&["config", "commit.gpgsign", "false"], root.path());
    write_file(&root.path().join("README.md"), "treeboot test repo\n");
    git(&["add", "README.md"], root.path());
    git(&["commit", "-m", "Initial commit"], root.path());

    let worktree_parent = TempDir::new().expect("worktree parent should be created");
    let worktree_path = worktree_parent.path().join("linked");
    let worktree = worktree_path
        .to_str()
        .expect("worktree path should be valid UTF-8");
    git(
        &["worktree", "add", "-b", "treeboot-core-test", worktree],
        root.path(),
    );

    GitWorktree {
        root,
        _worktree_parent: worktree_parent,
        worktree_path,
    }
}

fn temp_worktree(name: &str) -> (TempDir, Worktree) {
    let temp = TempDir::new().expect("tempdir should be created");
    let root = temp.path().join(format!("{name}-root"));
    let worktree = temp.path().join(format!("{name}-worktree"));
    std::fs::create_dir_all(&root).expect("root should be created");
    std::fs::create_dir_all(&worktree).expect("worktree should be created");

    let context = Worktree {
        root_path: root,
        worktree_path: worktree,
        default_branch: "main".to_owned(),
        environment: Environment::from([(
            "TREEBOOT_ROOT_PATH".to_owned(),
            OsString::from(temp.path()),
        )]),
    };

    (temp, context)
}

fn write_file(path: &Path, content: &str) {
    std::fs::write(path, content).expect("file should be written");
}

fn span() -> SourceSpan {
    SourceSpan {
        start: 0,
        end: 0,
        line: 1,
        column: 1,
    }
}

fn copy_spec(context: &Worktree, source: &str, target: &str) -> FileOperation {
    FileOperation {
        operation: FileOperationKind::Copy,
        source: PathBuf::from(source),
        target: PathBuf::from(target),
        source_path: context.root_path.join(source),
        target_path: context.worktree_path.join(target),
        required: false,
        compare: None,
        delete: None,
        symlinks: Some(SymlinkMode::Preserve),
        declaration: span(),
    }
}

#[test]
fn public_api_should_discover_load_plan_and_execute_manifest() {
    let repo = git_worktree();
    write_file(&repo.root_path().join(".env"), "TOKEN=1\n");
    let config_path = repo.worktree_path().join(".treeboot.toml");
    write_file(&config_path, r#"copy = [".env"]"#);

    let worktree = Worktree::discover(WorktreeOptions {
        cwd: Some(repo.worktree_path().to_path_buf()),
        root: None,
    })
    .expect("worktree should be discovered");
    let config = Config::load(&config_path, &worktree).expect("config should load");
    let plan =
        ActionPlan::from_manifest(&config_path, &config, &worktree, RunPlanOptions::default())
            .expect("manifest plan should build");

    assert_eq!(plan.context, worktree);
    assert_eq!(plan.config_path.as_deref(), Some(config_path.as_path()));
    assert!(matches!(plan.origin, PlanOrigin::Manifest { ref path } if path == &config_path));
    assert_eq!(plan.files.len(), 1);
    assert!(plan.commands.is_empty());

    let mut reporter = VecReporter::default();
    let report = Executor::new(ExecuteOptions::default())
        .execute(&plan, &mut reporter)
        .expect("plan should execute");

    assert_eq!(report.file_action_count, 1);
    assert_eq!(
        std::fs::read_to_string(repo.worktree_path().join(".env"))
            .expect("copied file should be readable"),
        "TOKEN=1\n"
    );
    assert!(reporter.events.iter().any(|event| {
        matches!(
            event,
            OutputEvent::FileApplied {
                operation: FileOperationKind::Copy,
                source,
                target,
            } if source == Path::new(".env") && target == Path::new(".env")
        )
    }));
}

#[test]
fn public_api_should_parse_manifest_and_dry_run_commands() {
    let (_temp, context) = temp_worktree("dry-run-command");
    let config = Config::parse(
        Path::new(".treeboot.toml"),
        r#"commands = ["echo planned"]"#,
        &context,
    )
    .expect("config should parse");
    let plan = ActionPlan::from_manifest(
        Path::new(".treeboot.toml"),
        &config,
        &context,
        RunPlanOptions::default(),
    )
    .expect("manifest plan should build");

    let mut reporter = VecReporter::default();
    let report = Executor::new(ExecuteOptions {
        dry_run: true,
        ..ExecuteOptions::default()
    })
    .execute(&plan, &mut reporter)
    .expect("dry-run command plan should execute");

    assert_eq!(report.file_action_count, 0);
    assert!(reporter.events.iter().any(|event| {
        matches!(
            event,
            OutputEvent::CommandWouldRun { label } if label == "echo planned"
        )
    }));
}

#[test]
fn public_api_executor_should_skip_commands_when_requested() {
    let (_temp, context) = temp_worktree("skip-command");
    let config = Config::parse(
        Path::new(".treeboot.toml"),
        r#"commands = ["echo skipped"]"#,
        &context,
    )
    .expect("config should parse");
    let plan = ActionPlan::from_manifest(
        Path::new(".treeboot.toml"),
        &config,
        &context,
        RunPlanOptions::default(),
    )
    .expect("manifest plan should build");

    let mut reporter = VecReporter::default();
    let report = Executor::new(ExecuteOptions {
        dry_run: true,
        skip_commands: true,
        ..ExecuteOptions::default()
    })
    .execute(&plan, &mut reporter)
    .expect("plan should execute without commands");

    assert_eq!(report.file_action_count, 0);
    assert!(reporter.events.is_empty());
}

#[test]
fn public_api_should_build_manual_file_plan_without_config_path() {
    let (_temp, context) = temp_worktree("manual-plan");
    write_file(&context.root_path.join(".env"), "TOKEN=1\n");
    let files = vec![copy_spec(&context, ".env", "local.env")];

    let plan = ActionPlan::from_file_operations(
        &context,
        PlanOrigin::Manual {
            operation: FileOperationKind::Copy,
        },
        &files,
        RunPlanOptions::default(),
    )
    .expect("manual file plan should build");

    assert_eq!(plan.context, context);
    assert!(matches!(
        plan.origin,
        PlanOrigin::Manual {
            operation: FileOperationKind::Copy
        }
    ));
    assert_eq!(plan.config_path, None);
    assert_eq!(plan.files.len(), 1);
    assert!(plan.commands.is_empty());
}

#[test]
fn public_api_file_operation_plan_should_preserve_manifest_origin() {
    let (_temp, context) = temp_worktree("manifest-file-plan");
    write_file(&context.root_path.join(".env"), "TOKEN=1\n");
    let config_path = context.worktree_path.join(".treeboot.toml");
    let files = vec![copy_spec(&context, ".env", ".env")];

    let plan = ActionPlan::from_file_operations(
        &context,
        PlanOrigin::Manifest {
            path: config_path.clone(),
        },
        &files,
        RunPlanOptions::default(),
    )
    .expect("manifest-origin file plan should build");

    assert_eq!(plan.config_path.as_deref(), Some(config_path.as_path()));
    assert!(matches!(plan.origin, PlanOrigin::Manifest { path } if path == config_path));
}

#[test]
fn public_api_config_load_should_report_io_errors() {
    let (_temp, context) = temp_worktree("missing-config");
    let path = context.worktree_path.join("missing.toml");
    let error = Config::load(&path, &context).expect_err("missing config should fail");

    match error {
        Error::ConfigIo {
            path: error_path, ..
        } => assert_eq!(error_path, path),
        other => panic!("expected config I/O error, got {other:?}"),
    }
}
