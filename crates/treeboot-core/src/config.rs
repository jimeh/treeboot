use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::de::{self, MapAccess, Visitor, value::MapAccessDeserializer};
use serde::{Deserialize, Serialize};
use toml::Spanned;

use crate::context;
use crate::discovery;
use crate::run::RunOptions;
use crate::{Error, Result, RunContext};

/// Options for inspecting a treeboot config.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigOptions {
    /// Directory from which config discovery starts.
    pub cwd: Option<PathBuf>,
    /// Overrides the root checkout used for resolved source paths.
    pub root: Option<PathBuf>,
    /// Uses one specific config file instead of discovery.
    pub config: Option<PathBuf>,
}

/// Result summary for a `treeboot config` invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigReport {
    /// Runtime context used while resolving config paths.
    pub context: RunContext,
    /// Config file path.
    pub path: PathBuf,
    /// Parsed and normalized config.
    pub config: Config,
}

/// Parsed and normalized treeboot config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Config {
    /// Ordered file operations.
    pub files: Vec<FileOperation>,
    /// Ordered command operations.
    pub commands: Vec<CommandOperation>,
    /// Declarative validation settings.
    pub validation: ValidationOptions,
}

/// A normalized file operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileOperation {
    /// File operation kind.
    pub operation: FileOperationKind,
    /// Declared source path.
    pub source: PathBuf,
    /// Declared target path.
    pub target: PathBuf,
    /// Source path resolved from the root checkout.
    pub source_path: PathBuf,
    /// Target path resolved from the current worktree.
    pub target_path: PathBuf,
    /// Whether a missing source should fail validation.
    pub required: bool,
    /// Sync comparison mode.
    pub compare: Option<SyncCompare>,
    /// Whether sync should delete target-only files.
    pub delete_extra: Option<bool>,
    /// How copy and sync should treat source symlinks.
    pub symlinks: Option<SymlinkMode>,
    /// Source location for the operation declaration.
    pub declaration: SourceSpan,
}

/// File operation kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileOperationKind {
    /// Copy source content to the target.
    Copy,
    /// Create a target symlink to the source.
    Symlink,
    /// Reconcile target content with source content.
    Sync,
}

/// Sync comparison mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncCompare {
    /// Compare size and modified time.
    Metadata,
    /// Compare file contents.
    Checksum,
}

/// Copy or sync symlink handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymlinkMode {
    /// Recreate safe source symlinks as symlinks.
    Preserve,
}

/// A normalized command operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommandOperation {
    /// Optional display name.
    pub name: Option<String>,
    /// Command invocation.
    pub command: CommandKind,
    /// Declared working directory.
    pub cwd: Option<PathBuf>,
    /// Working directory resolved from the current worktree.
    pub cwd_path: Option<PathBuf>,
    /// Extra environment variables for this command.
    pub env: BTreeMap<String, String>,
    /// Whether this command can run in an async batch.
    #[serde(rename = "async")]
    pub async_command: bool,
    /// Whether a non-zero exit status should be non-fatal.
    pub allow_failure: bool,
    /// Source location for the command declaration.
    pub declaration: SourceSpan,
}

/// Command invocation kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CommandKind {
    /// Shell command invocation.
    Shell {
        /// Shell command string.
        run: String,
    },
    /// Direct program invocation.
    Direct {
        /// Program executable.
        program: String,
        /// Program arguments.
        args: Vec<String>,
    },
}

/// Declarative validation settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ValidationOptions {
    /// Allows file operation sources outside the root checkout.
    pub dangerously_allow_sources_outside_root: bool,
    /// Allows file operation targets outside the current worktree.
    pub dangerously_allow_targets_outside_worktree: bool,
}

/// Byte and line location for a declaration in a config file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SourceSpan {
    /// Starting byte offset.
    pub start: usize,
    /// Ending byte offset.
    pub end: usize,
    /// One-based starting line.
    pub line: usize,
    /// One-based starting column.
    pub column: usize,
}

/// Parses, normalizes, and returns the selected config file.
///
/// # Errors
///
/// Returns an error if context discovery fails, no config exists, the requested
/// config path does not exist, the config cannot be read, or TOML parsing and
/// normalization fails.
pub fn inspect_config(options: ConfigOptions) -> Result<ConfigReport> {
    let run_options = RunOptions {
        cwd: options.cwd,
        root: options.root,
        config: options.config.clone(),
        ..RunOptions::default()
    };
    let context = context::resolve(&run_options)?;
    let path = discovery::discover_config(&context.worktree_path, options.config.as_deref())?
        .ok_or(Error::NoConfigDetectedStrict)?;
    let config = load_config(&path, &context)?;

    Ok(ConfigReport {
        context,
        path,
        config,
    })
}

pub(crate) fn load_config(path: &Path, context: &RunContext) -> Result<Config> {
    let content = std::fs::read_to_string(path).map_err(|source| Error::ConfigIo {
        path: path.to_path_buf(),
        source,
    })?;

    parse_config(path, &content, context)
}

fn parse_config(path: &Path, content: &str, context: &RunContext) -> Result<Config> {
    let raw: RawConfig = toml::from_str(content).map_err(|source| {
        let message = parse_error_message(content, &source);
        Error::ConfigParse {
            path: path.to_path_buf(),
            message,
        }
    })?;

    let mut files = Vec::new();
    normalize_file_group(
        path,
        content,
        context,
        &mut files,
        FileOperationKind::Copy,
        raw.copy,
    )?;
    normalize_file_group(
        path,
        content,
        context,
        &mut files,
        FileOperationKind::Symlink,
        raw.symlink,
    )?;
    normalize_file_group(
        path,
        content,
        context,
        &mut files,
        FileOperationKind::Sync,
        raw.sync,
    )?;
    normalize_mixed_files(path, content, context, &mut files, raw.files)?;
    normalize_file_tables(path, content, context, &mut files, raw.file)?;

    let mut commands = Vec::new();
    normalize_command_entries(path, content, context, &mut commands, raw.commands)?;
    normalize_command_tables(path, content, context, &mut commands, raw.command)?;

    Ok(Config {
        files,
        commands,
        validation: raw.validation.unwrap_or_default().into(),
    })
}

fn normalize_file_group(
    path: &Path,
    content: &str,
    context: &RunContext,
    files: &mut Vec<FileOperation>,
    operation: FileOperationKind,
    entries: Vec<Spanned<RawFileEntry>>,
) -> Result<()> {
    for entry in entries {
        let span = entry_span(content, &entry);
        let entry = entry.into_inner();
        let object = match entry {
            RawFileEntry::Path(source) => RawFileObject {
                operation: None,
                source: Some(source),
                target: None,
                required: false,
                compare: None,
                delete_extra: None,
                symlinks: None,
            },
            RawFileEntry::Object(object) => object,
        };

        if object.operation.is_some() {
            return invalid_config(
                path,
                content,
                span,
                "`operation` is only valid in `files` and `[[file]]` entries",
            );
        }

        files.push(normalize_file_object(
            path, content, context, operation, object, span,
        )?);
    }

    Ok(())
}

fn normalize_mixed_files(
    path: &Path,
    content: &str,
    context: &RunContext,
    files: &mut Vec<FileOperation>,
    entries: Vec<Spanned<RawFileObject>>,
) -> Result<()> {
    for entry in entries {
        let span = entry_span(content, &entry);
        let object = entry.into_inner();
        let operation = required_operation(path, content, span, object.operation)?;
        files.push(normalize_file_object(
            path, content, context, operation, object, span,
        )?);
    }

    Ok(())
}

fn normalize_file_tables(
    path: &Path,
    content: &str,
    context: &RunContext,
    files: &mut Vec<FileOperation>,
    entries: Vec<Spanned<RawFileObject>>,
) -> Result<()> {
    normalize_mixed_files(path, content, context, files, entries)
}

fn normalize_file_object(
    path: &Path,
    content: &str,
    context: &RunContext,
    operation: FileOperationKind,
    object: RawFileObject,
    span: SourceSpan,
) -> Result<FileOperation> {
    let source = object.source.ok_or_else(|| {
        invalid_config_error(
            path,
            content,
            span,
            "file operation is missing required `source`",
        )
    })?;
    let target = object.target.unwrap_or_else(|| source.clone());
    let compare = match operation {
        FileOperationKind::Sync => Some(object.compare.unwrap_or(SyncCompare::Metadata)),
        FileOperationKind::Copy | FileOperationKind::Symlink => {
            reject_non_sync_field(path, content, span, "compare", object.compare)?;
            None
        }
    };
    let delete_extra = match operation {
        FileOperationKind::Sync => Some(object.delete_extra.unwrap_or(true)),
        FileOperationKind::Copy | FileOperationKind::Symlink => {
            reject_non_sync_field(path, content, span, "delete_extra", object.delete_extra)?;
            None
        }
    };
    let symlinks = match operation {
        FileOperationKind::Copy | FileOperationKind::Sync => {
            Some(object.symlinks.unwrap_or(SymlinkMode::Preserve))
        }
        FileOperationKind::Symlink => {
            reject_non_symlink_field(path, content, span, "symlinks", object.symlinks)?;
            None
        }
    };

    Ok(FileOperation {
        operation,
        source_path: resolve_path(&context.root_path, Path::new(&source)),
        target_path: resolve_path(&context.worktree_path, Path::new(&target)),
        source: PathBuf::from(source),
        target: PathBuf::from(target),
        required: object.required,
        compare,
        delete_extra,
        symlinks,
        declaration: span,
    })
}

fn required_operation(
    path: &Path,
    content: &str,
    span: SourceSpan,
    operation: Option<FileOperationKind>,
) -> Result<FileOperationKind> {
    operation.ok_or_else(|| {
        invalid_config_error(
            path,
            content,
            span,
            "file operation is missing required `operation`",
        )
    })
}

fn reject_non_sync_field<T>(
    path: &Path,
    content: &str,
    span: SourceSpan,
    name: &str,
    value: Option<T>,
) -> Result<()> {
    if value.is_some() {
        return invalid_config(
            path,
            content,
            span,
            format!("`{name}` is only valid for sync file operations"),
        );
    }

    Ok(())
}

fn reject_non_symlink_field<T>(
    path: &Path,
    content: &str,
    span: SourceSpan,
    name: &str,
    value: Option<T>,
) -> Result<()> {
    if value.is_some() {
        return invalid_config(
            path,
            content,
            span,
            format!("`{name}` is only valid for copy and sync file operations"),
        );
    }

    Ok(())
}

fn normalize_command_entries(
    path: &Path,
    content: &str,
    context: &RunContext,
    commands: &mut Vec<CommandOperation>,
    entries: Vec<Spanned<RawCommandEntry>>,
) -> Result<()> {
    for entry in entries {
        let span = entry_span(content, &entry);
        let object = match entry.into_inner() {
            RawCommandEntry::Run(run) => RawCommandObject {
                name: None,
                run: Some(run),
                program: None,
                args: None,
                cwd: None,
                env: BTreeMap::new(),
                async_command: false,
                allow_failure: false,
            },
            RawCommandEntry::Object(object) => object,
        };

        commands.push(normalize_command_object(
            path, content, context, object, span,
        )?);
    }

    Ok(())
}

fn normalize_command_tables(
    path: &Path,
    content: &str,
    context: &RunContext,
    commands: &mut Vec<CommandOperation>,
    entries: Vec<Spanned<RawCommandObject>>,
) -> Result<()> {
    for entry in entries {
        let span = entry_span(content, &entry);
        commands.push(normalize_command_object(
            path,
            content,
            context,
            entry.into_inner(),
            span,
        )?);
    }

    Ok(())
}

fn normalize_command_object(
    path: &Path,
    content: &str,
    context: &RunContext,
    object: RawCommandObject,
    span: SourceSpan,
) -> Result<CommandOperation> {
    let command = match (object.run, object.program) {
        (Some(_), Some(_)) => {
            return invalid_config(
                path,
                content,
                span,
                "`run` and `program` are mutually exclusive",
            );
        }
        (Some(_), None) if object.args.is_some() => {
            return invalid_config(path, content, span, "`args` requires `program`");
        }
        (Some(run), None) => CommandKind::Shell { run },
        (None, Some(program)) => CommandKind::Direct {
            program,
            args: object.args.unwrap_or_default(),
        },
        (None, None) => {
            return invalid_config(
                path,
                content,
                span,
                "command is missing required `run` or `program`",
            );
        }
    };
    let cwd_path = object
        .cwd
        .as_ref()
        .map(|cwd| resolve_path(&context.worktree_path, Path::new(cwd)));

    Ok(CommandOperation {
        name: object.name,
        command,
        cwd: object.cwd.map(PathBuf::from),
        cwd_path,
        env: object.env,
        async_command: object.async_command,
        allow_failure: object.allow_failure,
        declaration: span,
    })
}

fn resolve_path(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

fn parse_error_message(content: &str, error: &toml::de::Error) -> String {
    match error.span() {
        Some(span) => format!("{} {}", error.message(), location_suffix(content, &span)),
        None => error.message().to_owned(),
    }
}

fn invalid_config<T>(
    path: &Path,
    content: &str,
    span: SourceSpan,
    message: impl Into<String>,
) -> Result<T> {
    Err(invalid_config_error(path, content, span, message))
}

fn invalid_config_error(
    path: &Path,
    content: &str,
    span: SourceSpan,
    message: impl Into<String>,
) -> Error {
    Error::ConfigInvalid {
        path: path.to_path_buf(),
        message: format!(
            "{} {}",
            message.into(),
            location_suffix(content, &(span.start..span.end))
        ),
    }
}

fn entry_span<T>(content: &str, entry: &Spanned<T>) -> SourceSpan {
    SourceSpan::from_range(content, entry.span())
}

fn location_suffix(content: &str, range: &std::ops::Range<usize>) -> String {
    let span = SourceSpan::from_range(content, range.clone());
    format!("at line {}, column {}", span.line, span.column)
}

impl SourceSpan {
    fn from_range(content: &str, range: std::ops::Range<usize>) -> Self {
        let (line, column) = line_column(content, range.start);

        Self {
            start: range.start,
            end: range.end,
            line,
            column,
        }
    }
}

fn line_column(content: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut column = 1;

    for character in content[..offset.min(content.len())].chars() {
        if character == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }

    (line, column)
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct RawConfig {
    copy: Vec<Spanned<RawFileEntry>>,
    symlink: Vec<Spanned<RawFileEntry>>,
    sync: Vec<Spanned<RawFileEntry>>,
    files: Vec<Spanned<RawFileObject>>,
    file: Vec<Spanned<RawFileObject>>,
    commands: Vec<Spanned<RawCommandEntry>>,
    command: Vec<Spanned<RawCommandObject>>,
    validation: Option<RawValidation>,
}

#[derive(Debug)]
enum RawFileEntry {
    Path(String),
    Object(RawFileObject),
}

impl<'de> Deserialize<'de> for RawFileEntry {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct RawFileEntryVisitor;

        impl<'de> Visitor<'de> for RawFileEntryVisitor {
            type Value = RawFileEntry;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a path string or file operation object")
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(RawFileEntry::Path(value.to_owned()))
            }

            fn visit_string<E>(self, value: String) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(RawFileEntry::Path(value))
            }

            fn visit_map<M>(self, map: M) -> std::result::Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                RawFileObject::deserialize(MapAccessDeserializer::new(map))
                    .map(RawFileEntry::Object)
            }
        }

        deserializer.deserialize_any(RawFileEntryVisitor)
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct RawFileObject {
    operation: Option<FileOperationKind>,
    source: Option<String>,
    target: Option<String>,
    required: bool,
    compare: Option<SyncCompare>,
    delete_extra: Option<bool>,
    symlinks: Option<SymlinkMode>,
}

#[derive(Debug)]
enum RawCommandEntry {
    Run(String),
    Object(RawCommandObject),
}

impl<'de> Deserialize<'de> for RawCommandEntry {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct RawCommandEntryVisitor;

        impl<'de> Visitor<'de> for RawCommandEntryVisitor {
            type Value = RawCommandEntry;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a shell command string or command object")
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(RawCommandEntry::Run(value.to_owned()))
            }

            fn visit_string<E>(self, value: String) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(RawCommandEntry::Run(value))
            }

            fn visit_map<M>(self, map: M) -> std::result::Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                RawCommandObject::deserialize(MapAccessDeserializer::new(map))
                    .map(RawCommandEntry::Object)
            }
        }

        deserializer.deserialize_any(RawCommandEntryVisitor)
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct RawCommandObject {
    name: Option<String>,
    run: Option<String>,
    program: Option<String>,
    args: Option<Vec<String>>,
    cwd: Option<String>,
    env: BTreeMap<String, String>,
    #[serde(rename = "async")]
    async_command: bool,
    allow_failure: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct RawValidation {
    dangerously_allow_sources_outside_root: bool,
    dangerously_allow_targets_outside_worktree: bool,
}

impl From<RawValidation> for ValidationOptions {
    fn from(raw: RawValidation) -> Self {
        Self {
            dangerously_allow_sources_outside_root: raw.dangerously_allow_sources_outside_root,
            dangerously_allow_targets_outside_worktree: raw
                .dangerously_allow_targets_outside_worktree,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;

    fn context() -> RunContext {
        RunContext {
            root_path: PathBuf::from("/repo"),
            worktree_path: PathBuf::from("/repo-worktree"),
            default_branch: "main".to_owned(),
            environment: BTreeMap::from([(
                "TREEBOOT_ROOT_PATH".to_owned(),
                OsString::from("/repo"),
            )]),
        }
    }

    fn parse(content: &str) -> Config {
        parse_config(Path::new(".treeboot.toml"), content, &context()).expect("config should parse")
    }

    fn parse_error(content: &str) -> String {
        parse_config(Path::new(".treeboot.toml"), content, &context())
            .expect_err("config should fail")
            .to_string()
    }

    fn assert_parse_error_contains(content: &str, expected: &str) {
        let error = parse_error(content);

        assert!(
            error.contains(expected),
            "expected error to contain {expected:?}, got {error:?}"
        );
    }

    #[test]
    fn parse_config_should_normalize_file_operations_in_spec_order() {
        let config = parse(
            r#"
sync = ["sync-dir"]
copy = [".env"]
symlink = [{ source = "shared/bin", target = "bin" }]
files = [{ operation = "copy", source = ".npmrc" }]

[[file]]
operation = "sync"
source = "editor"
target = ".editor"
"#,
        );

        let operations = config
            .files
            .iter()
            .map(|operation| (operation.operation, operation.source.as_path()))
            .collect::<Vec<_>>();

        assert_eq!(
            operations,
            vec![
                (FileOperationKind::Copy, Path::new(".env")),
                (FileOperationKind::Symlink, Path::new("shared/bin")),
                (FileOperationKind::Sync, Path::new("sync-dir")),
                (FileOperationKind::Copy, Path::new(".npmrc")),
                (FileOperationKind::Sync, Path::new("editor")),
            ]
        );
    }

    #[test]
    fn parse_config_should_apply_file_defaults() {
        let config = parse(
            r#"
copy = [{ source = ".env.local" }]
sync = ["shared/config"]
"#,
        );

        let copy = &config.files[0];
        let sync = &config.files[1];

        assert_eq!(copy.target, PathBuf::from(".env.local"));
        assert!(!copy.required);
        assert_eq!(copy.symlinks, Some(SymlinkMode::Preserve));
        assert_eq!(sync.compare, Some(SyncCompare::Metadata));
        assert_eq!(sync.delete_extra, Some(true));
    }

    #[test]
    fn parse_config_should_preserve_explicit_sync_options() {
        let config = parse(
            r#"
sync = [{
  source = "shared/config",
  compare = "checksum",
  delete_extra = false,
  symlinks = "preserve",
}]
"#,
        );

        let sync = &config.files[0];

        assert_eq!(sync.compare, Some(SyncCompare::Checksum));
        assert_eq!(sync.delete_extra, Some(false));
        assert_eq!(sync.symlinks, Some(SymlinkMode::Preserve));
    }

    #[test]
    fn parse_config_should_apply_validation_options() {
        let config = parse(
            r#"
[validation]
dangerously_allow_sources_outside_root = true
dangerously_allow_targets_outside_worktree = true
"#,
        );

        assert!(config.validation.dangerously_allow_sources_outside_root);
        assert!(config.validation.dangerously_allow_targets_outside_worktree);
    }

    #[test]
    fn parse_config_should_resolve_absolute_paths_without_rebasing() {
        let config = parse(
            r#"
copy = [{ source = "/shared/.env", target = "/worktree/.env" }]
commands = [{ program = "make", cwd = "/worktree/app" }]
"#,
        );

        assert_eq!(config.files[0].source_path, PathBuf::from("/shared/.env"));
        assert_eq!(config.files[0].target_path, PathBuf::from("/worktree/.env"));
        assert_eq!(
            config.commands[0].cwd_path,
            Some(PathBuf::from("/worktree/app"))
        );
    }

    #[test]
    fn parse_config_should_normalize_command_forms() {
        let config = parse(
            r#"
commands = [
  "mise install",
  { run = "bundle install", async = true },
]

[[command]]
program = "npm"
args = ["install"]
cwd = "web"
allow_failure = true
"#,
        );

        assert_eq!(config.commands.len(), 3);
        assert_eq!(
            config.commands[0].command,
            CommandKind::Shell {
                run: "mise install".to_owned()
            }
        );
        assert!(config.commands[1].async_command);
        assert_eq!(
            config.commands[2].command,
            CommandKind::Direct {
                program: "npm".to_owned(),
                args: vec!["install".to_owned()]
            }
        );
        assert_eq!(
            config.commands[2].cwd_path,
            Some(PathBuf::from("/repo-worktree/web"))
        );
    }

    #[test]
    fn parse_config_should_normalize_command_metadata_and_defaults() {
        let config = parse(
            r#"
commands = [{
  name = "Install",
  program = "npm",
  env = { NODE_ENV = "development" },
}]
"#,
        );

        let command = &config.commands[0];

        assert_eq!(command.name.as_deref(), Some("Install"));
        assert_eq!(command.env["NODE_ENV"], "development");
        assert!(!command.async_command);
        assert!(!command.allow_failure);
    }

    #[test]
    fn parse_config_should_allow_program_without_args() {
        let config = parse(r#"commands = [{ program = "mise" }]"#);

        assert_eq!(
            config.commands[0].command,
            CommandKind::Direct {
                program: "mise".to_owned(),
                args: Vec::new()
            }
        );
    }

    #[test]
    fn parse_config_should_reject_mutually_exclusive_command_fields() {
        assert_parse_error_contains(
            r#"commands = [{ run = "npm install", program = "npm" }]"#,
            "mutually exclusive",
        );
    }

    #[test]
    fn parse_config_should_reject_args_without_program() {
        assert_parse_error_contains(
            r#"commands = [{ run = "npm install", args = [] }]"#,
            "`args` requires `program`",
        );
    }

    #[test]
    fn parse_config_should_reject_missing_command_invocation() {
        assert_parse_error_contains(
            r#"commands = [{ name = "Install" }]"#,
            "missing required `run` or `program`",
        );
    }

    #[test]
    fn parse_config_should_reject_unknown_fields() {
        assert_parse_error_contains(
            r#"copy = [{ source = ".env", unknown = true }]"#,
            "unknown field",
        );
    }

    #[test]
    fn parse_config_should_reject_missing_file_operation() {
        assert_parse_error_contains(
            r#"files = [{ source = ".env" }]"#,
            "missing required `operation`",
        );
    }

    #[test]
    fn parse_config_should_reject_missing_file_source() {
        assert_parse_error_contains(
            r#"copy = [{ target = ".env" }]"#,
            "missing required `source`",
        );
    }

    #[test]
    fn parse_config_should_reject_operation_in_specific_file_groups() {
        assert_parse_error_contains(
            r#"copy = [{ operation = "copy", source = ".env" }]"#,
            "`operation` is only valid in `files` and `[[file]]` entries",
        );
    }

    #[test]
    fn parse_config_should_reject_compare_on_copy_file_operations() {
        assert_parse_error_contains(
            r#"copy = [{ source = ".env", compare = "checksum" }]"#,
            "`compare` is only valid for sync file operations",
        );
    }

    #[test]
    fn parse_config_should_reject_delete_extra_on_symlink_file_operations() {
        assert_parse_error_contains(
            r#"symlink = [{ source = ".env", delete_extra = true }]"#,
            "`delete_extra` is only valid for sync file operations",
        );
    }

    #[test]
    fn parse_config_should_reject_symlinks_on_symlink_file_operations() {
        assert_parse_error_contains(
            r#"symlink = [{ source = ".env", symlinks = "preserve" }]"#,
            "`symlinks` is only valid for copy and sync file operations",
        );
    }

    #[test]
    fn parse_config_should_report_invalid_toml_location() {
        assert_parse_error_contains("commands = [\n", "line 1, column");
    }
}
