use predicates::prelude::*;
#[cfg(unix)]
use tempfile::TempDir;

#[cfg(unix)]
use super::support::{git_worktree, runner_capabilities, skip, symlink_file, treeboot, write_file};
use crate::case::{CaseDefinition, CaseMetadata};

const SYMLINK_SPEC: &[&str] = &["#symlinks-inside-copy-and-sync"];
const MANUAL_DIAGNOSTIC_SPEC: &[&str] = &["#operator-experience-output-and-exit-codes"];
const TEARDOWN_SPEC: &[&str] = &["#treeboot-teardown"];
const COMPLETION_SPEC: &[&str] = &["#manual-operation-source-completion"];

pub(crate) const DEFINITIONS: &[CaseDefinition] = &[
    #[cfg(unix)]
    CaseDefinition::new(
        CaseMetadata::closure(
            "closure.copy.reject-top-level-dangling-source-symlink",
            SYMLINK_SPEC,
        ),
        reject_top_level_dangling_source_symlink,
    ),
    #[cfg(not(unix))]
    CaseDefinition::skipped(
        CaseMetadata::closure(
            "closure.copy.reject-top-level-dangling-source-symlink",
            SYMLINK_SPEC,
        ),
        "requires a Unix host with unprivileged symbolic-link creation",
    ),
    #[cfg(unix)]
    CaseDefinition::new(
        CaseMetadata::closure(
            "closure.copy.reject-nested-dangling-source-symlink",
            SYMLINK_SPEC,
        ),
        reject_nested_dangling_source_symlink,
    ),
    #[cfg(not(unix))]
    CaseDefinition::skipped(
        CaseMetadata::closure(
            "closure.copy.reject-nested-dangling-source-symlink",
            SYMLINK_SPEC,
        ),
        "requires a Unix host with unprivileged symbolic-link creation",
    ),
    #[cfg(unix)]
    CaseDefinition::new(
        CaseMetadata::closure(
            "closure.copy.ignore-bypasses-unsafe-source-symlink",
            SYMLINK_SPEC,
        ),
        ignore_bypasses_unsafe_source_symlink,
    ),
    #[cfg(not(unix))]
    CaseDefinition::skipped(
        CaseMetadata::closure(
            "closure.copy.ignore-bypasses-unsafe-source-symlink",
            SYMLINK_SPEC,
        ),
        "requires a Unix host with unprivileged symbolic-link creation",
    ),
    #[cfg(unix)]
    CaseDefinition::new(
        CaseMetadata::closure(
            "closure.copy.include-bypasses-unsafe-source-symlink",
            SYMLINK_SPEC,
        ),
        include_bypasses_unsafe_source_symlink,
    ),
    #[cfg(not(unix))]
    CaseDefinition::skipped(
        CaseMetadata::closure(
            "closure.copy.include-bypasses-unsafe-source-symlink",
            SYMLINK_SPEC,
        ),
        "requires a Unix host with unprivileged symbolic-link creation",
    ),
    CaseDefinition::new(
        CaseMetadata::closure(
            "closure.manual.command-wide-input-diagnostic",
            MANUAL_DIAGNOSTIC_SPEC,
        ),
        command_wide_input_diagnostic,
    ),
    CaseDefinition::new(
        CaseMetadata::closure(
            "closure.manual.normalized-mapping-diagnostic",
            MANUAL_DIAGNOSTIC_SPEC,
        ),
        normalized_mapping_diagnostic,
    ),
    CaseDefinition::new(
        CaseMetadata::closure(
            "closure.manual.post-normalization-path-conflict-diagnostic",
            MANUAL_DIAGNOSTIC_SPEC,
        ),
        post_normalization_path_conflict_diagnostic,
    ),
    #[cfg(unix)]
    CaseDefinition::new(
        CaseMetadata::closure(
            "closure.manual.recursive-source-diagnostic",
            MANUAL_DIAGNOSTIC_SPEC,
        ),
        recursive_source_diagnostic,
    ),
    #[cfg(not(unix))]
    CaseDefinition::skipped(
        CaseMetadata::closure(
            "closure.manual.recursive-source-diagnostic",
            MANUAL_DIAGNOSTIC_SPEC,
        ),
        "requires a Unix host with unprivileged symbolic-link creation",
    ),
    CaseDefinition::new(
        CaseMetadata::closure(
            "closure.teardown.confirm-through-terminal-input",
            TEARDOWN_SPEC,
        ),
        confirm_teardown_through_terminal_input,
    ),
    #[cfg(unix)]
    CaseDefinition::new(
        CaseMetadata::closure(
            "closure.completions.installed-bash-script-lists-root-sources",
            COMPLETION_SPEC,
        ),
        installed_bash_script_lists_root_sources,
    ),
    #[cfg(not(unix))]
    CaseDefinition::skipped(
        CaseMetadata::closure(
            "closure.completions.installed-bash-script-lists-root-sources",
            COMPLETION_SPEC,
        ),
        "requires Bash and the Bash completion runtime",
    ),
];

#[cfg(unix)]
fn reject_top_level_dangling_source_symlink() {
    let repo = git_worktree();
    symlink_file("missing", repo.root_path().join("shared"));
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        "copy = [\"shared\"]\n",
    );

    treeboot()
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("shared"));
    assert!(!repo.worktree_path().join("shared").exists());
}

#[cfg(unix)]
fn reject_nested_dangling_source_symlink() {
    let repo = git_worktree();
    std::fs::create_dir_all(repo.root_path().join("shared/nested"))
        .expect("source directory should be created");
    write_file(&repo.root_path().join("shared/safe.txt"), "safe\n");
    symlink_file("missing", repo.root_path().join("shared/nested/dangling"));
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        "copy = [\"shared\"]\n",
    );

    treeboot()
        .current_dir(repo.worktree_path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("shared/nested/dangling"));
    assert!(!repo.worktree_path().join("shared").exists());
}

#[cfg(unix)]
fn ignore_bypasses_unsafe_source_symlink() {
    let repo = git_worktree();
    let outside = TempDir::new().expect("outside directory should be created");
    std::fs::create_dir_all(repo.root_path().join("shared"))
        .expect("source directory should be created");
    write_file(&repo.root_path().join("shared/safe.txt"), "safe\n");
    write_file(&outside.path().join("secret"), "secret\n");
    symlink_file(
        outside.path().join("secret"),
        repo.root_path().join("shared/unsafe"),
    );
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        "copy = [{ source = \"shared\", ignore = [\"unsafe\"] }]\n",
    );

    treeboot()
        .current_dir(repo.worktree_path())
        .assert()
        .success();
    assert_eq!(
        std::fs::read_to_string(repo.worktree_path().join("shared/safe.txt")).unwrap(),
        "safe\n"
    );
    assert!(!repo.worktree_path().join("shared/unsafe").exists());
}

#[cfg(unix)]
fn include_bypasses_unsafe_source_symlink() {
    let repo = git_worktree();
    let outside = TempDir::new().expect("outside directory should be created");
    std::fs::create_dir_all(repo.root_path().join("shared"))
        .expect("source directory should be created");
    write_file(&repo.root_path().join("shared/safe.txt"), "safe\n");
    write_file(&outside.path().join("secret"), "secret\n");
    symlink_file(
        outside.path().join("secret"),
        repo.root_path().join("shared/unsafe"),
    );
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        "copy = [{ source = \"shared\", include = [\"safe.txt\"] }]\n",
    );

    treeboot()
        .current_dir(repo.worktree_path())
        .assert()
        .success();
    assert_eq!(
        std::fs::read_to_string(repo.worktree_path().join("shared/safe.txt")).unwrap(),
        "safe\n"
    );
    assert!(!repo.worktree_path().join("shared/unsafe").exists());
}

fn command_wide_input_diagnostic() {
    let repo = git_worktree();
    let output = treeboot()
        .args(["copy", "shared", "--include", "!docs"])
        .current_dir(repo.worktree_path())
        .output()
        .expect("candidate should run");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr.contains("copy"), "{stderr}");
    assert!(stderr.contains("!docs"), "{stderr}");
    assert!(!stderr.contains("invalid config"), "{stderr}");
    assert!(!stderr.contains("line 1"), "{stderr}");
}

fn normalized_mapping_diagnostic() {
    let repo = git_worktree();
    let output = treeboot()
        .args(["copy", "missing", "--target", "mapped", "--required"])
        .current_dir(repo.worktree_path())
        .output()
        .expect("candidate should run");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr.contains("copy"), "{stderr}");
    assert!(stderr.contains("missing"), "{stderr}");
    assert!(stderr.contains("mapped"), "{stderr}");
    assert!(!stderr.contains("invalid config"), "{stderr}");
}

fn post_normalization_path_conflict_diagnostic() {
    let repo = git_worktree();
    write_file(&repo.root_path().join("source"), "value\n");
    write_file(&repo.worktree_path().join("blocked"), "file\n");
    let output = treeboot()
        .args(["copy", "source", "--target", "blocked/target"])
        .current_dir(repo.worktree_path())
        .output()
        .expect("candidate should run");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr.contains("copy"), "{stderr}");
    assert!(stderr.contains("blocked"), "{stderr}");
    assert!(stderr.contains("file"), "{stderr}");
    assert!(!repo.worktree_path().join("blocked/target").exists());
}

#[cfg(unix)]
fn recursive_source_diagnostic() {
    let repo = git_worktree();
    std::fs::create_dir_all(repo.root_path().join("shared/nested"))
        .expect("source directory should be created");
    symlink_file("missing", repo.root_path().join("shared/nested/dangling"));
    let output = treeboot()
        .args(["copy", "shared"])
        .current_dir(repo.worktree_path())
        .output()
        .expect("candidate should run");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr.contains("copy"), "{stderr}");
    assert!(stderr.contains("shared/nested/dangling"), "{stderr}");
    assert!(!stderr.contains("invalid config"), "{stderr}");
}

fn confirm_teardown_through_terminal_input() {
    let repo = git_worktree();
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        "teardown_commands = [\"echo terminal-confirmed\"]\n",
    );

    treeboot()
        .arg("teardown")
        .write_terminal("y\n")
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stdout(predicate::str::contains("terminal-confirmed"));
}

#[cfg(unix)]
fn installed_bash_script_lists_root_sources() {
    if !runner_capabilities().completion_script_execution {
        skip("runner cannot execute generated completion scripts on the fixture host");
    }

    let repo = git_worktree();
    std::fs::create_dir_all(repo.root_path().join("shared-source"))
        .expect("root source directory should be created");
    let output = treeboot()
        .args(["completions", "bash"])
        .current_dir(repo.worktree_path())
        .output()
        .expect("candidate should generate Bash completion script");
    assert!(output.status.success());

    let script = String::from_utf8(output.stdout).expect("Bash completion script should be UTF-8");
    let temp = TempDir::new().expect("completion script directory should be created");
    let script_path = temp.path().join("completion-test.bash");
    write_file(
        &script_path,
        &format!(
            "{script}\nCOMP_WORDS=(treeboot copy sh)\nCOMP_CWORD=2\nCOMP_TYPE=9\n_clap_complete_treeboot '' 'sh'\nprintf '%s\\n' \"${{COMPREPLY[@]}}\"\n"
        ),
    );
    let completion = std::process::Command::new("bash")
        .arg(&script_path)
        .current_dir(repo.worktree_path())
        .output()
        .expect("installed Bash completion script should run");

    assert!(
        completion.status.success(),
        "{}",
        String::from_utf8_lossy(&completion.stderr)
    );
    assert!(
        String::from_utf8_lossy(&completion.stdout).contains("shared-source"),
        "{}",
        String::from_utf8_lossy(&completion.stdout)
    );
}
