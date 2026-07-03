use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;
use treeboot_core::{
    ActionPlan, ActionPlanOptions, Config, ConfigOptions, DiagnosticStatus, Environment,
    EnvironmentInput, Error, ExecuteOptions, Executor, FileOperation, FileOperationAction,
    FileOperationCompletionOptions, FileOperationKind, FileOperationOptions, FileOperationSummary,
    IgnoredInitScript, InitScriptDiscovery, InitScriptStatus, LoadedConfig,
    ManualFileOperationOptions, OutputEvent, PlanOrigin, Reporter, RunAction, RunOptions,
    SourceSpan, StatusOptions, SymlinkMode, Worktree, WorktreeOptions, check, config_schema_json,
    diagnose, file_operation_source_candidates, inspect_config, inspect_env, inspect_status,
    inspect_status_snapshot, run, run_file_operation, treeboot_version_info, version_info,
};

#[derive(Default)]
struct VecReporter {
    events: Vec<OutputEvent>,
    summaries: Vec<(FileOperationKind, PathBuf, PathBuf, FileOperationSummary)>,
}

impl Reporter for VecReporter {
    fn report(&mut self, event: OutputEvent) -> std::io::Result<()> {
        if let OutputEvent::FileOperationFinished {
            operation,
            source,
            target,
            summary,
            dry_run,
        } = &event
            && !dry_run
        {
            self.summaries
                .push((*operation, source.clone(), target.clone(), summary.clone()));
        }

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
    let root_env = root.as_os_str().to_os_string();

    let context = Worktree {
        root_path: root,
        worktree_path: worktree,
        default_branch: "main".to_owned(),
        environment: Environment::from([("TREEBOOT_ROOT_PATH".to_owned(), root_env)]),
    };

    (temp, context)
}

fn write_file(path: &Path, content: &str) {
    std::fs::write(path, content).expect("file should be written");
}

fn canonical_path(path: &Path) -> PathBuf {
    dunce::canonicalize(path).expect("path should canonicalize")
}

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = std::fs::metadata(path)
        .expect("script metadata should load")
        .permissions();
    permissions.set_mode(permissions.mode() | 0o111);
    std::fs::set_permissions(path, permissions).expect("script permissions should update");
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) {}

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
        glob: false,
        target_explicit: true,
        required: false,
        compare: None,
        delete: None,
        symlinks: Some(SymlinkMode::Preserve),
        ignore: Vec::new(),
        ignore_metadata: Vec::new(),
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
        environment: EnvironmentInput::empty(),
    })
    .expect("worktree should be discovered");
    let config = Config::load(&config_path, &worktree).expect("config should load");
    let plan = ActionPlan::from_manifest(
        &config_path,
        &config,
        &worktree,
        ActionPlanOptions::default(),
    )
    .expect("manifest plan should build");

    assert_eq!(plan.context(), &worktree);
    assert_eq!(plan.config_path(), Some(config_path.as_path()));
    assert!(matches!(plan.origin(), PlanOrigin::Manifest { path } if path == &config_path));
    assert_eq!(plan.files().len(), 1);
    assert!(plan.commands().is_empty());

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
    assert!(
        reporter
            .summaries
            .iter()
            .any(|(operation, source, target, summary)| {
                *operation == FileOperationKind::Copy
                    && source == Path::new(".env")
                    && target == Path::new(".env")
                    && summary.changed == 1
            })
    );
}

#[test]
fn public_api_worktree_discover_should_use_explicit_environment_input() {
    let repo = git_worktree();
    let alternate_root = TempDir::new().expect("alternate root should be created");
    let expected_root = canonical_path(alternate_root.path());

    let worktree = Worktree::discover(WorktreeOptions {
        cwd: Some(repo.worktree_path().to_path_buf()),
        root: None,
        environment: EnvironmentInput {
            treeboot_root_path: Some(OsString::from(alternate_root.path())),
            conductor_default_branch: Some(OsString::from("trunk")),
            ..EnvironmentInput::empty()
        },
    })
    .expect("worktree should be discovered");

    assert_eq!(worktree.root_path, expected_root);
    assert_eq!(worktree.default_branch, "trunk");
    assert_eq!(
        worktree.environment.get("CONDUCTOR_DEFAULT_BRANCH"),
        Some(&OsString::from("trunk"))
    );
}

#[test]
fn public_api_worktree_discover_should_use_next_non_empty_environment_alias() {
    let repo = git_worktree();
    let alternate_root = TempDir::new().expect("alternate root should be created");
    let expected_root = canonical_path(alternate_root.path());

    let worktree = Worktree::discover(WorktreeOptions {
        cwd: Some(repo.worktree_path().to_path_buf()),
        root: None,
        environment: EnvironmentInput {
            treeboot_root_path: Some(OsString::new()),
            codex_source_tree_path: Some(OsString::from(alternate_root.path())),
            ..EnvironmentInput::empty()
        },
    })
    .expect("worktree should be discovered");

    assert_eq!(worktree.root_path, expected_root);
}

#[test]
fn public_api_check_should_use_explicit_runtime_environment_input() {
    let repo = git_worktree();

    let error = check(treeboot_core::CheckOptions {
        cwd: Some(repo.root_path().to_path_buf()),
        environment: EnvironmentInput {
            treeboot_strict: Some(OsString::from("yes")),
            ..EnvironmentInput::empty()
        },
        ..treeboot_core::CheckOptions::default()
    })
    .expect_err("strict root checkout should fail");

    assert!(matches!(error, Error::RootWorktreeStrict));
}

#[test]
fn public_api_diagnose_should_report_explicit_default_branch_as_resolved() {
    let repo = git_worktree();

    let report = diagnose(treeboot_core::DoctorOptions {
        cwd: Some(repo.worktree_path().to_path_buf()),
        environment: EnvironmentInput {
            conductor_default_branch: Some(OsString::from("stable")),
            ..EnvironmentInput::empty()
        },
        no_init_script: true,
        ..treeboot_core::DoctorOptions::default()
    });
    let diagnostic = report
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.name == "default_branch")
        .expect("default_branch diagnostic should be present");

    assert_eq!(diagnostic.status, DiagnosticStatus::Ok);
    assert_eq!(
        report
            .context
            .as_ref()
            .expect("context should resolve")
            .default_branch,
        "stable"
    );
}

#[test]
fn public_api_should_expose_metadata_env_check_and_doctor() {
    let repo = git_worktree();
    write_file(&repo.root_path().join(".env"), "TOKEN=1\n");
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        r#"copy = [".env"]"#,
    );

    let version = version_info("treeboot", "0.4.1");
    assert_eq!(version.spec_version, treeboot_core::SPEC_VERSION);
    let treeboot_version = treeboot_version_info();
    assert_eq!(treeboot_version.package, "treeboot");
    assert_eq!(treeboot_version.version, treeboot_core::TREEBOOT_VERSION);
    assert_eq!(treeboot_version.spec_version, treeboot_core::SPEC_VERSION);
    assert!(config_schema_json().contains("\"$defs\""));

    let env = inspect_env(treeboot_core::EnvOptions {
        cwd: Some(repo.worktree_path().to_path_buf()),
        root: None,
        environment: EnvironmentInput::empty(),
    })
    .expect("environment should inspect");
    assert!(env.environment.contains_key("TREEBOOT_ROOT_PATH"));
    let env_json = serde_json::to_value(&env).expect("env should serialize");
    assert!(env_json.get("environment").is_none());
    assert!(env_json.get("TREEBOOT_ROOT_PATH").is_some());

    let checked = check(treeboot_core::CheckOptions {
        cwd: Some(repo.worktree_path().to_path_buf()),
        root: None,
        environment: EnvironmentInput::empty(),
        config: None,
        no_init_script: false,
        strict: false,
    })
    .expect("config should validate");
    assert!(matches!(
        checked.action,
        treeboot_core::CheckAction::Config { .. }
    ));

    let doctor = diagnose(treeboot_core::DoctorOptions {
        cwd: Some(repo.worktree_path().to_path_buf()),
        root: None,
        environment: EnvironmentInput::empty(),
        config: None,
        no_init_script: false,
        strict: false,
    });
    assert!(!doctor.has_fatal());
}

#[test]
fn public_api_should_load_discovered_manifest() {
    let repo = git_worktree();
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        r#"copy = ["README.md"]"#,
    );
    let worktree = Worktree::discover(WorktreeOptions {
        cwd: Some(repo.worktree_path().to_path_buf()),
        root: None,
        environment: EnvironmentInput::empty(),
    })
    .expect("worktree should be discovered");
    let config_path = worktree.worktree_path.join(".treeboot.toml");

    let report: LoadedConfig = Config::load_discovered(&worktree, None)
        .expect("config discovery should succeed")
        .expect("config should be found");

    assert_eq!(report.path, config_path);
    assert_eq!(report.context, worktree);
    assert_eq!(report.config.files.len(), 1);
}

#[test]
fn public_api_should_return_none_when_manifest_is_not_discovered() {
    let (_temp, worktree) = temp_worktree("missing-discovered-config");

    let report =
        Config::load_discovered(&worktree, None).expect("optional config discovery should succeed");

    assert_eq!(report, None);
}

#[test]
fn public_api_should_error_when_requested_manifest_is_missing() {
    let (_temp, worktree) = temp_worktree("missing-requested-config");
    let requested = Path::new("missing.toml");
    let error =
        Config::discover_path(&worktree, Some(requested)).expect_err("missing config should fail");

    match error {
        Error::ConfigNotFound(path) => {
            assert_eq!(path, worktree.worktree_path.join(requested));
        }
        other => panic!("expected missing config error, got {other:?}"),
    }
}

#[cfg(unix)]
#[test]
fn public_api_should_discover_executable_init_script_after_ignored_script() {
    let (_temp, worktree) = temp_worktree("init-script-discovery");
    let ignored = worktree.worktree_path.join(".treeboot.sh");
    let executable = worktree.worktree_path.join(".treebootrc");
    write_file(&ignored, "#!/bin/sh\n");
    write_file(&executable, "#!/bin/sh\n");
    make_executable(&executable);

    let discovery = InitScriptDiscovery::discover(&worktree);

    assert_eq!(discovery.executable.as_deref(), Some(executable.as_path()));
    assert_eq!(
        discovery.ignored,
        vec![IgnoredInitScript {
            path: ignored,
            reason: "not_executable",
        }]
    );
}

#[test]
fn public_api_run_should_report_init_script_in_dry_run() {
    let repo = git_worktree();
    let script = repo.worktree_path().join(".treeboot.sh");
    write_file(&script, "#!/usr/bin/env sh\nexit 0\n");
    make_executable(&script);
    let expected_script = canonical_path(&script);

    let mut reporter = VecReporter::default();
    let report = run(
        RunOptions {
            cwd: Some(repo.worktree_path().to_path_buf()),
            dry_run: true,
            ..RunOptions::default()
        },
        &mut reporter,
    )
    .expect("run should dry-run init script");

    assert!(matches!(
        report.action,
        RunAction::WouldRunInitScript { path } if path == expected_script
    ));
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
        ActionPlanOptions::default(),
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
fn public_api_inspect_status_should_report_context_and_config_without_parsing() {
    let repo = git_worktree();
    let config_path = repo.worktree_path().join(".treeboot.toml");
    write_file(&config_path, "invalid toml = [\n");
    let expected_worktree = canonical_path(repo.worktree_path());
    let expected_root = canonical_path(repo.root_path());
    let expected_config = canonical_path(&config_path);

    let report = inspect_status(StatusOptions {
        cwd: Some(repo.worktree_path().to_path_buf()),
        ..StatusOptions::default()
    })
    .expect("status should inspect without parsing config");

    assert_eq!(report.context.worktree_path, expected_worktree);
    assert_eq!(report.context.root_path, expected_root);
    assert_eq!(report.config.as_deref(), Some(expected_config.as_path()));
    assert!(matches!(
        report.init_script,
        InitScriptStatus::NotFound { ref ignored } if ignored.is_empty()
    ));

    let snapshot = inspect_status_snapshot(StatusOptions {
        cwd: Some(repo.worktree_path().to_path_buf()),
        ..StatusOptions::default()
    })
    .expect("status snapshot should inspect without parsing config");
    assert_eq!(snapshot.context.worktree_path, expected_worktree);
    assert_eq!(snapshot.context.root_path, expected_root);
    assert_eq!(snapshot.config.as_deref(), Some(expected_config.as_path()));
}

#[cfg(unix)]
#[test]
fn public_api_inspect_status_should_report_ignored_init_script_details() {
    let repo = git_worktree();
    let script = repo.worktree_path().join(".treeboot.sh");
    write_file(&script, "#!/bin/sh\n");
    let expected_script = canonical_path(&script);

    let report = inspect_status(StatusOptions {
        cwd: Some(repo.worktree_path().to_path_buf()),
        ..StatusOptions::default()
    })
    .expect("status should inspect init script candidates");

    let InitScriptStatus::NotFound { ignored } = report.init_script else {
        panic!("expected not_found init script status");
    };
    assert_eq!(
        ignored,
        vec![IgnoredInitScript {
            path: expected_script.clone(),
            reason: "not_executable",
        }]
    );

    let snapshot = inspect_status_snapshot(StatusOptions {
        cwd: Some(repo.worktree_path().to_path_buf()),
        ..StatusOptions::default()
    })
    .expect("status snapshot should inspect init script candidates");

    let InitScriptStatus::NotFound { ignored } = snapshot.init_script else {
        panic!("expected not_found init script status");
    };
    assert_eq!(
        ignored,
        vec![IgnoredInitScript {
            path: expected_script,
            reason: "not_executable",
        }]
    );
}

#[cfg(unix)]
#[test]
fn public_api_inspect_status_should_report_executable_init_script() {
    let repo = git_worktree();
    let script = repo.worktree_path().join(".treeboot.sh");
    write_file(&script, "#!/bin/sh\n");
    make_executable(&script);
    let expected_script = canonical_path(&script);

    let report = inspect_status(StatusOptions {
        cwd: Some(repo.worktree_path().to_path_buf()),
        ..StatusOptions::default()
    })
    .expect("status should inspect init script");

    assert!(matches!(
        report.init_script,
        InitScriptStatus::Found { ref path } if path == &expected_script
    ));
}

#[test]
fn public_api_inspect_status_should_use_explicit_environment_input() {
    let repo = git_worktree();
    let alternate_root = TempDir::new().expect("alternate root should be created");
    let expected_root = canonical_path(alternate_root.path());

    let report = inspect_status(StatusOptions {
        cwd: Some(repo.worktree_path().to_path_buf()),
        environment: EnvironmentInput {
            treeboot_root_path: Some(OsString::from(alternate_root.path())),
            conductor_default_branch: Some(OsString::from("release")),
            ..EnvironmentInput::empty()
        },
        no_init_script: true,
        ..StatusOptions::default()
    })
    .expect("status should inspect with explicit environment");

    assert_eq!(report.context.root_path, expected_root);
    assert_eq!(report.context.default_branch, "release");
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
        ActionPlanOptions::default(),
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
    let mut options = ManualFileOperationOptions::copy(vec![PathBuf::from(".env")]);
    options.target = Some(PathBuf::from("local.env"));
    options.symlinks = Some(SymlinkMode::Preserve);

    let files = FileOperation::from_manual_options(&context, options)
        .expect("manual file specs should build");

    let plan = ActionPlan::from_file_operations(
        &context,
        PlanOrigin::Manual {
            operation: FileOperationKind::Copy,
        },
        &files,
        ActionPlanOptions::default(),
    )
    .expect("manual file plan should build");

    assert_eq!(plan.context(), &context);
    assert!(matches!(
        plan.origin(),
        PlanOrigin::Manual {
            operation: FileOperationKind::Copy
        }
    ));
    assert_eq!(plan.config_path(), None);
    assert_eq!(plan.files().len(), 1);
    assert!(plan.commands().is_empty());
}

#[test]
fn public_api_file_operation_option_constructors_should_set_operation_and_sources() {
    for (options, operation) in [
        (
            ManualFileOperationOptions::copy(vec![PathBuf::from("copy")]),
            FileOperationKind::Copy,
        ),
        (
            ManualFileOperationOptions::symlink(vec![PathBuf::from("symlink")]),
            FileOperationKind::Symlink,
        ),
        (
            ManualFileOperationOptions::sync(vec![PathBuf::from("sync")]),
            FileOperationKind::Sync,
        ),
    ] {
        assert_eq!(options.operation, operation);
        assert_eq!(options.sources, vec![PathBuf::from(operation.as_str())]);
        assert_eq!(
            options,
            ManualFileOperationOptions {
                operation,
                sources: vec![PathBuf::from(operation.as_str())],
                ..ManualFileOperationOptions::default()
            }
        );
    }

    for (options, operation) in [
        (
            FileOperationOptions::copy(vec![PathBuf::from("copy")]),
            FileOperationKind::Copy,
        ),
        (
            FileOperationOptions::symlink(vec![PathBuf::from("symlink")]),
            FileOperationKind::Symlink,
        ),
        (
            FileOperationOptions::sync(vec![PathBuf::from("sync")]),
            FileOperationKind::Sync,
        ),
    ] {
        assert_eq!(options.operation, operation);
        assert_eq!(options.sources, vec![PathBuf::from(operation.as_str())]);
        assert_eq!(
            options,
            FileOperationOptions {
                operation,
                sources: vec![PathBuf::from(operation.as_str())],
                environment: EnvironmentInput::empty(),
                ..FileOperationOptions::default()
            }
        );
    }
}

#[test]
fn public_api_run_file_operation_should_apply_manual_copy() {
    let repo = git_worktree();
    write_file(&repo.root_path().join(".env"), "TOKEN=1\n");
    let mut reporter = VecReporter::default();

    let report = run_file_operation(
        {
            let mut options = FileOperationOptions::copy(vec![PathBuf::from(".env")]);
            options.cwd = Some(repo.worktree_path().to_path_buf());
            options
        },
        &mut reporter,
    )
    .expect("manual copy should run");

    assert_eq!(report.action, FileOperationAction::Applied);
    assert_eq!(
        std::fs::read_to_string(repo.worktree_path().join(".env"))
            .expect("copied file should be readable"),
        "TOKEN=1\n"
    );
}

#[test]
fn public_api_file_operation_source_candidates_should_use_explicit_environment_input() {
    let repo = git_worktree();
    let alternate_root = TempDir::new().expect("alternate root should be created");
    write_file(&alternate_root.path().join(".env"), "TOKEN=1\n");
    std::fs::create_dir_all(alternate_root.path().join("shared/nested"))
        .expect("shared directory should be created");

    let candidates = file_operation_source_candidates(FileOperationCompletionOptions {
        cwd: Some(repo.worktree_path().to_path_buf()),
        root: None,
        environment: EnvironmentInput {
            treeboot_root_path: Some(OsString::new()),
            codex_source_tree_path: Some(OsString::from(alternate_root.path())),
            ..EnvironmentInput::empty()
        },
        current: PathBuf::new(),
    });

    assert_eq!(
        candidates,
        vec![
            ".env".to_owned(),
            format!("shared{}", std::path::MAIN_SEPARATOR),
        ]
    );
}

#[test]
fn public_api_inspect_config_should_load_normalized_manifest() {
    let repo = git_worktree();
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        "copy = [\"README.md\"]\n",
    );

    let report = inspect_config(ConfigOptions {
        cwd: Some(repo.worktree_path().to_path_buf()),
        root: None,
        environment: EnvironmentInput::empty(),
        config: None,
    })
    .expect("config should inspect");

    assert_eq!(report.config.files.len(), 1);
    assert_eq!(report.config.files[0].source, PathBuf::from("README.md"));
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
        ActionPlanOptions::default(),
    )
    .expect("manifest-origin file plan should build");

    assert_eq!(plan.config_path(), Some(config_path.as_path()));
    assert!(matches!(plan.origin(), PlanOrigin::Manifest { path } if path == &config_path));
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
