use std::path::{Path, PathBuf};

use predicates::prelude::*;
use tempfile::TempDir;

use crate::cases::support::{
    assert_json_object_keys, canonical_path, git, git_worktree, parse_json, treeboot, write_file,
};
#[cfg(unix)]
use crate::cases::support::{symlink_dir, symlink_file};

pub(crate) struct ExtraWorktree {
    _parent: TempDir,
    path: PathBuf,
}

pub(crate) fn add_worktree(root: &Path, name: &str) -> ExtraWorktree {
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

pub(crate) fn worktree_id(cwd: &Path) -> String {
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

pub(crate) struct BarePrimaryWorktree {
    _temp: TempDir,
    bare_path: PathBuf,
    linked_path: PathBuf,
}

pub(crate) fn bare_primary_worktree() -> BarePrimaryWorktree {
    let temp = TempDir::new().expect("tempdir should be created");
    let bare_path = temp.path().join("bare.git");
    let seed_path = temp.path().join("seed");
    let linked_path = temp.path().join("linked");
    let bare = bare_path.to_str().expect("bare path should be UTF-8");
    let linked = linked_path.to_str().expect("linked path should be UTF-8");

    git(&["init", "--bare", bare], temp.path());
    git(&["init", "seed"], temp.path());
    git(&["config", "user.name", "treeboot"], &seed_path);
    git(
        &["config", "user.email", "treeboot@example.invalid"],
        &seed_path,
    );
    git(&["config", "commit.gpgsign", "false"], &seed_path);
    write_file(&seed_path.join("README.md"), "treeboot bare repo\n");
    git(&["add", "README.md"], &seed_path);
    git(&["commit", "-m", "Initial commit"], &seed_path);
    git(&["remote", "add", "origin", bare], &seed_path);
    git(&["push", "origin", "HEAD:main"], &seed_path);
    git(&["symbolic-ref", "HEAD", "refs/heads/main"], &bare_path);
    git(
        &["worktree", "add", "-b", "linked-worktree", linked, "main"],
        &bare_path,
    );

    BarePrimaryWorktree {
        _temp: temp,
        bare_path,
        linked_path,
    }
}

pub(crate) fn worktree_id_and_slug_should_print_bare_text_and_exact_structured_shapes() {
    let repo = git_worktree();
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        "worktree_id = { length = 8 }\n\
         worktree_slug = { max_length = 20, separator = \"_\" }\n",
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
    assert_eq!(id.trim().len(), 8);

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

    let yaml = treeboot()
        .args(["worktree", "id", "--yaml"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let yaml: serde_json::Value = yaml_serde::from_slice(&yaml)
        .unwrap_or_else(|error| panic!("worktree ID YAML should parse: {error}"));
    assert_json_object_keys(&yaml, &["id"]);
    assert_eq!(yaml["id"], id.trim());

    let slug = treeboot()
        .args(["worktree", "slug"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let slug = String::from_utf8(slug).expect("text slug should be UTF-8");
    assert!(slug.trim().starts_with("linked_"));
    assert!(slug.trim().ends_with(id.trim()));
    assert!(slug.trim().len() <= 20);

    let slug_json = treeboot()
        .args(["worktree", "slug", "--json"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let slug_json = parse_json(slug_json, "worktree slug");
    assert_json_object_keys(&slug_json, &["slug"]);
    assert_eq!(slug_json["slug"], slug.trim());
    assert_eq!(slug_json["slug"], env_json["TREEBOOT_WORKTREE_SLUG"]);

    treeboot()
        .args(["worktree", "slug", "--yaml"])
        .current_dir(repo.worktree_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains(format!("slug: {}", slug.trim())));
}

pub(crate) fn worktree_path_and_list_should_use_candidate_local_config_and_contract_order() {
    let repo = git_worktree();
    let second = add_worktree(repo.root_path(), "zeta");
    write_file(
        &repo.root_path().join(".treeboot.toml"),
        "worktree_id = { length = 5 }\nworktree_slug = { separator = \"_\" }\n",
    );
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        "worktree_id = { length = 7 }\nworktree_slug = { separator = \"_\" }\n",
    );
    write_file(
        &second.path.join(".treeboot.toml"),
        "worktree_id = { length = 9 }\nworktree_slug = { separator = \"_\" }\n",
    );

    let root_id = worktree_id(repo.root_path());
    let linked_id = worktree_id(repo.worktree_path());
    let second_id = worktree_id(&second.path);
    assert_eq!(root_id.len(), 5);
    assert_eq!(linked_id.len(), 7);
    assert_eq!(second_id.len(), 9);

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
        assert_json_object_keys(entry, &["id", "path", "slug"]);
        assert!(
            entry["slug"]
                .as_str()
                .expect("slug should be a string")
                .ends_with(entry["id"].as_str().expect("ID should be a string"))
        );
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
        .stdout(predicate::str::contains("SLUG"))
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
        .stdout(predicate::str::contains("slug:"))
        .stdout(predicate::str::contains("path:"));
}

pub(crate) fn worktree_list_should_keep_main_first_when_a_linked_path_sorts_before_it() {
    let temp = TempDir::new().expect("tempdir should be created");
    let root = temp.path().join("z-root");
    let linked = temp.path().join("a-linked");
    std::fs::create_dir(&root).expect("root should be created");
    git(&["init"], &root);
    git(&["config", "user.name", "treeboot"], &root);
    git(&["config", "user.email", "treeboot@example.invalid"], &root);
    git(&["config", "commit.gpgsign", "false"], &root);
    write_file(&root.join("README.md"), "treeboot ordering repo\n");
    git(&["add", "README.md"], &root);
    git(&["commit", "-m", "Initial commit"], &root);
    git(
        &[
            "worktree",
            "add",
            "-b",
            "treeboot-ordering",
            linked.to_str().expect("linked path should be UTF-8"),
        ],
        &root,
    );
    let root = canonical_path(&root);
    let linked = canonical_path(&linked);
    assert!(linked < root, "fixture should sort linked path before root");

    let list = treeboot()
        .args(["worktree", "list", "--format", "json"])
        .current_dir(&linked)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let list = parse_json(list, "main-first list");
    let entries = list["worktrees"]
        .as_array()
        .expect("worktrees should be an array");

    assert_eq!(entries[0]["path"], root.display().to_string());
    assert_eq!(entries[1]["path"], linked.display().to_string());
}

pub(crate) fn worktree_commands_should_support_a_bare_primary_repository() {
    let repo = bare_primary_worktree();
    let linked = canonical_path(&repo.linked_path);
    let bare = canonical_path(&repo.bare_path);
    let id = worktree_id(&repo.linked_path);

    let env = treeboot()
        .args(["env", "--json"])
        .current_dir(&repo.linked_path)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let env = parse_json(env, "bare primary env");
    assert_eq!(env["TREEBOOT_ROOT_PATH"], bare.display().to_string());

    let list = treeboot()
        .args(["worktree", "list", "--json"])
        .current_dir(&repo.linked_path)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let list = parse_json(list, "bare primary list");
    let entries = list["worktrees"]
        .as_array()
        .expect("worktrees should be an array");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["id"], id);
    assert_eq!(entries[0]["path"], linked.display().to_string());
    assert_ne!(entries[0]["path"], bare.display().to_string());

    treeboot()
        .args(["worktree", "path", &id])
        .current_dir(&repo.linked_path)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(format!("{}\n", linked.display()));
}

pub(crate) fn worktree_path_should_emit_exact_structured_shapes() {
    let repo = git_worktree();
    let id = worktree_id(repo.worktree_path());
    let path = canonical_path(repo.worktree_path());
    let nested = repo.worktree_path().join("nested");
    std::fs::create_dir(&nested).expect("nested directory should be created");

    let json = treeboot()
        .args(["worktree", "path", &id, "--format", "json"])
        .current_dir(&nested)
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

    let yaml = treeboot()
        .args(["worktree", "path", &id, "--format", "yaml"])
        .current_dir(repo.root_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let yaml: serde_json::Value = yaml_serde::from_slice(&yaml)
        .unwrap_or_else(|error| panic!("worktree path YAML should parse: {error}"));
    assert_json_object_keys(&yaml, &["id", "path"]);
    assert_eq!(yaml["id"], id);
    assert_eq!(yaml["path"], path.display().to_string());
}

pub(crate) fn worktree_inspection_should_use_defaults_without_config() {
    let repo = git_worktree();
    let id = worktree_id(repo.worktree_path());

    assert_eq!(id.len(), 6);
    assert!(id.bytes().all(|byte| {
        byte.is_ascii_digit()
            || matches!(
                byte,
                b'a'..=b'h' | b'j'..=b'k' | b'm'..=b'n' | b'p'..=b't' | b'v'..=b'z'
            )
    }));
}

pub(crate) fn explicit_actual_worktree_identity_should_match_implicit_and_local_config() {
    let repo = git_worktree();
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        "worktree_id = { length = 10 }\n\
         worktree_slug = { max_length = 24, separator = \"_\" }\n",
    );

    for command in ["id", "slug"] {
        let implicit = treeboot()
            .args(["worktree", command])
            .current_dir(repo.worktree_path())
            .output()
            .expect("implicit identity command should run");
        assert!(
            implicit.status.success(),
            "implicit {command} should succeed"
        );

        let explicit = treeboot()
            .args(["worktree", command])
            .arg(repo.worktree_path())
            .current_dir(repo.root_path())
            .output()
            .expect("explicit identity command should run");
        assert!(
            explicit.status.success(),
            "explicit {command} should succeed"
        );
        assert_eq!(explicit.stdout, implicit.stdout);
    }
}

pub(crate) fn explicit_ordinary_directory_identity_should_work_outside_git_and_use_local_config() {
    let parent = TempDir::new().expect("tempdir should be created");
    let target = parent.path().join("Feature Login");
    std::fs::create_dir(&target).expect("target should be created");
    write_file(
        &target.join(".treeboot.toml"),
        "worktree_id = { length = 9 }\n\
         worktree_slug = { max_length = 22, separator = \"_\" }\n",
    );

    let id = treeboot()
        .args(["worktree", "id"])
        .arg(&target)
        .current_dir(parent.path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let id = String::from_utf8(id).expect("ID should be UTF-8");
    assert_eq!(id.trim().len(), 9);

    let slug = treeboot()
        .args(["worktree", "slug"])
        .arg(&target)
        .current_dir(parent.path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let slug = String::from_utf8(slug).expect("slug should be UTF-8");
    assert!(slug.trim().starts_with("feature_logi_"));
    assert!(slug.trim().ends_with(id.trim()));
    assert_eq!(slug.trim().len(), 22);
}

pub(crate) fn explicit_identity_relative_absolute_dot_and_dotdot_should_normalize_to_one_target() {
    let parent = TempDir::new().expect("tempdir should be created");
    let real_parent = parent.path().join("real");
    let target = real_parent.join("target");
    std::fs::create_dir_all(&target).expect("target should be created");

    for command in ["id", "slug"] {
        let absolute = worktree_identity_for_path(command, parent.path(), &target);
        let relative = worktree_identity_for_path(command, parent.path(), Path::new("real/target"));
        let dotted =
            worktree_identity_for_path(command, parent.path(), Path::new("./real/./target"));
        let parented =
            worktree_identity_for_path(command, parent.path(), Path::new("real/other/../target"));

        assert_eq!(absolute, relative);
        assert_eq!(absolute, dotted);
        assert_eq!(absolute, parented);
    }
}

#[cfg(unix)]
pub(crate) fn explicit_identity_symlink_alias_should_normalize_to_target() {
    let parent = TempDir::new().expect("tempdir should be created");
    let real_parent = parent.path().join("real");
    let target = real_parent.join("target");
    let alias = parent.path().join("alias");
    std::fs::create_dir_all(&target).expect("target should be created");
    symlink_dir(&real_parent, &alias);

    for command in ["id", "slug"] {
        let expected = worktree_identity_for_path(command, parent.path(), &target);
        let aliased = worktree_identity_for_path(command, parent.path(), Path::new("alias/target"));

        assert_eq!(expected, aliased);
    }
}

pub(crate) fn explicit_nonexistent_identity_should_survive_ordinary_directory_creation() {
    let parent = TempDir::new().expect("tempdir should be created");
    let target = parent.path().join("future").join("worktree");

    let before_id = worktree_identity_for_path("id", parent.path(), &target);
    let before_slug = worktree_identity_for_path("slug", parent.path(), &target);
    std::fs::create_dir_all(&target).expect("target should be created");
    let after_id = worktree_identity_for_path("id", parent.path(), &target);
    let after_slug = worktree_identity_for_path("slug", parent.path(), &target);

    assert_eq!(before_id, after_id);
    assert_eq!(before_slug, after_slug);
}

pub(crate) fn explicit_nonexistent_git_targets_should_keep_fallback_slug_after_creation() {
    let repo = git_worktree();
    let targets = [
        repo.worktree_path().join("future").join("deadbeef"),
        repo.worktree_path().join("future").join("💥"),
    ];

    for target in targets {
        let before_id = worktree_identity_for_path("id", repo.worktree_path(), &target);
        let before_slug = worktree_identity_for_path("slug", repo.worktree_path(), &target);
        std::fs::create_dir_all(&target).expect("target should be created");
        let after_id = worktree_identity_for_path("id", repo.worktree_path(), &target);
        let after_slug = worktree_identity_for_path("slug", repo.worktree_path(), &target);

        assert_eq!(before_id, after_id);
        assert_eq!(before_slug, after_slug, "target: {}", target.display());
    }
}

pub(crate) fn explicit_git_subdirectory_should_keep_exact_identity_and_config_scope() {
    let repo = git_worktree();
    write_file(
        &repo.root_path().join(".treeboot.toml"),
        "worktree_id = { length = 12 }\n",
    );
    write_file(
        &repo.worktree_path().join(".treeboot.toml"),
        "worktree_id = { length = 10 }\n",
    );
    let target = repo.worktree_path().join("nested").join("target");
    std::fs::create_dir_all(&target).expect("target should be created");

    let worktree = worktree_identity_for_path("id", repo.worktree_path(), repo.worktree_path());
    let subdirectory = worktree_identity_for_path("id", repo.worktree_path(), &target);

    assert_eq!(worktree.len(), 10);
    assert_eq!(subdirectory.len(), 6);
    assert_ne!(subdirectory, worktree);
}

pub(crate) fn explicit_regular_file_and_invalid_config_should_fail_before_stdout() {
    let parent = TempDir::new().expect("tempdir should be created");
    let file = parent.path().join("file");
    write_file(&file, "not a directory\n");

    for command in ["id", "slug"] {
        treeboot()
            .args(["worktree", command])
            .arg(&file)
            .current_dir(parent.path())
            .assert()
            .failure()
            .stdout(predicate::str::is_empty())
            .stderr(predicate::str::contains("is not a directory"));
    }

    let invalid = parent.path().join("invalid");
    std::fs::create_dir(&invalid).expect("invalid target should be created");
    write_file(
        &invalid.join(".treeboot.toml"),
        "worktree_id = { length = 0 }\n",
    );
    for command in ["id", "slug"] {
        treeboot()
            .args(["worktree", command])
            .arg(&invalid)
            .current_dir(parent.path())
            .assert()
            .failure()
            .stdout(predicate::str::is_empty())
            .stderr(predicate::str::contains("must be between 1 and 52"));
    }

    let unrelated = parent.path().join("unrelated-invalid");
    std::fs::create_dir(&unrelated).expect("invalid target should be created");
    write_file(
        &unrelated.join(".treeboot.toml"),
        "commands = [{ args = [\"orphan\"] }]\n",
    );
    for command in ["id", "slug"] {
        treeboot()
            .args(["worktree", command])
            .arg(&unrelated)
            .current_dir(parent.path())
            .assert()
            .failure()
            .stdout(predicate::str::is_empty())
            .stderr(predicate::str::contains(
                "command is missing required `run` or `program`",
            ));
    }
}

#[cfg(unix)]
pub(crate) fn explicit_dangling_symlink_should_fail_before_stdout() {
    let parent = TempDir::new().expect("tempdir should be created");
    let dangling = parent.path().join("dangling");
    symlink_file(parent.path().join("missing"), &dangling);

    for command in ["id", "slug"] {
        treeboot()
            .args(["worktree", command])
            .arg(&dangling)
            .current_dir(parent.path())
            .assert()
            .failure()
            .stdout(predicate::str::is_empty())
            .stderr(predicate::str::contains("is not a directory"));
    }
}

pub(crate) fn explicit_empty_path_should_be_a_usage_error_without_stdout() {
    for command in ["id", "slug"] {
        treeboot()
            .args(["worktree", command, ""])
            .assert()
            .code(2)
            .stdout(predicate::str::is_empty())
            .stderr(predicate::str::contains("PATH must not be empty"));
    }
}

#[cfg(windows)]
pub(crate) fn explicit_windows_special_relative_paths_should_fail_before_stdout() {
    let parent = TempDir::new().expect("tempdir should be created");
    for (path, reason) in [
        (r"C:relative", "drive-relative paths are not supported"),
        (
            r"\relative",
            "root-relative paths without a drive or share are not supported",
        ),
    ] {
        for command in ["id", "slug"] {
            treeboot()
                .args(["worktree", command, path])
                .current_dir(parent.path())
                .assert()
                .failure()
                .stdout(predicate::str::is_empty())
                .stderr(predicate::str::contains(reason));
        }
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn explicit_identity_path_parser_should_preserve_native_non_utf8_input() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let parent = TempDir::new().expect("tempdir should be created");
    let target = parent
        .path()
        .join(OsString::from_vec(b"target-\xff".to_vec()));
    std::fs::create_dir(&target).expect("target should be created");

    for command in ["id", "slug"] {
        treeboot()
            .args(["worktree", command])
            .arg(&target)
            .current_dir(parent.path())
            .assert()
            .success()
            .stderr(predicate::str::is_empty())
            .stdout(predicate::str::is_empty().not());
    }
}

pub(crate) fn explicit_identity_should_ignore_ambient_discovery_overrides() {
    let parent = TempDir::new().expect("tempdir should be created");
    let target = parent.path().join("target");
    std::fs::create_dir(&target).expect("target should be created");
    for command in ["id", "slug"] {
        let expected = worktree_identity_for_path(command, parent.path(), &target);

        treeboot()
            .args(["worktree", command])
            .arg(&target)
            .env("TREEBOOT_ROOT_PATH", parent.path().join("missing-root"))
            .env("CONDUCTOR_DEFAULT_BRANCH", "ambient-branch")
            .current_dir(parent.path())
            .assert()
            .success()
            .stderr(predicate::str::is_empty())
            .stdout(format!("{expected}\n"));
    }
}

pub(crate) fn worktree_identity_for_path(command: &str, cwd: &Path, path: &Path) -> String {
    let output = treeboot()
        .args(["worktree", command])
        .arg(path)
        .current_dir(cwd)
        .output()
        .expect("explicit identity command should run");
    assert!(
        output.status.success(),
        "explicit {command} should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("identity should be UTF-8")
        .trim()
        .to_owned()
}

pub(crate) fn worktree_list_should_skip_stale_registered_paths() {
    let repo = git_worktree();
    let stale_parent = TempDir::new().expect("stale parent should be created");
    let stale_path = stale_parent.path().join("stale");
    let stale_text = stale_path.to_str().expect("stale path should be UTF-8");
    git(
        &["worktree", "add", "-b", "treeboot-stale", stale_text],
        repo.root_path(),
    );
    std::fs::remove_dir_all(&stale_path).expect("stale worktree should be removed from disk");

    let root = canonical_path(repo.root_path()).display().to_string();
    treeboot()
        .args(["worktree", "list"])
        .current_dir(repo.root_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("stale").not())
        .stdout(predicate::str::contains(root));
}

// Git on macOS rejects non-UTF-8 worktree administrative directory names.
#[cfg(target_os = "linux")]
pub(crate) fn worktree_text_paths_should_preserve_native_bytes_and_structured_fail_atomically() {
    use std::ffi::OsString;
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    let repo = git_worktree();
    let parent = TempDir::new().expect("non-UTF-8 parent should be created");
    let path = parent
        .path()
        .join(OsString::from_vec(b"non-utf8-\xff".to_vec()));
    let output = crate::cases::support::host_process("git")
        .args(["worktree", "add", "-b", "treeboot-non-utf8"])
        .arg(&path)
        .current_dir(repo.root_path())
        .output()
        .expect("git should run");
    crate::cases::support::assert_fixture_process_success("git worktree add", &output);
    let path = canonical_path(&path);
    let id = worktree_id(&path);
    let mut expected_path = path.as_os_str().as_bytes().to_vec();
    expected_path.push(b'\n');

    let path_output = treeboot()
        .args(["worktree", "path", &id])
        .current_dir(repo.root_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    assert_eq!(path_output, expected_path);

    let list_output = treeboot()
        .args(["worktree", "list"])
        .current_dir(repo.root_path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    assert!(
        list_output
            .windows(expected_path.len())
            .any(|window| window == expected_path),
        "list should contain the exact native path bytes"
    );

    for args in [
        vec!["worktree", "path", &id, "--json"],
        vec!["worktree", "path", &id, "--yaml"],
        vec!["worktree", "list", "--format", "json"],
        vec!["worktree", "list", "--format", "yaml"],
    ] {
        treeboot()
            .args(args)
            .current_dir(repo.root_path())
            .assert()
            .failure()
            .stdout(predicate::str::is_empty())
            .stderr(predicate::str::is_empty().not());
    }
}

pub(crate) fn worktree_path_should_report_missing_and_ambiguous_ids_without_stdout() {
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
            "worktree_id = { length = 1 }\nworktree_slug = { max_length = 3 }\n",
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
            "worktree_id = { length = 1 }\nworktree_slug = { max_length = 3 }\n",
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
            stderr.contains(&format!("{:?}", canonical_path(path))),
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

pub(crate) fn malformed_sibling_config_should_fail_atomically_with_candidate_path() {
    let repo = git_worktree();
    let sibling = add_worktree(repo.root_path(), "malformed");
    write_file(
        &sibling.path.join(".treeboot.toml"),
        "worktree_id = { length = 0 }\n",
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
            .stderr(predicate::str::contains(format!(
                "{:?}",
                canonical_path(&sibling.path)
            )))
            .stderr(predicate::str::contains("length"));
    }
}

pub(crate) fn worktree_commands_should_fail_outside_git() {
    let dir = TempDir::new().expect("tempdir should be created");

    for args in [
        vec!["worktree", "id"],
        vec!["worktree", "slug"],
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

pub(crate) fn worktree_commands_should_honor_recognized_ambient_environment() {
    let repo = git_worktree();
    let missing_root = repo.root_path().join("missing-root");

    for args in [
        vec!["worktree", "id"],
        vec!["worktree", "slug"],
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

pub(crate) fn worktree_nested_help_and_version_should_be_exposed() {
    treeboot()
        .arg("worktree")
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::is_empty().not());

    treeboot()
        .args(["worktree", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("id"))
        .stdout(predicate::str::contains("slug"))
        .stdout(predicate::str::contains("path"))
        .stdout(predicate::str::contains("list"));

    treeboot()
        .args(["worktree", "id", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[PATH]"))
        .stdout(predicate::str::contains("--format"))
        .stdout(predicate::str::contains("--json"))
        .stdout(predicate::str::contains("--yaml"));

    treeboot()
        .args(["worktree", "slug", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[PATH]"))
        .stdout(predicate::str::contains("--format"));

    for flag in ["--version", "-V"] {
        treeboot()
            .args(["worktree", flag])
            .assert()
            .success()
            .stdout(predicate::str::contains(
                crate::cases::support::candidate_package_version(),
            ))
            .stdout(predicate::str::contains(format!(
                "(spec {})",
                crate::SPEC_VERSION
            )));
    }
}
