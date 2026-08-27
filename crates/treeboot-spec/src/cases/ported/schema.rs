use predicates::prelude::*;
use std::collections::BTreeSet;
use tempfile::TempDir;

use crate::cases::support::treeboot;

const ROOT_SCHEMA_JSON: &str = crate::CONFIG_SCHEMA_JSON;

pub(crate) fn schema_should_print_or_write_embedded_schema() {
    let stdout = treeboot()
        .arg("schema")
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    parse_functional_schema(&stdout, "schema stdout");

    let temp = TempDir::new().expect("tempdir should be created");
    let output = temp.path().join("config.schema.json");
    treeboot()
        .args(["schema", "--output"])
        .arg(&output)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::is_empty());

    let content = std::fs::read(output).expect("schema should be written");
    parse_functional_schema(&content, "schema output file");
    assert_schema_bytes_equal(&content, &stdout, "schema stdout and --output file differ");
}

pub(crate) fn schema_output_short_flag_should_write_file() {
    let temp = TempDir::new().expect("tempdir should be created");
    let output = temp.path().join("schema.json");

    treeboot()
        .args(["schema", "-o"])
        .arg(&output)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::is_empty());

    let content = std::fs::read(output).expect("schema should be written");
    parse_functional_schema(&content, "schema -o output file");
}

pub(crate) fn schema_stdout_should_match_canonical_bytes() {
    let stdout = treeboot()
        .arg("schema")
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    assert_schema_bytes_equal(
        &stdout,
        ROOT_SCHEMA_JSON.as_bytes(),
        "schema stdout differs from the canonical schema",
    );
}

pub(crate) fn schema_short_output_should_match_canonical_bytes() {
    let temp = TempDir::new().expect("tempdir should be created");
    let output = temp.path().join("schema.json");
    treeboot()
        .args(["schema", "-o"])
        .arg(&output)
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::is_empty());
    let content = std::fs::read(output).expect("schema should be written");
    assert_schema_bytes_equal(
        &content,
        ROOT_SCHEMA_JSON.as_bytes(),
        "schema -o output differs from the canonical schema",
    );
}

fn parse_schema(bytes: &[u8], label: &str) -> serde_json::Value {
    serde_json::from_slice(bytes)
        .unwrap_or_else(|error| panic!("{label} is not valid JSON: {error}"))
}

fn parse_functional_schema(bytes: &[u8], label: &str) -> serde_json::Value {
    let schema = parse_schema(bytes, label);
    let object = schema
        .as_object()
        .unwrap_or_else(|| panic!("{label} must be a JSON object"));
    assert!(
        object
            .get("$schema")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| !value.is_empty()),
        "{label} must declare a non-empty $schema string"
    );
    assert_eq!(
        object.get("type").and_then(serde_json::Value::as_str),
        Some("object"),
        "{label} must describe an object"
    );
    assert!(
        object
            .get("properties")
            .and_then(serde_json::Value::as_object)
            .is_some_and(|properties| !properties.is_empty()),
        "{label} must declare a non-empty properties object"
    );
    schema
}

fn assert_schema_bytes_equal(actual: &[u8], expected: &[u8], label: &str) {
    let actual_value = parse_schema(actual, "candidate schema");
    let expected_value = parse_schema(expected, "canonical schema");
    if actual_value != expected_value {
        let mut pointers = Vec::new();
        collect_json_differences(&actual_value, &expected_value, "", &mut pointers, 13);
        let suffix = if pointers.len() > 12 {
            pointers.truncate(12);
            "\n  ... additional differences omitted"
        } else {
            ""
        };
        panic!(
            "{label}; differing JSON pointers:\n  {}{suffix}",
            pointers.join("\n  ")
        );
    }
    if actual != expected {
        let offset = actual
            .iter()
            .zip(expected)
            .position(|(actual, expected)| actual != expected)
            .unwrap_or_else(|| actual.len().min(expected.len()));
        let (line, column) = byte_location(actual, offset);
        panic!(
            "{label}; JSON structures match but bytes first differ at offset {offset} (line {line}, column {column}); candidate length {}, canonical length {}",
            actual.len(),
            expected.len()
        );
    }
}

fn collect_json_differences(
    actual: &serde_json::Value,
    expected: &serde_json::Value,
    pointer: &str,
    differences: &mut Vec<String>,
    limit: usize,
) {
    if differences.len() >= limit || actual == expected {
        return;
    }
    match (actual, expected) {
        (serde_json::Value::Object(actual), serde_json::Value::Object(expected)) => {
            let keys = actual
                .keys()
                .chain(expected.keys())
                .collect::<BTreeSet<_>>();
            for key in keys {
                let child = format!("{pointer}/{}", escape_json_pointer(key));
                match (actual.get(key), expected.get(key)) {
                    (Some(actual), Some(expected)) => {
                        collect_json_differences(actual, expected, &child, differences, limit)
                    }
                    _ => differences.push(child),
                }
                if differences.len() >= limit {
                    break;
                }
            }
        }
        (serde_json::Value::Array(actual), serde_json::Value::Array(expected)) => {
            let length = actual.len().max(expected.len());
            for index in 0..length {
                let child = format!("{pointer}/{index}");
                match (actual.get(index), expected.get(index)) {
                    (Some(actual), Some(expected)) => {
                        collect_json_differences(actual, expected, &child, differences, limit)
                    }
                    _ => differences.push(child),
                }
                if differences.len() >= limit {
                    break;
                }
            }
        }
        _ => differences.push(if pointer.is_empty() {
            "/".to_owned()
        } else {
            pointer.to_owned()
        }),
    }
}

fn escape_json_pointer(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

fn byte_location(bytes: &[u8], offset: usize) -> (usize, usize) {
    let before = &bytes[..offset.min(bytes.len())];
    let line = before.iter().filter(|byte| **byte == b'\n').count() + 1;
    let column = before
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(before.len() + 1, |newline| before.len() - newline);
    (line, column)
}

pub(crate) fn schema_should_fail_when_output_parent_is_missing() {
    let temp = TempDir::new().expect("tempdir should be created");
    let output = temp.path().join("missing").join("schema.json");

    treeboot()
        .args(["schema", "--output"])
        .arg(&output)
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::is_empty().not());
}

pub(crate) fn schema_should_not_accept_report_format_options() {
    treeboot()
        .args(["schema", "--json"])
        .assert()
        .code(2)
        .stderr(predicate::str::is_empty().not());

    treeboot()
        .args(["schema", "--format", "yaml"])
        .assert()
        .code(2)
        .stderr(predicate::str::is_empty().not());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_difference_should_report_bounded_json_pointers() {
        let panic = std::panic::catch_unwind(|| {
            assert_schema_bytes_equal(
                br#"{"a":{"b":2},"extra":true}"#,
                br#"{"a":{"b":1},"expected":true}"#,
                "schema mismatch",
            );
        })
        .expect_err("different schemas should fail");
        let message = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .expect("schema assertion should panic with text");

        assert!(message.contains("/a/b"), "{message}");
        assert!(message.contains("/expected"), "{message}");
        assert!(message.contains("/extra"), "{message}");
        assert!(message.len() < 512, "{message}");
    }

    #[test]
    fn byte_only_schema_difference_should_report_first_location() {
        let panic = std::panic::catch_unwind(|| {
            assert_schema_bytes_equal(b"{\n  \"a\": 1\n}\n", b"{\"a\":1}\n", "schema mismatch");
        })
        .expect_err("byte-distinct schemas should fail");
        let message = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .expect("schema assertion should panic with text");

        assert!(message.contains("structures match"), "{message}");
        assert!(message.contains("line 1, column 2"), "{message}");
        assert!(message.len() < 512, "{message}");
    }

    #[test]
    fn functional_schema_floor_should_reject_empty_or_non_object_documents() {
        for schema in [br#"{}"#.as_slice(), br#"null"#.as_slice()] {
            let panic = std::panic::catch_unwind(|| {
                parse_functional_schema(schema, "candidate schema");
            });

            assert!(
                panic.is_err(),
                "schema should not satisfy the functional floor"
            );
        }
    }
}
