use predicates::prelude::*;

use crate::cases::support::{assert_json_object_keys, parse_json, treeboot};

pub(crate) fn version_command_should_print_package_and_spec_version() {
    let output = treeboot()
        .arg("version")
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    assert_version_text_shape(&output);
}

pub(crate) fn version_command_should_support_json_yaml_and_text_formats() {
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
    assert_eq!(
        json["version"],
        crate::cases::support::candidate_package_version()
    );
    assert!(
        json["spec_version"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
    );

    let yaml = treeboot()
        .args(["version", "--format", "yaml"])
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    assert!(yaml.ends_with(b"\n"));
    assert!(!yaml.ends_with(b"\n\n"));
    let yaml: serde_json::Value = yaml_serde::from_slice(&yaml).expect("version YAML should parse");
    assert_json_object_keys(&yaml, &["package", "spec_version", "version"]);
    assert_eq!(yaml["package"], "treeboot");
    assert_eq!(
        yaml["version"],
        crate::cases::support::candidate_package_version()
    );
    assert!(
        yaml["spec_version"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
    );

    let text = treeboot()
        .args(["version", "--format", "text"])
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    assert_version_text_shape(&text);
}

pub(crate) fn version_text_should_declare_suite_spec() {
    treeboot()
        .arg("version")
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "(spec {})",
            crate::SPEC_VERSION
        )));
}

pub(crate) fn version_formats_should_declare_suite_spec() {
    let json = treeboot()
        .args(["version", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json = parse_json(json, "version");
    assert_eq!(json["spec_version"], crate::SPEC_VERSION);

    let yaml = treeboot()
        .args(["version", "--format", "yaml"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let yaml: serde_json::Value = yaml_serde::from_slice(&yaml).expect("version YAML should parse");
    assert_eq!(yaml["spec_version"], crate::SPEC_VERSION);

    treeboot()
        .args(["version", "--format", "text"])
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "(spec {})",
            crate::SPEC_VERSION
        )));
}

fn assert_version_text_shape(output: &[u8]) {
    let text = std::str::from_utf8(output).expect("version text should be valid UTF-8");
    let prefix = format!(
        "treeboot {} (spec ",
        crate::cases::support::candidate_package_version()
    );
    assert!(
        text.starts_with(&prefix),
        "unexpected version text: {text:?}"
    );
    assert!(text.ends_with(")\n"), "unexpected version text: {text:?}");
    assert!(
        text.len() > prefix.len() + 2,
        "spec version should not be empty"
    );
}

pub(crate) fn version_command_output_shortcuts_should_conflict_with_format() {
    treeboot()
        .args(["version", "--json", "--format", "yaml"])
        .assert()
        .code(2)
        .stderr(predicate::str::is_empty().not());

    treeboot()
        .args(["version", "--json", "--yaml"])
        .assert()
        .code(2)
        .stderr(predicate::str::is_empty().not());
}
