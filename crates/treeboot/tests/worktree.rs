use std::path::{Path, PathBuf};

use predicates::prelude::*;
use tempfile::TempDir;

mod common;

use common::{
    assert_json_object_keys, canonical_path, git, git_worktree, parse_json, treeboot, write_file,
};

struct ExtraWorktree {
    _parent: TempDir,
    path: PathBuf,
}

fn add_worktree(root: &Path, name: &str) -> ExtraWorktree {
    let parent = TempDir::new().expect("worktree parent should be created");
    let path = parent.path().join(name);
    let path_text = path.to_str().expect("test path should be UTF-8");
    git(
        &[
            "worktree",
            "add",
            "-b",
            &format!("treeboot-{name}"),
            path_text,
        ],
        root,
    );
    ExtraWorktree {
        _parent: parent,
        path,
    }
}

fn worktree_id(cwd: &Path) -> String {
    let output = treeboot()
        .args(["worktree", "id"])
        .current_dir(cwd)
        .output()
        .expect("treeboot should run");
    assert!(output.status.success(), "ID command should succeed");
    String::from_utf8(output.stdout)
        .expect("ID should be UTF-8")
        .trim()
        .to_owned()
}

#[test]
fn worktree_id_should_print_bare_text_and_exact_structured_shapes() {
    let repo = git_worktree();
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        r#"worktree_id = { max_length = 20, hash_length = 8, separator = "_" }"#,
    );

    let text = treeboot()
        .args(["worktree", "id"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let id = String::from_utf8(text).expect("text ID should be UTF-8");
    assert!(id.ends_with('\n'));
    assert!(!id.trim().contains(char::is_whitespace));
    assert_eq!(
        id.trim().rsplit_once('_').map(|(_, hash)| hash.len()),
        Some(8)
    );

    let json = treeboot()
        .args(["worktree", "id", "--json"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let json = parse_json(json, "worktree id");
    assert_json_object_keys(&json, &["id"]);
    assert_eq!(json["id"], id.trim());

    let env_json = treeboot()
        .args(["env", "--json"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let env_json = parse_json(env_json, "configured env");
    assert_eq!(json["id"], env_json["TREEBOOT_WORKTREE_ID"]);

    treeboot()
        .args(["worktree", "id", "--yaml"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains(format!("id: {}", id.trim())));
}

#[test]
fn worktree_path_and_list_should_use_candidate_local_config_and_contract_order() {
    let repo = git_worktree();
    let second = add_worktree(repo.root_path(), "zeta");
    write_file(
        &repo.root_path().join(".treeboot.toml"),
        r#"worktree_id = { hash_length = 5, separator = "_" }"#,
    );
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        r#"worktree_id = { hash_length = 7, separator = "_" }"#,
    );
    write_file(
        &second.path.join(".treeboot.toml"),
        r#"worktree_id = { hash_length = 9, separator = "_" }"#,
    );

    let root_id = worktree_id(repo.root_path());
    let linked_id = worktree_id(repo.worktree_path());
    let second_id = worktree_id(&second.path);
    assert_eq!(
        root_id.rsplit_once('_').map(|(_, hash)| hash.len()),
        Some(5)
    );
    assert_eq!(
        linked_id.rsplit_once('_').map(|(_, hash)| hash.len()),
        Some(7)
    );
    assert_eq!(
        second_id.rsplit_once('_').map(|(_, hash)| hash.len()),
        Some(9)
    );

    for (id, path) in [
        (&root_id, repo.root_path()),
        (&linked_id, repo.worktree_path()),
        (&second_id, second.path.as_path()),
    ] {
        treeboot()
            .args(["worktree", "path", id])
            .current_dir(repo.worktree_path())
            .assert()
            .success()
            .stderr(predicate::str::is_empty())
            .stdout(format!("{}\n", canonical_path(path).display()));
    }

    let json = treeboot()
        .args(["worktree", "list", "--json"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let json = parse_json(json, "worktree list");
    assert_json_object_keys(&json, &["worktrees"]);
    let entries = json["worktrees"]
        .as_array()
        .expect("worktrees should be an array");
    assert_eq!(entries.len(), 3);
    for entry in entries {
        assert_json_object_keys(entry, &["id", "path"]);
    }
    assert_eq!(
        entries[0]["path"],
        canonical_path(repo.root_path()).display().to_string()
    );
    let remaining = entries[1..]
        .iter()
        .map(|entry| entry["path"].as_str().expect("path should be a string"))
        .collect::<Vec<_>>();
    assert!(remaining.windows(2).all(|paths| paths[0] <= paths[1]));

    treeboot()
        .args(["worktree", "list", "--format", "text"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::starts_with("ID"))
        .stdout(predicate::str::contains("PATH"))
        .stdout(predicate::str::contains(root_id))
        .stdout(predicate::str::contains(linked_id))
        .stdout(predicate::str::contains(second_id));

    treeboot()
        .args(["worktree", "list", "--yaml"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::starts_with("worktrees:"))
        .stdout(predicate::str::contains("id:"))
        .stdout(predicate::str::contains("path:"));
}

#[test]
fn worktree_path_should_emit_exact_structured_shapes() {
    let repo = git_worktree();
    let id = worktree_id(repo.worktree_path());
    let path = canonical_path(repo.worktree_path());

    let json = treeboot()
        .args(["worktree", "path", &id, "--json"])
        .current_dir(repo.root_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let json = parse_json(json, "worktree path");
    assert_json_object_keys(&json, &["id", "path"]);
    assert_eq!(json["id"], id);
    assert_eq!(json["path"], path.display().to_string());

    treeboot()
        .args(["worktree", "path", &id, "--format", "yaml"])
        .current_dir(repo.root_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains(format!("id: {id}")))
        .stdout(predicate::str::contains(format!(
            "path: {}",
            path.display()
        )));
}

#[test]
fn worktree_inspection_should_use_defaults_without_config() {
    let repo = git_worktree();
    let id = worktree_id(repo.worktree_path());

    assert!(id.starts_with("linked-"));
    assert_eq!(id.rsplit_once('-').map(|(_, hash)| hash.len()), Some(6));
}

#[test]
fn worktree_list_should_skip_stale_registered_paths() {
    let repo = git_worktree();
    let stale_parent = TempDir::new().expect("stale parent should be created");
    let stale_path = stale_parent.path().join("stale");
    let stale_text = stale_path.to_str().expect("stale path should be UTF-8");
    git(
        &["worktree", "add", "-b", "treeboot-stale", stale_text],
        repo.root_path(),
    );
    std::fs::remove_dir_all(&stale_path).expect("stale worktree should be removed from disk");

    treeboot()
        .args(["worktree", "list"])
        .current_dir(repo.root_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("stale").not());
}

#[test]
fn worktree_path_should_report_missing_and_ambiguous_ids_without_stdout() {
    let repo = git_worktree();

    treeboot()
        .args(["worktree", "path", "missing-id"])
        .current_dir(repo.root_path())
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("no worktree found"));

    let second = add_worktree(repo.root_path(), "collision");
    for path in [repo.worktree_path(), second.path.as_path()] {
        write_file(
            &path.join(".treeboot.toml"),
            "worktree_id = { max_length = 3, hash_length = 1 }\n",
        );
    }

    // A one-character hash has only 32 values. Add linked worktrees until two
    // complete identifiers collide, then assert lookup reports every path.
    let mut candidates = vec![repo.worktree_path().to_path_buf(), second.path];
    let mut parents = Vec::new();
    let (collision_id, collision_paths) = loop {
        let mut by_id = std::collections::BTreeMap::<String, Vec<PathBuf>>::new();
        for path in &candidates {
            by_id
                .entry(worktree_id(path))
                .or_default()
                .push(path.clone());
        }
        if let Some((id, paths)) = by_id.into_iter().find(|(_, paths)| paths.len() > 1) {
            break (id, paths);
        }
        let name = format!("collision-{}", candidates.len());
        let extra = add_worktree(repo.root_path(), &name);
        write_file(
            &extra.path.join(".treeboot.toml"),
            "worktree_id = { max_length = 3, hash_length = 1 }\n",
        );
        candidates.push(extra.path.clone());
        parents.push(extra);
        assert!(candidates.len() <= 34, "pigeonhole collision should exist");
    };

    let output = treeboot()
        .args(["worktree", "path", &collision_id])
        .current_dir(repo.root_path())
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("ambiguous worktree ID"))
        .get_output()
        .stderr
        .clone();
    let stderr = String::from_utf8(output).expect("stderr should be UTF-8");
    for path in &collision_paths {
        assert!(
            stderr.contains(&canonical_path(path).display().to_string()),
            "stderr should report colliding path"
        );
    }

    let list = treeboot()
        .args(["worktree", "list", "--json"])
        .current_dir(repo.root_path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let list = parse_json(list, "colliding worktree list");
    let duplicate_count = list["worktrees"]
        .as_array()
        .expect("worktrees should be an array")
        .iter()
        .filter(|entry| entry["id"] == collision_id)
        .count();
    assert_eq!(duplicate_count, collision_paths.len());
}

#[test]
fn malformed_sibling_config_should_fail_atomically_with_candidate_path() {
    let repo = git_worktree();
    let sibling = add_worktree(repo.root_path(), "malformed");
    write_file(
        &sibling.path.join(".treeboot.toml"),
        "worktree_id = { hash_length = 0 }\n",
    );

    for args in [
        vec!["worktree", "list"],
        vec!["worktree", "path", "missing"],
    ] {
        treeboot()
            .args(args)
            .current_dir(repo.root_path())
            .assert()
            .failure()
            .stdout(predicate::str::is_empty())
            .stderr(predicate::str::contains(
                canonical_path(&sibling.path).display().to_string(),
            ))
            .stderr(predicate::str::contains("hash_length"));
    }
}

#[test]
fn worktree_commands_should_fail_outside_git() {
    let dir = TempDir::new().expect("tempdir should be created");

    for args in [
        vec!["worktree", "id"],
        vec!["worktree", "list"],
        vec!["worktree", "path", "id"],
    ] {
        treeboot()
            .args(args)
            .current_dir(dir.path())
            .assert()
            .failure()
            .stdout(predicate::str::is_empty())
            .stderr(predicate::str::contains("not inside a Git worktree"));
    }
}

#[test]
fn worktree_commands_should_honor_recognized_ambient_environment() {
    let repo = git_worktree();
    let missing_root = repo.root_path().join("missing-root");

    for args in [
        vec!["worktree", "id"],
        vec!["worktree", "list"],
        vec!["worktree", "path", "id"],
    ] {
        treeboot()
            .args(args)
            .env("TREEBOOT_ROOT_PATH", &missing_root)
            .current_dir(repo.worktree_path())
            .assert()
            .failure()
            .stdout(predicate::str::is_empty())
            .stderr(predicate::str::contains("missing-root"));
    }
}

#[test]
fn worktree_nested_help_and_version_should_be_exposed() {
    treeboot()
        .args(["worktree", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("id"))
        .stdout(predicate::str::contains("path"))
        .stdout(predicate::str::contains("list"));

    treeboot()
        .args(["worktree", "id", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--format"))
        .stdout(predicate::str::contains("--json"))
        .stdout(predicate::str::contains("--yaml"));

    treeboot()
        .args(["worktree", "--version"])
        .assert()
        .success()
        .stdout(predicate::str::contains(treeboot_core::TREEBOOT_VERSION));
}
