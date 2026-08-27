const ROOT_SCHEMA_JSON: &str = include_str!("../../../schemas/treeboot.schema.json");

#[test]
fn schema_should_describe_worktree_identity_settings() {
    let schema: serde_json::Value =
        serde_json::from_str(ROOT_SCHEMA_JSON).expect("schema should parse");
    assert_eq!(
        schema["properties"]["worktree_id"]["$ref"],
        "#/$defs/WorktreeIdConfig"
    );

    assert_eq!(
        schema["properties"]["worktree_slug"]["$ref"],
        "#/$defs/WorktreeSlugConfig"
    );

    let id_properties = &schema["$defs"]["WorktreeIdConfig"]["properties"];
    assert_eq!(id_properties["length"]["minimum"], 1);
    assert_eq!(id_properties["length"]["maximum"], 52);
    let slug_properties = &schema["$defs"]["WorktreeSlugConfig"]["properties"];
    assert_eq!(slug_properties["max_length"]["minimum"], 3);
    assert_eq!(
        slug_properties["separator"]["$ref"],
        "#/$defs/WorktreeSlugSeparator"
    );
    assert_eq!(
        schema["$defs"]["WorktreeSlugSeparator"]["enum"],
        serde_json::json!(["-", "_"])
    );
}

#[test]
fn schema_should_describe_both_teardown_command_forms() {
    let schema: serde_json::Value =
        serde_json::from_str(ROOT_SCHEMA_JSON).expect("schema should parse");
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
