#![expect(
    dead_code,
    reason = "schema marker types are consumed by schemars derive"
)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::Serialize;

#[derive(JsonSchema, Serialize)]
#[serde(deny_unknown_fields)]
struct TreebootConfig {
    /// Enables strict declarative validation and conflict handling.
    #[serde(skip_serializing_if = "Option::is_none")]
    strict: Option<bool>,
    /// Default path ignore patterns prepended to copy and sync operations.
    #[serde(skip_serializing_if = "Option::is_none")]
    default_ignore: Option<Vec<String>>,
    /// Stable compact worktree ID settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    worktree_id: Option<WorktreeIdConfig>,
    /// Readable worktree slug presentation settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    worktree_slug: Option<WorktreeSlugConfig>,
    /// Allows file operation sources outside the root checkout.
    #[serde(skip_serializing_if = "Option::is_none")]
    dangerously_allow_sources_outside_root: Option<bool>,
    /// Allows file operation targets outside the current worktree.
    #[serde(skip_serializing_if = "Option::is_none")]
    dangerously_allow_targets_outside_worktree: Option<bool>,
    /// Copy file entries. Entries run before symlink, sync, files, and file.
    #[serde(skip_serializing_if = "Option::is_none")]
    copy: Option<Vec<CopyEntry>>,
    /// Symlink file entries. Entries run after copy and before sync.
    #[serde(skip_serializing_if = "Option::is_none")]
    symlink: Option<Vec<SymlinkEntry>>,
    /// Sync file entries. Entries run after symlink and before files.
    #[serde(skip_serializing_if = "Option::is_none")]
    sync: Option<Vec<SyncEntry>>,
    /// Mixed file operation entries. Entries run after copy, symlink, and sync.
    #[serde(skip_serializing_if = "Option::is_none")]
    files: Option<Vec<MixedFileObject>>,
    /// Verbose mixed file operation entries. TOML uses this as [[file]].
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<Vec<MixedFileObject>>,
    /// Command entries. Entries run before verbose [[command]] entries.
    #[serde(skip_serializing_if = "Option::is_none")]
    commands: Option<Vec<CommandEntry>>,
    /// Verbose command entries. TOML uses this as [[command]].
    #[serde(skip_serializing_if = "Option::is_none")]
    command: Option<Vec<CommandObject>>,
    /// Teardown command entries. Entries run before verbose
    /// [[teardown_command]] entries.
    #[serde(skip_serializing_if = "Option::is_none")]
    teardown_commands: Option<Vec<CommandEntry>>,
    /// Verbose teardown entries. TOML uses this as [[teardown_command]].
    #[serde(skip_serializing_if = "Option::is_none")]
    teardown_command: Option<Vec<CommandObject>>,
}

#[derive(JsonSchema, Serialize)]
#[serde(deny_unknown_fields)]
struct WorktreeIdConfig {
    /// Number of lowercase Crockford base32 digest characters.
    #[schemars(range(min = 1, max = 52))]
    #[serde(skip_serializing_if = "Option::is_none")]
    length: Option<usize>,
}

#[derive(JsonSchema, Serialize)]
#[serde(deny_unknown_fields)]
struct WorktreeSlugConfig {
    /// Maximum length of the complete slug.
    #[schemars(range(min = 3))]
    #[serde(skip_serializing_if = "Option::is_none")]
    max_length: Option<usize>,
    /// Separator used inside the readable name and before the ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    separator: Option<WorktreeSlugSeparator>,
}

#[derive(JsonSchema, Serialize)]
enum WorktreeSlugSeparator {
    #[serde(rename = "-")]
    Hyphen,
    #[serde(rename = "_")]
    Underscore,
}

#[derive(JsonSchema, Serialize)]
#[serde(untagged)]
enum CopyEntry {
    /// Source and target use the same relative path.
    Path(String),
    /// Copy object entry.
    Object(CopyObject),
}

#[derive(JsonSchema, Serialize)]
#[serde(untagged)]
enum SymlinkEntry {
    /// Source and target use the same relative path.
    Path(String),
    /// Symlink object entry.
    Object(SymlinkObject),
}

#[derive(JsonSchema, Serialize)]
#[serde(untagged)]
enum SyncEntry {
    /// Source and target use the same relative path.
    Path(String),
    /// Sync object entry.
    Object(SyncObject),
}

#[derive(JsonSchema, Serialize)]
#[serde(untagged)]
enum CommandEntry {
    /// Shell command string.
    Run(String),
    /// Command object entry.
    Object(CommandObject),
}

#[derive(JsonSchema, Serialize)]
#[serde(deny_unknown_fields)]
struct CopyObject {
    /// Source path, relative to the root checkout unless absolute.
    source: String,
    /// Target path, relative to the current worktree unless absolute.
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<String>,
    /// Whether a missing source should fail validation.
    #[serde(skip_serializing_if = "Option::is_none")]
    required: Option<bool>,
    /// How safe source symlinks are handled.
    #[serde(skip_serializing_if = "Option::is_none")]
    symlinks: Option<SymlinkMode>,
    /// Source-relative path patterns that narrow directory copies to
    /// matching source paths.
    #[serde(skip_serializing_if = "Option::is_none")]
    include: Option<Vec<String>>,
    /// Source-relative path patterns that copy should skip.
    #[serde(skip_serializing_if = "Option::is_none")]
    ignore: Option<Vec<String>>,
    /// Metadata fields that copy should not compare or apply.
    #[serde(skip_serializing_if = "Option::is_none")]
    ignore_metadata: Option<Vec<MetadataField>>,
}

#[derive(JsonSchema, Serialize)]
#[serde(deny_unknown_fields)]
struct SymlinkObject {
    /// Source path, relative to the root checkout unless absolute.
    source: String,
    /// Target path, relative to the current worktree unless absolute.
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<String>,
    /// Whether a missing source should fail validation.
    #[serde(skip_serializing_if = "Option::is_none")]
    required: Option<bool>,
}

#[derive(JsonSchema, Serialize)]
#[serde(deny_unknown_fields)]
#[schemars(extend(
    "not" = {
        "properties": {
            "delete": { "const": true },
            "include": { "minItems": 1 }
        },
        "required": ["delete", "include"]
    }
))]
struct SyncObject {
    /// Source path, relative to the root checkout unless absolute.
    source: String,
    /// Target path, relative to the current worktree unless absolute.
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<String>,
    /// Whether a missing source should fail validation.
    #[serde(skip_serializing_if = "Option::is_none")]
    required: Option<bool>,
    /// File comparison mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    compare: Option<SyncCompare>,
    /// Whether target-only files are deleted for directory sync.
    #[serde(skip_serializing_if = "Option::is_none")]
    delete: Option<bool>,
    /// How safe source symlinks are handled.
    #[serde(skip_serializing_if = "Option::is_none")]
    symlinks: Option<SymlinkMode>,
    /// Source-relative path patterns that narrow directory sync to matching
    /// source paths. A non-empty list cannot combine with `delete = true`.
    #[serde(skip_serializing_if = "Option::is_none")]
    include: Option<Vec<String>>,
    /// Source-relative path patterns that sync should skip.
    #[serde(skip_serializing_if = "Option::is_none")]
    ignore: Option<Vec<String>>,
    /// Metadata fields that sync should not compare or apply.
    #[serde(skip_serializing_if = "Option::is_none")]
    ignore_metadata: Option<Vec<MetadataField>>,
}

#[derive(JsonSchema, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
enum MixedFileObject {
    /// Copy object entry for mixed `files` and `[[file]]` declarations.
    Copy {
        /// Source path, relative to the root checkout unless absolute.
        source: String,
        /// Target path, relative to the current worktree unless absolute.
        #[serde(skip_serializing_if = "Option::is_none")]
        target: Option<String>,
        /// Whether a missing source should fail validation.
        #[serde(skip_serializing_if = "Option::is_none")]
        required: Option<bool>,
        /// How safe source symlinks are handled.
        #[serde(skip_serializing_if = "Option::is_none")]
        symlinks: Option<SymlinkMode>,
        /// Source-relative path patterns that narrow directory copies to
        /// matching source paths.
        #[serde(skip_serializing_if = "Option::is_none")]
        include: Option<Vec<String>>,
        /// Source-relative path patterns that copy should skip.
        #[serde(skip_serializing_if = "Option::is_none")]
        ignore: Option<Vec<String>>,
        /// Metadata fields that copy should not compare or apply.
        #[serde(skip_serializing_if = "Option::is_none")]
        ignore_metadata: Option<Vec<MetadataField>>,
    },
    /// Symlink object entry for mixed `files` and `[[file]]` declarations.
    Symlink {
        /// Source path, relative to the root checkout unless absolute.
        source: String,
        /// Target path, relative to the current worktree unless absolute.
        #[serde(skip_serializing_if = "Option::is_none")]
        target: Option<String>,
        /// Whether a missing source should fail validation.
        #[serde(skip_serializing_if = "Option::is_none")]
        required: Option<bool>,
    },
    /// Sync object entry for mixed `files` and `[[file]]` declarations.
    #[schemars(extend(
        "not" = {
            "properties": {
                "delete": { "const": true },
                "include": { "minItems": 1 }
            },
            "required": ["delete", "include"]
        }
    ))]
    Sync {
        /// Source path, relative to the root checkout unless absolute.
        source: String,
        /// Target path, relative to the current worktree unless absolute.
        #[serde(skip_serializing_if = "Option::is_none")]
        target: Option<String>,
        /// Whether a missing source should fail validation.
        #[serde(skip_serializing_if = "Option::is_none")]
        required: Option<bool>,
        /// File comparison mode.
        #[serde(skip_serializing_if = "Option::is_none")]
        compare: Option<SyncCompare>,
        /// Whether target-only files are deleted for directory sync.
        #[serde(skip_serializing_if = "Option::is_none")]
        delete: Option<bool>,
        /// How safe source symlinks are handled.
        #[serde(skip_serializing_if = "Option::is_none")]
        symlinks: Option<SymlinkMode>,
        /// Source-relative path patterns that narrow directory sync to
        /// matching source paths. A non-empty list cannot combine with
        /// `delete = true`.
        #[serde(skip_serializing_if = "Option::is_none")]
        include: Option<Vec<String>>,
        /// Source-relative path patterns that sync should skip.
        #[serde(skip_serializing_if = "Option::is_none")]
        ignore: Option<Vec<String>>,
        /// Metadata fields that sync should not compare or apply.
        #[serde(skip_serializing_if = "Option::is_none")]
        ignore_metadata: Option<Vec<MetadataField>>,
    },
}

#[derive(JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
enum SyncCompare {
    Metadata,
    Checksum,
}

#[derive(JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
enum SymlinkMode {
    Preserve,
}

#[derive(JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
enum MetadataField {
    Permissions,
    Owner,
    Group,
    Ownership,
}

#[derive(JsonSchema, Serialize)]
#[serde(untagged)]
enum CommandObject {
    /// Shell command object.
    Shell(ShellCommandObject),
    /// Direct program command object.
    Direct(DirectCommandObject),
}

#[derive(JsonSchema, Serialize)]
#[serde(deny_unknown_fields)]
struct ShellCommandObject {
    /// Optional display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    /// Shell command to execute.
    run: String,
    /// Command working directory, relative to the worktree unless absolute.
    #[serde(skip_serializing_if = "Option::is_none")]
    cwd: Option<String>,
    /// Extra environment variables for this command.
    #[serde(skip_serializing_if = "Option::is_none")]
    env: Option<BTreeMap<String, String>>,
    /// Whether a non-zero exit status should be non-fatal.
    #[serde(skip_serializing_if = "Option::is_none")]
    allow_failure: Option<bool>,
}

#[derive(JsonSchema, Serialize)]
#[serde(deny_unknown_fields)]
struct DirectCommandObject {
    /// Optional display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    /// Program executable.
    program: String,
    /// Program arguments.
    #[serde(skip_serializing_if = "Option::is_none")]
    args: Option<Vec<String>>,
    /// Command working directory, relative to the worktree unless absolute.
    #[serde(skip_serializing_if = "Option::is_none")]
    cwd: Option<String>,
    /// Extra environment variables for this command.
    #[serde(skip_serializing_if = "Option::is_none")]
    env: Option<BTreeMap<String, String>>,
    /// Whether a non-zero exit status should be non-fatal.
    #[serde(skip_serializing_if = "Option::is_none")]
    allow_failure: Option<bool>,
}

fn main() {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("schemas/treeboot.schema.json"));
    let schema = treeboot_config_schema();
    let content = serde_json::to_string_pretty(&schema).expect("schema should serialize as JSON");

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("schema parent directory should be created");
    }

    std::fs::write(&path, format!("{content}\n")).expect("schema should be written");
}

fn treeboot_config_schema() -> serde_json::Value {
    let schema = schemars::schema_for!(TreebootConfig);
    let mut schema = serde_json::to_value(schema).expect("schema should serialize as JSON");
    strip_null_type(&mut schema);
    schema
}

fn strip_null_type(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Array(items) => {
            for item in items {
                strip_null_type(item);
            }
        }
        serde_json::Value::Object(object) => {
            strip_null_type_array(object);
            strip_null_any_of(object);

            for value in object.values_mut() {
                strip_null_type(value);
            }
        }
        _ => {}
    }
}

fn strip_null_type_array(object: &mut serde_json::Map<String, serde_json::Value>) {
    let Some(serde_json::Value::Array(types)) = object.get_mut("type") else {
        return;
    };

    types.retain(|item| item.as_str() != Some("null"));

    if types.len() == 1 {
        let only = types.pop().expect("single schema type should exist");
        object.insert("type".to_owned(), only);
    }
}

fn strip_null_any_of(object: &mut serde_json::Map<String, serde_json::Value>) {
    let Some(serde_json::Value::Array(any_of)) = object.get_mut("anyOf") else {
        return;
    };

    any_of.retain(|item| {
        !matches!(
            item,
            serde_json::Value::Object(schema)
                if schema.get("type").and_then(serde_json::Value::as_str) == Some("null")
        )
    });

    if any_of.len() == 1 {
        let only = any_of.pop().expect("single anyOf schema should exist");

        if let serde_json::Value::Object(only) = only {
            object.remove("anyOf");
            object.extend(only);
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::treeboot_config_schema;

    #[test]
    fn schema_should_reject_include_with_delete_for_sync_entries() {
        let schema = treeboot_config_schema();

        assert_eq!(
            schema.pointer("/$defs/SyncObject/not"),
            Some(&include_with_delete_shape())
        );
    }

    #[test]
    fn schema_should_reject_include_with_delete_for_mixed_sync_entries() {
        let schema = treeboot_config_schema();
        let sync = schema
            .pointer("/$defs/MixedFileObject/oneOf")
            .and_then(Value::as_array)
            .and_then(|variants| {
                variants.iter().find(|variant| {
                    variant.pointer("/properties/operation/const") == Some(&json!("sync"))
                })
            });

        assert_eq!(
            sync.and_then(|variant| variant.get("not")),
            Some(&include_with_delete_shape())
        );
    }

    fn include_with_delete_shape() -> Value {
        json!({
            "properties": {
                "delete": { "const": true },
                "include": { "minItems": 1 }
            },
            "required": ["delete", "include"]
        })
    }
}
