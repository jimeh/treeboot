use predicates::prelude::*;

mod common;

use common::{git_repo, git_worktree, toml_string_path, treeboot, write_file};

#[test]
fn teardown_should_reject_root_checkout() {
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

#[test]
fn teardown_should_noop_when_discovered_config_is_missing() {
    let repo = git_worktree();

    treeboot()
        .arg("teardown")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout("treeboot: no config detected\n");
}

#[test]
fn teardown_should_fail_when_requested_config_is_missing() {
    let repo = git_worktree();

    treeboot()
        .args(["teardown", "--config", "missing.toml", "--yes"])
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("config file not found"));
}

#[test]
fn teardown_should_noop_without_configured_commands() {
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

#[test]
fn teardown_should_require_yes_when_input_is_not_a_terminal() {
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

#[test]
fn teardown_dry_run_should_not_require_approval_or_spawn() {
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
#[test]
fn teardown_yes_should_run_only_teardown_commands() {
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
#[test]
fn teardown_can_target_linked_worktree_from_root() {
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
#[test]
fn bootstrap_semantic_failure_should_not_block_teardown() {
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

#[test]
fn whole_config_parse_failure_should_block_teardown() {
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
        .stderr(predicate::str::contains(
            "`run` and `program` are mutually exclusive",
        ));
}

#[test]
fn teardown_should_ignore_bootstrap_strict_environment() {
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
#[test]
fn teardown_should_continue_after_allowed_failure() {
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
#[test]
fn teardown_should_stop_after_fatal_failure() {
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
