use std::ffi::{OsStr, OsString};
use std::path::Path;

use predicates::prelude::*;

mod common;

use common::{display_path, git_repo, git_worktree, treeboot, write_file};

#[cfg(unix)]
use common::write_executable_script;

const COMPLETION_SHELLS: [&str; 5] = ["bash", "zsh", "fish", "powershell", "elvish"];

fn complete_source_candidates<I, S>(
    shell: &str,
    args: I,
    current_dir: &Path,
) -> assert_cmd::assert::Assert
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let args = args
        .into_iter()
        .map(|arg| arg.as_ref().to_os_string())
        .collect::<Vec<OsString>>();
    let index = args.len().saturating_sub(1).to_string();
    let mut command = treeboot();

    command
        .env("COMPLETE", shell)
        .env("_CLAP_COMPLETE_INDEX", index)
        .env("_CLAP_IFS", "\n")
        .arg("--")
        .args(args)
        .current_dir(current_dir);

    command.assert()
}

#[test]
fn manual_commands_should_require_sources() {
    for command in ["copy", "symlink", "sync"] {
        treeboot()
            .arg(command)
            .assert()
            .code(2)
            .stderr(predicate::str::contains("required"));
    }
}

#[test]
fn copy_should_create_files_and_directories_from_root() {
    let repo = git_worktree();
    write_file(&repo.root_path().join(".env"), "TOKEN=1\n");
    std::fs::create_dir_all(repo.root_path().join("shared/nested"))
        .expect("source directory should be created");
    write_file(&repo.root_path().join("shared/nested/config"), "value\n");

    treeboot()
        .args(["copy", ".env", "shared"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: copy .env -> .env"))
        .stdout(predicate::str::contains("treeboot: copy shared -> shared"));

    assert_eq!(
        std::fs::read_to_string(repo.worktree_path().join(".env"))
            .expect("copied file should be readable"),
        "TOKEN=1\n"
    );
    assert_eq!(
        std::fs::read_to_string(repo.worktree_path().join("shared/nested/config"))
            .expect("copied nested file should be readable"),
        "value\n"
    );
}

#[test]
fn copy_should_place_multiple_sources_under_target_prefix() {
    let repo = git_worktree();
    write_file(&repo.root_path().join("a"), "a\n");
    write_file(&repo.root_path().join("c"), "c\n");

    treeboot()
        .args(["copy", "a", "c", "--target", "local"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "treeboot: copy a -> {}",
            display_path("local/a")
        )))
        .stdout(predicate::str::contains(format!(
            "treeboot: copy c -> {}",
            display_path("local/c")
        )));

    assert_eq!(
        std::fs::read_to_string(repo.worktree_path().join("local/a"))
            .expect("first target should be readable"),
        "a\n"
    );
    assert_eq!(
        std::fs::read_to_string(repo.worktree_path().join("local/c"))
            .expect("second target should be readable"),
        "c\n"
    );
}

#[cfg(unix)]
#[test]
fn symlink_should_create_relative_link() {
    let repo = git_worktree();
    write_file(&repo.root_path().join(".tool-versions"), "rust latest\n");

    treeboot()
        .args([
            "symlink",
            ".tool-versions",
            "--target",
            "config/tool-versions",
        ])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "treeboot: symlink .tool-versions -> config/tool-versions",
        ));

    let link = repo.worktree_path().join("config/tool-versions");
    let target = std::fs::read_link(&link).expect("target should be a symlink");
    assert!(target.is_relative());
    assert_eq!(
        std::fs::canonicalize(link).expect("link should resolve"),
        std::fs::canonicalize(repo.root_path().join(".tool-versions"))
            .expect("source should canonicalize")
    );
}

#[test]
fn sync_should_update_changed_files() {
    let repo = git_worktree();
    std::fs::create_dir_all(repo.root_path().join("shared"))
        .expect("source directory should be created");
    std::fs::create_dir_all(repo.worktree_path().join("shared"))
        .expect("target directory should be created");
    write_file(&repo.root_path().join("shared/config"), "new\n");
    write_file(&repo.worktree_path().join("shared/config"), "old\n");

    treeboot()
        .args(["sync", "shared", "--compare", "checksum"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "treeboot: sync {} -> {}",
            display_path("shared/config"),
            display_path("shared/config")
        )));

    assert_eq!(
        std::fs::read_to_string(repo.worktree_path().join("shared/config"))
            .expect("synced file should be readable"),
        "new\n"
    );
}

#[cfg(unix)]
#[test]
fn sync_should_honor_ignore_metadata_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let repo = git_worktree();
    let source = repo.root_path().join("config");
    let target = repo.worktree_path().join("config");
    write_file(&source, "value\n");
    write_file(&target, "value\n");
    let modified = std::fs::metadata(&source)
        .expect("source metadata should be readable")
        .modified()
        .expect("source mtime should be readable");
    std::fs::File::options()
        .write(true)
        .open(&target)
        .and_then(|file| file.set_times(std::fs::FileTimes::new().set_modified(modified)))
        .expect("target mtime should match source");
    let mut source_permissions = std::fs::metadata(&source)
        .expect("source metadata should be readable")
        .permissions();
    source_permissions.set_mode(0o600);
    std::fs::set_permissions(&source, source_permissions).expect("source mode should be set");
    let mut target_permissions = std::fs::metadata(&target)
        .expect("target metadata should be readable")
        .permissions();
    target_permissions.set_mode(0o644);
    std::fs::set_permissions(&target, target_permissions).expect("target mode should be set");

    treeboot()
        .args(["sync", "config", "--ignore-metadata", "permissions"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::is_empty());

    let mode = std::fs::metadata(&target)
        .expect("target metadata should be readable")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o644);
}

#[test]
fn sync_delete_should_remove_target_only_files() {
    let repo = git_worktree();
    std::fs::create_dir_all(repo.root_path().join("shared"))
        .expect("source directory should be created");
    std::fs::create_dir_all(repo.worktree_path().join("shared"))
        .expect("target directory should be created");
    write_file(&repo.root_path().join("shared/config"), "value\n");
    write_file(&repo.worktree_path().join("shared/extra"), "remove\n");

    treeboot()
        .args(["sync", "shared", "--delete"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "treeboot: delete {}",
            display_path("shared/extra")
        )));

    assert!(!repo.worktree_path().join("shared/extra").exists());
}

#[test]
fn sync_no_delete_should_preserve_target_only_files() {
    let repo = git_worktree();
    std::fs::create_dir_all(repo.root_path().join("shared"))
        .expect("source directory should be created");
    std::fs::create_dir_all(repo.worktree_path().join("shared"))
        .expect("target directory should be created");
    write_file(&repo.root_path().join("shared/config"), "value\n");
    write_file(&repo.worktree_path().join("shared/extra"), "keep\n");

    treeboot()
        .args(["sync", "shared", "--no-delete"])
        .current_dir(repo.worktree_path())
        .assert()
        .success();

    assert_eq!(
        std::fs::read_to_string(repo.worktree_path().join("shared/extra"))
            .expect("extra file should remain"),
        "keep\n"
    );
}

#[test]
fn dry_run_should_report_without_mutation() {
    let repo = git_worktree();
    write_file(&repo.root_path().join(".env"), "TOKEN=1\n");

    treeboot()
        .args(["copy", ".env", "--dry-run"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "treeboot: would copy .env -> .env",
        ));

    assert!(!repo.worktree_path().join(".env").exists());
}

#[test]
fn force_should_replace_supported_targets() {
    let repo = git_worktree();
    write_file(&repo.root_path().join(".env"), "new\n");
    write_file(&repo.worktree_path().join(".env"), "old\n");

    treeboot()
        .args(["copy", ".env", "--force"])
        .current_dir(repo.worktree_path())
        .assert()
        .success();

    assert_eq!(
        std::fs::read_to_string(repo.worktree_path().join(".env"))
            .expect("target should be readable"),
        "new\n"
    );
}

#[test]
fn root_checkout_should_skip_manual_file_operations() {
    let repo = git_repo();
    write_file(&repo.path().join(".env"), "TOKEN=1\n");

    treeboot()
        .args(["copy", ".env"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "treeboot: This is not a work tree",
        ))
        .stdout(predicate::str::contains("treeboot: copy").not());
}

#[test]
fn root_checkout_should_skip_manual_config_parsing() {
    let repo = git_repo();
    write_file(&repo.path().join(".env"), "TOKEN=1\n");
    write_file(&repo.path().join(".treeboot.toml"), "invalid = [\n");

    treeboot()
        .args(["copy", ".env"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "treeboot: This is not a work tree",
        ))
        .stderr(predicate::str::contains("invalid config").not());
}

#[test]
fn strict_root_checkout_should_fail_manual_file_operations() {
    let repo = git_repo();

    treeboot()
        .args(["copy", ".env", "--strict"])
        .current_dir(repo.path())
        .assert()
        .code(1)
        .stdout(predicate::str::contains(
            "treeboot: This is not a work tree",
        ))
        .stderr(predicate::str::contains(
            "treeboot: This is not a work tree",
        ));
}

#[test]
fn invalid_operation_specific_flags_should_be_usage_errors() {
    treeboot()
        .args(["copy", ".env", "--compare", "checksum"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("unexpected argument"));

    treeboot()
        .args(["symlink", ".env", "--symlinks", "preserve"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("unexpected argument"));

    treeboot()
        .args(["symlink", ".env", "--ignore-metadata", "permissions"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("unexpected argument"));
}

#[test]
fn required_missing_source_should_fail_without_config_locations() {
    let repo = git_worktree();

    treeboot()
        .args(["copy", "missing", "--required"])
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("invalid copy file operation"))
        .stderr(predicate::str::contains("required source does not exist"))
        .stderr(predicate::str::contains("invalid config").not())
        .stderr(predicate::str::contains("line").not());
}

#[test]
fn duplicate_manual_targets_should_fail_before_side_effects() {
    let repo = git_worktree();
    write_file(&repo.root_path().join("a"), "value\n");

    treeboot()
        .args(["copy", "a", "./a"])
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("invalid copy file operation"))
        .stderr(predicate::str::contains("duplicate target"));

    assert!(!repo.worktree_path().join("a").exists());
}

#[test]
fn overlapping_manual_sync_delete_targets_should_fail_before_side_effects() {
    let repo = git_worktree();
    std::fs::create_dir_all(repo.root_path().join("shared/nested"))
        .expect("nested source should be created");
    write_file(&repo.root_path().join("shared/nested/config"), "value\n");

    treeboot()
        .args(["sync", "--delete", "shared", "shared/nested"])
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("invalid sync file operation"))
        .stderr(predicate::str::contains("overlapping targets"));

    assert!(!repo.worktree_path().join("shared").exists());
}

#[cfg(unix)]
#[test]
fn unsafe_source_symlink_should_fail_before_side_effects() {
    let repo = git_worktree();
    let outside = repo
        .root_path()
        .parent()
        .expect("root should have parent")
        .join("outside-secret");
    write_file(&outside, "secret\n");
    std::fs::create_dir_all(repo.root_path().join("unsafe"))
        .expect("source directory should be created");
    std::os::unix::fs::symlink(&outside, repo.root_path().join("unsafe/link"))
        .expect("unsafe symlink should be created");

    treeboot()
        .args(["copy", "unsafe"])
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("invalid copy file operation"))
        .stderr(predicate::str::contains("unsafe symlink"));

    assert!(!repo.worktree_path().join("unsafe").exists());
}

#[test]
fn manual_commands_should_fail_on_invalid_config_before_side_effects() {
    let repo = git_worktree();
    write_file(&repo.root_path().join(".env"), "TOKEN=1\n");
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        "invalid = [\n",
    );

    treeboot()
        .args(["copy", ".env"])
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("invalid config"));

    assert!(!repo.worktree_path().join(".env").exists());
}

#[test]
fn manual_commands_should_use_config_runtime_policy() {
    let repo = git_worktree();
    let outside = repo
        .root_path()
        .parent()
        .expect("root should have parent")
        .join("outside.env");
    write_file(&outside, "TOKEN=1\n");
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        "dangerously_allow_sources_outside_root = true\n",
    );

    treeboot()
        .args(["copy", "--target", "copied.env", "../outside.env"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "treeboot: copy ../outside.env -> copied.env",
        ));

    assert_eq!(
        std::fs::read_to_string(repo.worktree_path().join("copied.env"))
            .expect("outside source should be copied"),
        "TOKEN=1\n"
    );
}

#[test]
fn manual_commands_should_reject_targets_outside_worktree_by_default() {
    let repo = git_worktree();
    let outside = repo
        .worktree_path()
        .parent()
        .expect("worktree should have parent")
        .join("outside-target.env");
    write_file(&repo.root_path().join(".env"), "TOKEN=1\n");

    treeboot()
        .args([
            OsStr::new("copy"),
            OsStr::new("--target"),
            outside.as_os_str(),
            OsStr::new(".env"),
        ])
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("invalid copy file operation"))
        .stderr(predicate::str::contains("target resolves outside worktree"));

    assert!(!outside.exists());
}

#[test]
fn manual_commands_should_allow_configured_targets_outside_worktree() {
    let repo = git_worktree();
    let outside = repo
        .worktree_path()
        .parent()
        .expect("worktree should have parent")
        .join("outside-target.env");
    write_file(&repo.root_path().join(".env"), "TOKEN=1\n");
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        "dangerously_allow_targets_outside_worktree = true\n",
    );

    treeboot()
        .args([
            OsStr::new("copy"),
            OsStr::new("--target"),
            outside.as_os_str(),
            OsStr::new(".env"),
        ])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: copy .env ->"));

    assert_eq!(
        std::fs::read_to_string(&outside).expect("outside target should be copied"),
        "TOKEN=1\n"
    );
}

#[test]
fn manual_config_strict_should_reject_sync_before_side_effects() {
    let repo = git_worktree();
    std::fs::create_dir_all(repo.root_path().join("shared"))
        .expect("source directory should be created");
    write_file(&repo.root_path().join("shared/config"), "value\n");
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        "strict = true\n",
    );

    treeboot()
        .args(["sync", "shared"])
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("invalid sync file operation"))
        .stderr(predicate::str::contains("cannot be used with sync"));

    assert!(!repo.worktree_path().join("shared").exists());
}

#[test]
fn manual_env_false_should_override_config_strict() {
    let repo = git_worktree();
    std::fs::create_dir_all(repo.root_path().join("shared"))
        .expect("source directory should be created");
    write_file(&repo.root_path().join("shared/config"), "value\n");
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        "strict = true\n",
    );

    treeboot()
        .args(["sync", "shared"])
        .env("TREEBOOT_STRICT", "false")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::contains("invalid sync file operation").not());

    assert_eq!(
        std::fs::read_to_string(repo.worktree_path().join("shared/config"))
            .expect("synced file should be readable"),
        "value\n"
    );
}

#[test]
fn manual_cli_strict_should_override_env_and_config_false() {
    let repo = git_worktree();
    std::fs::create_dir_all(repo.root_path().join("shared"))
        .expect("source directory should be created");
    write_file(&repo.root_path().join("shared/config"), "value\n");
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        "strict = false\n",
    );

    treeboot()
        .args(["sync", "--strict", "shared"])
        .env("TREEBOOT_STRICT", "false")
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("invalid sync file operation"))
        .stderr(predicate::str::contains("cannot be used with sync"));

    assert!(!repo.worktree_path().join("shared").exists());
}

#[cfg(unix)]
#[test]
fn manual_commands_should_ignore_executable_init_script() {
    let repo = git_worktree();
    let marker = repo.worktree_path().join("script.out");
    write_file(&repo.root_path().join(".env"), "TOKEN=1\n");
    write_executable_script(
        &repo.worktree_path().join(".treeboot.sh"),
        &format!("#!/bin/sh\nprintf 'ran\\n' > {}\n", marker.display()),
    );

    treeboot()
        .args(["copy", ".env"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("treeboot: run").not());

    assert!(!marker.exists());
    assert!(repo.worktree_path().join(".env").is_file());
}

#[test]
fn dynamic_completion_should_list_root_relative_sources() {
    let repo = git_worktree();
    write_file(&repo.root_path().join(".env"), "TOKEN=1\n");
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        "invalid = [\n",
    );
    std::fs::create_dir_all(repo.root_path().join("shared"))
        .expect("source directory should be created");

    for shell in COMPLETION_SHELLS {
        complete_source_candidates(shell, ["treeboot", "copy", ""], repo.worktree_path())
            .success()
            .stdout(predicate::str::contains(".env"))
            .stdout(predicate::str::contains("shared"));
    }
}

#[test]
fn dynamic_completion_should_use_root_option_for_sources() {
    let repo = git_worktree();
    let root = tempfile::TempDir::new().expect("override root should be created");
    write_file(&root.path().join("override.env"), "TOKEN=1\n");
    write_file(&repo.root_path().join("default.env"), "TOKEN=1\n");

    for shell in COMPLETION_SHELLS {
        complete_source_candidates(
            shell,
            [
                OsStr::new("treeboot"),
                OsStr::new("copy"),
                OsStr::new("--root"),
                root.path().as_os_str(),
                OsStr::new(""),
            ],
            repo.worktree_path(),
        )
        .success()
        .stdout(predicate::str::contains("override.env"))
        .stdout(predicate::str::contains("default.env").not());
    }
}

#[test]
fn dynamic_completion_should_use_root_equals_option_for_sources() {
    let repo = git_worktree();
    let root = tempfile::TempDir::new().expect("override root should be created");
    write_file(&root.path().join("override.env"), "TOKEN=1\n");
    write_file(&repo.root_path().join("default.env"), "TOKEN=1\n");

    for shell in COMPLETION_SHELLS {
        complete_source_candidates(
            shell,
            [
                OsString::from("treeboot"),
                OsString::from("copy"),
                OsString::from(format!("--root={}", root.path().display())),
                OsString::new(),
            ],
            repo.worktree_path(),
        )
        .success()
        .stdout(predicate::str::contains("override.env"))
        .stdout(predicate::str::contains("default.env").not());
    }
}
