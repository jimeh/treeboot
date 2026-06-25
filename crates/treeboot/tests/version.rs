use predicates::prelude::*;

mod common;

use common::{assert_json_object_keys, parse_json, treeboot};

#[test]
fn version_command_should_print_package_and_spec_version() {
    treeboot()
        .arg("version")
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(format!(
            "treeboot {} (spec {})\n",
            env!("CARGO_PKG_VERSION"),
            treeboot_core::SPEC_VERSION
        ));
}

#[test]
fn version_command_should_support_json_yaml_and_text_formats() {
    let json = treeboot()
        .args(["version", "--json"])
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let json = parse_json(json, "version");
    assert_json_object_keys(&json, &["package", "spec_version", "version"]);
    assert_eq!(json["package"], "treeboot");
    assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(json["spec_version"], treeboot_core::SPEC_VERSION);

    treeboot()
        .args(["version", "--format", "yaml"])
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("package: treeboot"))
        .stdout(predicate::str::contains(format!(
            "spec_version: {}",
            treeboot_core::SPEC_VERSION
        )));

    treeboot()
        .args(["version", "--format", "text"])
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(format!(
            "treeboot {} (spec {})\n",
            env!("CARGO_PKG_VERSION"),
            treeboot_core::SPEC_VERSION
        ));
}

#[test]
fn version_command_output_shortcuts_should_conflict_with_format() {
    treeboot()
        .args(["version", "--json", "--format", "yaml"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("cannot be used with"));

    treeboot()
        .args(["version", "--json", "--yaml"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("cannot be used with"));
}
