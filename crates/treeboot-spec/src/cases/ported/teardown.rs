use predicates::prelude::*;

#[cfg(unix)]
use crate::cases::support::git;
use crate::cases::support::{git_repo, git_worktree, toml_string_path, treeboot, write_file};

pub(crate) fn teardown_should_reject_root_checkout() {
    let repo = git_repo();

    treeboot()
        .arg("teardown")
        .current_dir(repo.path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "teardown is only valid for a linked worktree",
        ));
}

pub(crate) fn teardown_should_reject_actual_root_with_explicit_source_root_override() {
    let repo = git_worktree();
    let alternate_root = tempfile::TempDir::new().expect("alternate root should be created");

    treeboot()
        .args(["teardown", "--root"])
        .arg(alternate_root.path())
        .arg("--dry-run")
        .current_dir(repo.root_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "teardown is only valid for a linked worktree",
        ));
}

pub(crate) fn teardown_should_reject_actual_root_with_source_root_environment_aliases() {
    let repo = git_worktree();
    let alternate_root = tempfile::TempDir::new().expect("alternate root should be created");

    for variable in [
        "TREEBOOT_ROOT_PATH",
        "CODEX_SOURCE_TREE_PATH",
        "CONDUCTOR_ROOT_PATH",
        "SUPERSET_ROOT_PATH",
    ] {
        treeboot()
            .args(["teardown", "--dry-run"])
            .env(variable, alternate_root.path())
            .current_dir(repo.root_path())
            .assert()
            .code(1)
            .stderr(predicate::str::contains(
                "teardown is only valid for a linked worktree",
            ));
    }
}

pub(crate) fn teardown_should_noop_when_discovered_config_is_missing() {
    let repo = git_worktree();

    treeboot()
        .arg("teardown")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout("treeboot: no config detected\n");
}

pub(crate) fn teardown_should_fail_when_requested_config_is_missing() {
    let repo = git_worktree();

    treeboot()
        .args(["teardown", "--config", "missing.toml", "--yes"])
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("config file not found"));
}

pub(crate) fn teardown_should_noop_without_configured_commands() {
    let repo = git_worktree();
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        "commands = []\n",
    );

    treeboot()
        .arg("teardown")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "treeboot: no teardown commands configured",
        ));
}

pub(crate) fn teardown_should_require_yes_when_input_is_not_a_terminal() {
    let repo = git_worktree();
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        "teardown_commands = [\"echo cleanup\"]\n",
    );

    treeboot()
        .arg("teardown")
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stdout(predicate::str::contains("treeboot: config detected"))
        .stderr(predicate::str::contains("rerun with --yes"));
}

pub(crate) fn teardown_dry_run_should_not_require_approval_or_spawn() {
    let repo = git_worktree();
    let marker = repo.worktree_path().join("teardown-marker");
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        &format!(
            "teardown_commands = [{{ run = \"touch {}\", name = \"Cleanup\" }}]\n",
            toml_string_path(&marker)
        ),
    );

    treeboot()
        .args(["teardown", "--dry-run"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "treeboot: teardown would run Cleanup:",
        ));
    assert!(!marker.exists());
}

#[cfg(unix)]
pub(crate) fn teardown_yes_should_run_only_teardown_commands() {
    let repo = git_worktree();
    let bootstrap = repo.worktree_path().join("bootstrap-marker");
    let teardown = repo.worktree_path().join("teardown-marker");
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        &format!(
            "commands = [\"touch {}\"]\n\
             teardown_commands = [\"touch {}\"]\n",
            toml_string_path(&bootstrap),
            toml_string_path(&teardown)
        ),
    );

    treeboot()
        .args(["teardown", "--yes"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: teardown run"));

    assert!(!bootstrap.exists());
    assert!(teardown.exists());
}

#[cfg(unix)]
pub(crate) fn bootstrap_and_teardown_commands_should_share_worktree_identity_after_branch_rename() {
    let repo = git_worktree();
    let bootstrap_shell = repo.worktree_path().join("bootstrap-shell.out");
    let bootstrap_direct = repo.worktree_path().join("bootstrap-direct.out");
    let teardown_shell = repo.worktree_path().join("teardown-shell.out");
    let teardown_direct = repo.worktree_path().join("teardown-direct.out");
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        &format!(
            r#"
worktree_id = {{ length = 8 }}
worktree_slug = {{ max_length = 32, separator = "_" }}
commands = [
  {{ run = "printf '%s|%s' \"$TREEBOOT_WORKTREE_ID\" \"$TREEBOOT_WORKTREE_SLUG\" > {}" }},
  {{ program = "sh", args = ["-c", "printf '%s|%s' \"$TREEBOOT_WORKTREE_ID\" \"$TREEBOOT_WORKTREE_SLUG\" > {}"] }},
]
teardown_commands = [
  {{ run = "printf '%s|%s' \"$TREEBOOT_WORKTREE_ID\" \"$TREEBOOT_WORKTREE_SLUG\" > {}" }},
  {{ program = "sh", args = ["-c", "printf '%s|%s' \"$TREEBOOT_WORKTREE_ID\" \"$TREEBOOT_WORKTREE_SLUG\" > {}"] }},
]
"#,
            toml_string_path(&bootstrap_shell),
            toml_string_path(&bootstrap_direct),
            toml_string_path(&teardown_shell),
            toml_string_path(&teardown_direct),
        ),
    );

    treeboot()
        .arg("run")
        .current_dir(repo.worktree_path())
        .assert()
        .success();
    git(
        &["branch", "-m", "renamed-after-bootstrap"],
        repo.worktree_path(),
    );
    treeboot()
        .args(["teardown", "--yes"])
        .current_dir(repo.worktree_path())
        .assert()
        .success();

    let expected =
        std::fs::read_to_string(bootstrap_shell).expect("bootstrap shell output should exist");
    assert_eq!(
        std::fs::read_to_string(bootstrap_direct).expect("bootstrap direct output should exist"),
        expected
    );
    assert_eq!(
        std::fs::read_to_string(teardown_shell).expect("teardown shell output should exist"),
        expected
    );
    assert_eq!(
        std::fs::read_to_string(teardown_direct).expect("teardown direct output should exist"),
        expected
    );
    let (id, slug) = expected
        .split_once('|')
        .expect("command output should contain both identity values");
    assert_eq!(id.len(), 8);
    assert!(slug.ends_with(id), "slug should end with the complete ID");
}

#[cfg(unix)]
pub(crate) fn teardown_can_target_linked_worktree_from_root() {
    let repo = git_worktree();
    let marker = repo.worktree_path().join("targeted-marker");
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        &format!(
            "teardown_commands = [\"touch {}\"]\n",
            toml_string_path(&marker)
        ),
    );

    treeboot()
        .args(["teardown", "--worktree"])
        .arg(repo.worktree_path())
        .arg("--yes")
        .current_dir(repo.root_path())
        .assert()
        .success();

    assert!(marker.exists());
}

#[cfg(unix)]
pub(crate) fn bootstrap_semantic_failure_should_not_block_teardown() {
    let repo = git_worktree();
    let marker = repo.worktree_path().join("cleanup-marker");
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        &format!(
            "copy = [{{ source = \"missing\", required = true }}]\n\
             teardown_commands = [\"touch {}\"]\n",
            toml_string_path(&marker)
        ),
    );

    treeboot()
        .args(["teardown", "--yes"])
        .current_dir(repo.worktree_path())
        .assert()
        .success();

    assert!(marker.exists());
}

#[cfg(unix)]
pub(crate) fn bootstrap_command_cwd_escape_should_not_block_teardown() {
    let repo = git_worktree();
    let marker = repo.worktree_path().join("cleanup-after-cwd-error");
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        &format!(
            "commands = [{{ run = \"echo bootstrap\", cwd = \"..\" }}]\n\
             teardown_commands = [\"touch {}\"]\n",
            toml_string_path(&marker)
        ),
    );

    treeboot()
        .args(["teardown", "--yes"])
        .current_dir(repo.worktree_path())
        .assert()
        .success();

    assert!(marker.exists());
}

pub(crate) fn whole_config_parse_failure_should_block_teardown() {
    let repo = git_worktree();
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        "commands = [{ run = \"echo bootstrap\", program = \"echo\" }]\n\
         teardown_commands = [\"echo teardown\"]\n",
    );

    treeboot()
        .args(["teardown", "--yes"])
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stdout(predicate::str::contains("treeboot: config detected"))
        .stderr(predicate::str::contains(
            "`run` and `program` are mutually exclusive",
        ));
}

pub(crate) fn teardown_worktree_identity_overrides_should_fail_before_any_command() {
    for variable in ["TREEBOOT_WORKTREE_ID", "TREEBOOT_WORKTREE_SLUG"] {
        let repo = git_worktree();
        let marker = repo.worktree_path().join("should-not-run");
        write_file(
            &repo.worktree_path().join(".treeboot.toml"),
            &format!(
                r#"
teardown_commands = [
  {{ run = "touch {}", env = {{ {variable} = "override" }} }},
  {{ run = "touch {}" }},
]
"#,
                toml_string_path(&marker),
                toml_string_path(&marker),
            ),
        );

        treeboot()
            .args(["teardown", "--yes"])
            .current_dir(repo.worktree_path())
            .assert()
            .failure()
            .stderr(predicate::str::contains(variable))
            .stderr(predicate::str::contains(
                "overrides treeboot-owned variable",
            ));

        assert!(!marker.exists());
    }
}

pub(crate) fn teardown_should_ignore_bootstrap_strict_environment() {
    let repo = git_worktree();

    treeboot()
        .arg("teardown")
        .env("TREEBOOT_STRICT", "not-a-bool")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout("treeboot: no config detected\n");
}

#[cfg(unix)]
pub(crate) fn teardown_should_continue_after_allowed_failure() {
    let repo = git_worktree();
    let marker = repo.worktree_path().join("continued-marker");
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        &format!(
            "teardown_commands = [\n\
             {{ name = \"Optional\", run = \"exit 7\", allow_failure = true }},\n\
             \"touch {}\",\n\
             ]\n",
            toml_string_path(&marker)
        ),
    );

    treeboot()
        .args(["teardown", "--yes"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "treeboot: warning: teardown command Optional: exit 7 failed",
        ));

    assert!(marker.exists());
}

#[cfg(unix)]
pub(crate) fn teardown_should_stop_after_fatal_failure() {
    let repo = git_worktree();
    let marker = repo.worktree_path().join("must-not-exist");
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        &format!(
            "teardown_commands = [\"exit 7\", \"touch {}\"]\n",
            toml_string_path(&marker)
        ),
    );

    treeboot()
        .args(["teardown", "--yes"])
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("command exit 7 failed"));

    assert!(!marker.exists());
}
