use treeboot_spec::CONFIG_SCHEMA_JSON;

fn schema() -> serde_json::Value {
    serde_json::from_str(CONFIG_SCHEMA_JSON).expect("canonical schema should be valid JSON")
}

#[test]
fn canonical_schema_has_a_supported_shape_and_resolvable_local_references() {
    let schema = schema();

    assert_eq!(
        schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(schema["type"], "object");
    assert!(schema["additionalProperties"].is_boolean());
    assert!(schema["$defs"].is_object());

    let mut references = Vec::new();
    collect_local_references(&schema, &mut references);
    assert!(!references.is_empty());
    for reference in references {
        let pointer = reference
            .strip_prefix('#')
            .expect("schema self-test only collects local references");
        assert!(
            schema.pointer(pointer).is_some(),
            "unresolved local schema reference: {reference}"
        );
    }
}

#[test]
fn canonical_schema_describes_worktree_identity_settings() {
    let schema = schema();
    assert_eq!(
        schema["properties"]["worktree_id"]["$ref"],
        "#/$defs/WorktreeIdConfig"
    );
    assert_eq!(
        schema["properties"]["worktree_slug"]["$ref"],
        "#/$defs/WorktreeSlugConfig"
    );

    let id = &schema["$defs"]["WorktreeIdConfig"]["properties"];
    assert_eq!(id["length"]["minimum"], 1);
    assert_eq!(id["length"]["maximum"], 52);
    let slug = &schema["$defs"]["WorktreeSlugConfig"]["properties"];
    assert_eq!(slug["max_length"]["minimum"], 3);
    assert_eq!(slug["separator"]["$ref"], "#/$defs/WorktreeSlugSeparator");
    assert_eq!(
        schema["$defs"]["WorktreeSlugSeparator"]["enum"],
        serde_json::json!(["-", "_"])
    );
}

#[test]
fn canonical_schema_describes_both_teardown_command_forms() {
    let schema = schema();
    let properties = &schema["properties"];

    assert_eq!(properties["teardown_commands"]["type"], "array");
    assert_eq!(
        properties["teardown_commands"]["items"]["$ref"],
        "#/$defs/CommandEntry"
    );
    assert_eq!(properties["teardown_command"]["type"], "array");
    assert_eq!(
        properties["teardown_command"]["items"]["$ref"],
        "#/$defs/CommandObject"
    );
}

#[test]
fn canonical_schema_leaves_runtime_policy_and_pattern_syntax_to_conformance_cases() {
    let schema = schema();

    assert_eq!(schema["properties"]["strict"]["type"], "boolean");
    assert_eq!(schema["$defs"]["SyncEntry"]["anyOf"][0]["type"], "string");
    let include_item = &schema["$defs"]["CopyObject"]["properties"]["include"]["items"];
    assert_eq!(include_item["type"], "string");
    assert!(include_item.get("pattern").is_none());

    assert!(
        treeboot_spec::Suite::current()
            .cases()
            .any(|case| case.id() == "run.config-strict-sync.exit-with-config-error")
    );
    assert!(
        treeboot_spec::Suite::current()
            .cases()
            .any(|case| case.id() == "closure.manual.command-wide-input-diagnostic")
    );
}

fn collect_local_references<'a>(value: &'a serde_json::Value, references: &mut Vec<&'a str>) {
    match value {
        serde_json::Value::Object(object) => {
            if let Some(reference) = object.get("$ref").and_then(serde_json::Value::as_str)
                && reference.starts_with('#')
            {
                references.push(reference);
            }
            for value in object.values() {
                collect_local_references(value, references);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                collect_local_references(value, references);
            }
        }
        _ => {}
    }
}
