use predicates::prelude::*;
use tempfile::TempDir;

mod common;

use common::treeboot;

const ROOT_SCHEMA_JSON: &str = include_str!("../../../schemas/treeboot.schema.json");

#[test]
fn schema_should_print_or_write_embedded_schema() {
    treeboot()
        .arg("schema")
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(ROOT_SCHEMA_JSON);

    let temp = TempDir::new().expect("tempdir should be created");
    let output = temp.path().join("config.schema.json");
    treeboot()
        .args(["schema", "--output"])
        .arg(&output)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::is_empty());

    let content = std::fs::read_to_string(output).expect("schema should be written");
    assert_eq!(content, ROOT_SCHEMA_JSON);
}

#[test]
fn schema_output_short_flag_should_write_file() {
    let temp = TempDir::new().expect("tempdir should be created");
    let output = temp.path().join("schema.json");

    treeboot()
        .args(["schema", "-o"])
        .arg(&output)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::is_empty());

    let content = std::fs::read_to_string(output).expect("schema should be written");
    assert_eq!(content, ROOT_SCHEMA_JSON);
}

#[test]
fn schema_should_fail_when_output_parent_is_missing() {
    let temp = TempDir::new().expect("tempdir should be created");
    let output = temp.path().join("missing").join("schema.json");

    treeboot()
        .args(["schema", "--output"])
        .arg(&output)
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("failed to write output"))
        .stderr(predicate::str::contains("No such file or directory"));
}

#[test]
fn schema_should_not_accept_report_format_options() {
    treeboot()
        .args(["schema", "--json"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("unexpected argument"));

    treeboot()
        .args(["schema", "--format", "yaml"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("unexpected argument"));
}
