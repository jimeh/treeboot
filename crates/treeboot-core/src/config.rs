use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::de::{self, MapAccess, Visitor, value::MapAccessDeserializer};
use serde::{Deserialize, Serialize};
use toml::Spanned;

use crate::context;
use crate::discovery;
use crate::paths::{self, UnsupportedPath};
use crate::{EnvironmentInput, Error, Result, Worktree, WorktreeOptions};

/// Options for inspecting a treeboot config.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigOptions {
    /// Directory from which config discovery starts.
    pub cwd: Option<PathBuf>,
    /// Overrides the root checkout used for resolved source paths.
    pub root: Option<PathBuf>,
    /// Explicit environment input used for compatibility discovery.
    pub environment: EnvironmentInput,
    /// Uses one specific config file instead of discovery.
    pub config: Option<PathBuf>,
}

/// Loaded treeboot config selected for a worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedConfig {
    /// Runtime context used while resolving config paths.
    pub context: Worktree,
    /// Config file path.
    pub path: PathBuf,
    /// Parsed and normalized config.
    pub config: Config,
}

/// Result summary for a `treeboot config` invocation.
pub type ConfigReport = LoadedConfig;

/// Parsed and normalized treeboot config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Config {
    /// Runtime options declared by the config.
    #[serde(flatten)]
    pub options: ConfigRuntimeOptions,
    /// Ordered file operations.
    pub files: Vec<FileOperation>,
    /// Ordered command operations.
    pub commands: Vec<CommandOperation>,
}

impl Config {
    /// Loads and parses a treeboot config from disk.
    ///
    /// Relative paths inside the config are normalized against the supplied
    /// worktree context.
    ///
    /// # Errors
    ///
    /// Returns an error if the config cannot be read or TOML parsing and
    /// normalization fails.
    pub fn load(path: &Path, context: &Worktree) -> Result<Self> {
        let content = std::fs::read_to_string(path).map_err(|source| Error::ConfigIo {
            path: path.to_path_buf(),
            source,
        })?;

        Self::parse(path, &content, context)
    }

    /// Parses a treeboot config string.
    ///
    /// The path is used for diagnostics. Relative paths inside the config are
    /// normalized against the supplied worktree context.
    ///
    /// # Errors
    ///
    /// Returns an error if TOML parsing or normalization fails.
    pub fn parse(path: &Path, content: &str, context: &Worktree) -> Result<Self> {
        parse_config(path, content, context)
    }

    /// Discovers the selected treeboot config path for a worktree.
    ///
    /// When `requested_config` is provided, it is resolved relative to the
    /// worktree path and must exist. When omitted, standard treeboot config
    /// paths are searched in precedence order.
    ///
    /// # Errors
    ///
    /// Returns an error when a requested config path does not exist.
    pub fn discover_path(
        context: &Worktree,
        requested_config: Option<&Path>,
    ) -> Result<Option<PathBuf>> {
        discovery::discover_config(&context.worktree_path, requested_config)
    }

    /// Discovers, loads, and parses the selected treeboot config.
    ///
    /// Returns `Ok(None)` when no config was requested and no standard config
    /// path exists.
    ///
    /// # Errors
    ///
    /// Returns an error if a requested config path does not exist, the selected
    /// config cannot be read, or TOML parsing and normalization fails.
    pub fn load_discovered(
        context: &Worktree,
        requested_config: Option<&Path>,
    ) -> Result<Option<LoadedConfig>> {
        let Some(path) = Self::discover_path(context, requested_config)? else {
            return Ok(None);
        };
        let config = Self::load(&path, context)?;

        Ok(Some(LoadedConfig {
            context: context.clone(),
            path,
            config,
        }))
    }
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
    /// Whether the source is treated as a glob pattern.
    pub glob: bool,
    /// Sync comparison mode.
    pub compare: Option<SyncCompare>,
    /// Whether sync should delete target-only files.
    pub delete: Option<bool>,
    /// How copy and sync should treat source symlinks.
    pub symlinks: Option<SymlinkMode>,
    /// Source-relative path patterns ignored by copy and sync.
    pub ignore: Vec<String>,
    /// Metadata fields ignored by copy and sync.
    pub ignore_metadata: Vec<MetadataField>,
    /// Pattern-base-relative prefix applied to operation-relative paths
    /// during ignore matching. Non-empty only for operations expanded from a
    /// glob source pattern, where ignore rules stay anchored at the pattern
    /// base instead of the expanded operation's own source.
    #[serde(skip)]
    pub ignore_prefix: PathBuf,
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

impl FileOperationKind {
    /// Returns the stable lowercase operation name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Copy => "copy",
            Self::Symlink => "symlink",
            Self::Sync => "sync",
        }
    }
}

impl fmt::Display for FileOperationKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
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

/// Metadata field ignored by copy and sync operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetadataField {
    /// Ignore file and directory permissions.
    Permissions,
    /// Ignore file and directory owner.
    Owner,
    /// Ignore file and directory group.
    Group,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RawMetadataField {
    Permissions,
    Owner,
    Group,
    Ownership,
}

impl RawMetadataField {
    const fn expanded(self) -> &'static [MetadataField] {
        match self {
            Self::Permissions => &[MetadataField::Permissions],
            Self::Owner => &[MetadataField::Owner],
            Self::Group => &[MetadataField::Group],
            Self::Ownership => &[MetadataField::Owner, MetadataField::Group],
        }
    }
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

/// Runtime options declared by a config file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ConfigRuntimeOptions {
    /// Enables strict declarative validation and conflict handling.
    pub strict: bool,
    /// Default path ignore patterns prepended to copy and sync operations.
    pub default_ignore: Vec<String>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileOperationSettingsInput {
    pub(crate) compare: Option<SyncCompare>,
    pub(crate) delete: Option<bool>,
    pub(crate) symlinks: Option<SymlinkMode>,
    pub(crate) ignore: Vec<String>,
    pub(crate) ignore_metadata: Vec<RawMetadataField>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileOperationSettings {
    pub(crate) compare: Option<SyncCompare>,
    pub(crate) delete: Option<bool>,
    pub(crate) symlinks: Option<SymlinkMode>,
    pub(crate) ignore: Vec<String>,
    pub(crate) ignore_metadata: Vec<MetadataField>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InvalidFileOperationField {
    Compare,
    Delete,
    Symlinks,
    Ignore,
    IgnoreMetadata,
}

impl InvalidFileOperationField {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Compare => "compare",
            Self::Delete => "delete",
            Self::Symlinks => "symlinks",
            Self::Ignore => "ignore",
            Self::IgnoreMetadata => "ignore_metadata",
        }
    }

    pub(crate) const fn allowed_operations(self) -> &'static str {
        match self {
            Self::Compare | Self::Delete => "sync",
            Self::Symlinks | Self::Ignore | Self::IgnoreMetadata => "copy and sync",
        }
    }
}

pub(crate) fn normalize_file_operation_settings(
    operation: FileOperationKind,
    input: FileOperationSettingsInput,
) -> std::result::Result<FileOperationSettings, InvalidFileOperationField> {
    let compare = match operation {
        FileOperationKind::Sync => Some(input.compare.unwrap_or(SyncCompare::Metadata)),
        FileOperationKind::Copy | FileOperationKind::Symlink => {
            if input.compare.is_some() {
                return Err(InvalidFileOperationField::Compare);
            }
            None
        }
    };
    let delete = match operation {
        FileOperationKind::Sync => Some(input.delete.unwrap_or(false)),
        FileOperationKind::Copy | FileOperationKind::Symlink => {
            if input.delete.is_some() {
                return Err(InvalidFileOperationField::Delete);
            }
            None
        }
    };
    let symlinks = match operation {
        FileOperationKind::Copy | FileOperationKind::Sync => {
            Some(input.symlinks.unwrap_or(SymlinkMode::Preserve))
        }
        FileOperationKind::Symlink => {
            if input.symlinks.is_some() {
                return Err(InvalidFileOperationField::Symlinks);
            }
            None
        }
    };
    let ignore = match operation {
        FileOperationKind::Copy | FileOperationKind::Sync => input.ignore,
        FileOperationKind::Symlink => {
            if !input.ignore.is_empty() {
                return Err(InvalidFileOperationField::Ignore);
            }
            Vec::new()
        }
    };
    let ignore_metadata = match operation {
        FileOperationKind::Copy | FileOperationKind::Sync => {
            normalize_ignored_metadata(input.ignore_metadata)
        }
        FileOperationKind::Symlink => {
            if !input.ignore_metadata.is_empty() {
                return Err(InvalidFileOperationField::IgnoreMetadata);
            }
            Vec::new()
        }
    };

    Ok(FileOperationSettings {
        compare,
        delete,
        symlinks,
        ignore,
        ignore_metadata,
    })
}

pub(crate) fn effective_ignore_patterns(
    operation: FileOperationKind,
    default_ignore: &[String],
    ignore: Vec<String>,
) -> Vec<String> {
    match operation {
        FileOperationKind::Copy | FileOperationKind::Sync => {
            let mut effective = Vec::with_capacity(default_ignore.len() + ignore.len());
            effective.extend(default_ignore.iter().cloned());
            effective.extend(ignore);
            effective
        }
        FileOperationKind::Symlink => ignore,
    }
}

pub(crate) fn normalize_ignored_metadata(fields: Vec<RawMetadataField>) -> Vec<MetadataField> {
    let mut normalized = Vec::new();
    for field in fields {
        for expanded in field.expanded() {
            if !normalized.contains(expanded) {
                normalized.push(*expanded);
            }
        }
    }
    normalized
}

/// Parses, normalizes, and returns the selected config file.
///
/// # Errors
///
/// Returns an error if context discovery fails, no config exists, the requested
/// config path does not exist, the config cannot be read, or TOML parsing and
/// normalization fails.
pub fn inspect_config(options: ConfigOptions) -> Result<ConfigReport> {
    let worktree_options = WorktreeOptions {
        cwd: options.cwd,
        root: options.root,
        environment: options.environment,
    };
    let context = context::resolve(&worktree_options)?;
    Config::load_discovered(&context, options.config.as_deref())?
        .ok_or(Error::NoConfigDetectedStrict)
}

fn parse_config(path: &Path, content: &str, context: &Worktree) -> Result<Config> {
    let raw: RawConfig = toml::from_str(content).map_err(|source| {
        let message = parse_error_message(content, &source);
        Error::ConfigParse {
            path: path.to_path_buf(),
            message,
        }
    })?;

    let default_ignore = raw.default_ignore;
    let mut files = Vec::new();
    normalize_file_group(
        path,
        content,
        context,
        &mut files,
        FileOperationKind::Copy,
        raw.copy,
        &default_ignore,
    )?;
    normalize_file_group(
        path,
        content,
        context,
        &mut files,
        FileOperationKind::Symlink,
        raw.symlink,
        &default_ignore,
    )?;
    normalize_file_group(
        path,
        content,
        context,
        &mut files,
        FileOperationKind::Sync,
        raw.sync,
        &default_ignore,
    )?;
    normalize_mixed_files(
        path,
        content,
        context,
        &mut files,
        raw.files,
        &default_ignore,
    )?;
    normalize_file_tables(
        path,
        content,
        context,
        &mut files,
        raw.file,
        &default_ignore,
    )?;

    let mut commands = Vec::new();
    normalize_command_entries(path, content, context, &mut commands, raw.commands)?;
    normalize_command_tables(path, content, context, &mut commands, raw.command)?;

    Ok(Config {
        options: ConfigRuntimeOptions {
            strict: raw.strict,
            default_ignore,
            dangerously_allow_sources_outside_root: raw.dangerously_allow_sources_outside_root,
            dangerously_allow_targets_outside_worktree: raw
                .dangerously_allow_targets_outside_worktree,
        },
        files,
        commands,
    })
}

fn normalize_file_group(
    path: &Path,
    content: &str,
    context: &Worktree,
    files: &mut Vec<FileOperation>,
    operation: FileOperationKind,
    entries: Vec<Spanned<RawFileEntry>>,
    default_ignore: &[String],
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
                glob: None,
                compare: None,
                delete: None,
                symlinks: None,
                ignore: Vec::new(),
                ignore_metadata: Vec::new(),
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
            path,
            content,
            context,
            operation,
            object,
            span,
            default_ignore,
        )?);
    }

    Ok(())
}

fn normalize_mixed_files(
    path: &Path,
    content: &str,
    context: &Worktree,
    files: &mut Vec<FileOperation>,
    entries: Vec<Spanned<RawFileObject>>,
    default_ignore: &[String],
) -> Result<()> {
    for entry in entries {
        let span = entry_span(content, &entry);
        let object = entry.into_inner();
        let operation = required_operation(path, content, span, object.operation)?;
        files.push(normalize_file_object(
            path,
            content,
            context,
            operation,
            object,
            span,
            default_ignore,
        )?);
    }

    Ok(())
}

fn normalize_file_tables(
    path: &Path,
    content: &str,
    context: &Worktree,
    files: &mut Vec<FileOperation>,
    entries: Vec<Spanned<RawFileObject>>,
    default_ignore: &[String],
) -> Result<()> {
    normalize_mixed_files(path, content, context, files, entries, default_ignore)
}

fn normalize_file_object(
    path: &Path,
    content: &str,
    context: &Worktree,
    operation: FileOperationKind,
    object: RawFileObject,
    span: SourceSpan,
    default_ignore: &[String],
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
    let settings = normalize_file_operation_settings(
        operation,
        FileOperationSettingsInput {
            compare: object.compare,
            delete: object.delete,
            symlinks: object.symlinks,
            ignore: object.ignore,
            ignore_metadata: object.ignore_metadata,
        },
    )
    .map_err(|field| {
        invalid_config_error(
            path,
            content,
            span,
            format!(
                "`{}` is only valid for {} file operations",
                field.name(),
                field.allowed_operations()
            ),
        )
    })?;

    let glob = object.glob.unwrap_or(true) && crate::glob::is_glob_source(Path::new(&source));

    Ok(FileOperation {
        operation,
        source_path: resolve_path(path, content, span, &context.root_path, Path::new(&source))?,
        target_path: resolve_target_path(
            path,
            content,
            span,
            &context.worktree_path,
            Path::new(&target),
        )?,
        source: PathBuf::from(source),
        target: PathBuf::from(target),
        required: object.required,
        glob,
        compare: settings.compare,
        delete: settings.delete,
        symlinks: settings.symlinks,
        ignore: effective_ignore_patterns(operation, default_ignore, settings.ignore),
        ignore_metadata: settings.ignore_metadata,
        ignore_prefix: PathBuf::new(),
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

fn normalize_command_entries(
    path: &Path,
    content: &str,
    context: &Worktree,
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
    context: &Worktree,
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
    context: &Worktree,
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
        .map(|cwd| resolve_path(path, content, span, &context.worktree_path, Path::new(cwd)))
        .transpose()?;

    Ok(CommandOperation {
        name: object.name,
        command,
        cwd: object.cwd.map(PathBuf::from),
        cwd_path,
        env: object.env,
        allow_failure: object.allow_failure,
        declaration: span,
    })
}

fn resolve_path(
    config_path: &Path,
    content: &str,
    span: SourceSpan,
    base: &Path,
    path: &Path,
) -> Result<PathBuf> {
    let resolved = paths::resolve_path(base, path).map_err(|source| {
        invalid_config_error(
            config_path,
            content,
            span,
            unsupported_path_message(path, source),
        )
    })?;

    paths::normalize_maybe_existing(&resolved).map_err(|source| {
        invalid_config_error(
            config_path,
            content,
            span,
            normalize_path_message(path, source),
        )
    })
}

fn resolve_target_path(
    config_path: &Path,
    content: &str,
    span: SourceSpan,
    base: &Path,
    path: &Path,
) -> Result<PathBuf> {
    let resolved = paths::resolve_path(base, path).map_err(|source| {
        invalid_config_error(
            config_path,
            content,
            span,
            unsupported_path_message(path, source),
        )
    })?;

    let Some(name) = resolved.file_name() else {
        return paths::normalize_maybe_existing(&resolved).map_err(|source| {
            invalid_config_error(
                config_path,
                content,
                span,
                normalize_path_message(path, source),
            )
        });
    };
    let parent = resolved.parent().unwrap_or_else(|| Path::new("."));
    let mut normalized = paths::normalize_maybe_existing(parent).map_err(|source| {
        invalid_config_error(
            config_path,
            content,
            span,
            normalize_path_message(path, source),
        )
    })?;
    normalized.push(name);

    Ok(paths::normalize_lexical(&normalized))
}

fn unsupported_path_message(path: &Path, source: UnsupportedPath) -> String {
    format!("unsupported path `{}`: {}", path.display(), source.reason())
}

fn normalize_path_message(path: &Path, source: std::io::Error) -> String {
    format!("failed to normalize path `{}`: {}", path.display(), source)
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
    strict: bool,
    default_ignore: Vec<String>,
    dangerously_allow_sources_outside_root: bool,
    dangerously_allow_targets_outside_worktree: bool,
    copy: Vec<Spanned<RawFileEntry>>,
    symlink: Vec<Spanned<RawFileEntry>>,
    sync: Vec<Spanned<RawFileEntry>>,
    files: Vec<Spanned<RawFileObject>>,
    file: Vec<Spanned<RawFileObject>>,
    commands: Vec<Spanned<RawCommandEntry>>,
    command: Vec<Spanned<RawCommandObject>>,
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
    glob: Option<bool>,
    compare: Option<SyncCompare>,
    delete: Option<bool>,
    symlinks: Option<SymlinkMode>,
    ignore: Vec<String>,
    ignore_metadata: Vec<RawMetadataField>,
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
    allow_failure: bool,
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use crate::test_support::symlink_dir;

    use super::*;

    fn context() -> Worktree {
        Worktree {
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

    fn toml_basic_string_path(path: &Path) -> String {
        path.display()
            .to_string()
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
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
        assert!(copy.ignore.is_empty());
        assert!(copy.ignore_metadata.is_empty());
        assert_eq!(sync.compare, Some(SyncCompare::Metadata));
        assert_eq!(sync.delete, Some(false));
        assert!(sync.ignore.is_empty());
        assert!(sync.ignore_metadata.is_empty());
    }

    #[test]
    fn parse_config_should_preserve_explicit_sync_options() {
        let config = parse(
            r#"
sync = [{
  source = "shared/config",
  compare = "checksum",
  delete = true,
  symlinks = "preserve",
}]
"#,
        );

        let sync = &config.files[0];

        assert_eq!(sync.compare, Some(SyncCompare::Checksum));
        assert_eq!(sync.delete, Some(true));
        assert_eq!(sync.symlinks, Some(SymlinkMode::Preserve));
    }

    #[test]
    fn parse_config_should_normalize_ignored_metadata() {
        let config = parse(
            r#"
copy = [{ source = ".env", ignore_metadata = ["ownership", "permissions", "owner"] }]
sync = [{ source = "shared", ignore_metadata = ["group"] }]
"#,
        );

        assert_eq!(
            config.files[0].ignore_metadata,
            vec![
                MetadataField::Owner,
                MetadataField::Group,
                MetadataField::Permissions,
            ]
        );
        assert_eq!(config.files[1].ignore_metadata, vec![MetadataField::Group]);
    }

    #[test]
    fn parse_config_should_preserve_explicit_ignore_patterns() {
        let config = parse(
            r#"
copy = [{ source = ".env", ignore = ["**/vendor/**", "!**/vendor/keep/**"] }]
sync = [{ source = "shared", ignore = ["cache/", "!cache/keep"] }]
"#,
        );

        assert_eq!(
            config.files[0].ignore,
            vec!["**/vendor/**", "!**/vendor/keep/**"]
        );
        assert_eq!(config.files[1].ignore, vec!["cache/", "!cache/keep"]);
    }

    #[test]
    fn parse_config_should_prepend_default_ignore_to_copy_and_sync() {
        let config = parse(
            r#"
default_ignore = [".DS_Store", "Thumbs.db"]
copy = [{ source = ".env", ignore = ["!.DS_Store"] }]
sync = [{ source = "shared", ignore = ["cache/"] }]
"#,
        );

        assert_eq!(
            config.options.default_ignore,
            vec![".DS_Store", "Thumbs.db"]
        );
        assert_eq!(
            config.files[0].ignore,
            vec![".DS_Store", "Thumbs.db", "!.DS_Store"]
        );
        assert_eq!(
            config.files[1].ignore,
            vec![".DS_Store", "Thumbs.db", "cache/"]
        );
    }

    #[test]
    fn parse_config_should_prepend_default_ignore_to_mixed_file_entries() {
        let config = parse(
            r#"
default_ignore = [".DS_Store"]
files = [
  { operation = "copy", source = ".env", ignore = ["!.DS_Store"] },
  { operation = "symlink", source = "bin" },
]

[[file]]
operation = "sync"
source = "shared"
ignore = ["cache/"]
"#,
        );

        assert_eq!(config.files[0].ignore, vec![".DS_Store", "!.DS_Store"]);
        assert!(config.files[1].ignore.is_empty());
        assert_eq!(config.files[2].ignore, vec![".DS_Store", "cache/"]);
    }

    #[test]
    fn parse_config_should_not_apply_default_ignore_to_symlink() {
        let config = parse(
            r#"
default_ignore = [".DS_Store"]
symlink = ["shared/bin"]
"#,
        );

        assert!(config.files[0].ignore.is_empty());
    }

    #[test]
    fn parse_config_should_reject_ignore_on_symlink_file_operations() {
        assert_parse_error_contains(
            r#"
symlink = [{ source = "link", ignore = ["**/tmp/**"] }]
"#,
            "`ignore` is only valid for copy and sync",
        );
    }

    #[test]
    fn parse_config_should_reject_ignored_metadata_on_symlink_file_operations() {
        assert_parse_error_contains(
            r#"
symlink = [{ source = "link", ignore_metadata = ["ownership"] }]
"#,
            "`ignore_metadata` is only valid for copy and sync",
        );
    }

    #[test]
    fn parse_config_should_apply_runtime_options() {
        let config = parse(
            r#"
strict = true
default_ignore = [".DS_Store"]
dangerously_allow_sources_outside_root = true
dangerously_allow_targets_outside_worktree = true
"#,
        );

        assert!(config.options.strict);
        assert_eq!(config.options.default_ignore, vec![".DS_Store"]);
        assert!(config.options.dangerously_allow_sources_outside_root);
        assert!(config.options.dangerously_allow_targets_outside_worktree);
    }

    #[test]
    fn parse_config_should_reject_nested_validation_options() {
        assert_parse_error_contains(
            r#"
[validation]
dangerously_allow_sources_outside_root = true
"#,
            "unknown field",
        );
    }

    #[test]
    fn parse_config_should_resolve_absolute_paths_without_rebasing() {
        let temp = std::env::temp_dir().join("treeboot-config-absolute-paths");
        let source = temp.join("shared").join("..").join(".env");
        let target = temp.join("worktree").join("..").join("worktree/.env");
        let cwd = temp.join("worktree").join("..").join("worktree/app");
        let config = parse(&format!(
            r#"
copy = [{{ source = "{}", target = "{}" }}]
commands = [{{ program = "make", cwd = "{}" }}]
"#,
            toml_basic_string_path(&source),
            toml_basic_string_path(&target),
            toml_basic_string_path(&cwd),
        ));

        assert_eq!(
            config.files[0].source_path,
            paths::normalize_maybe_existing(&temp.join(".env")).expect("source should normalize")
        );
        assert_eq!(
            config.files[0].target_path,
            paths::normalize_maybe_existing(&temp.join("worktree/.env"))
                .expect("target should normalize")
        );
        assert_eq!(
            config.commands[0].cwd_path,
            Some(
                paths::normalize_maybe_existing(&temp.join("worktree/app"))
                    .expect("cwd should normalize")
            )
        );
    }

    #[test]
    fn parse_config_should_normalize_relative_paths_through_existing_aliases() {
        let temp = tempfile::TempDir::new().expect("tempdir should be created");
        let actual = temp.path().join("actual");
        let root = actual.join("root");
        let worktree = actual.join("worktree");
        let alias = temp.path().join("alias");
        let alias_root = alias.join("root");
        let alias_worktree = alias.join("worktree");
        std::fs::create_dir_all(root.join("shared")).expect("root source dir should be created");
        std::fs::create_dir_all(worktree.join("app")).expect("worktree app dir should be created");
        std::fs::create_dir_all(&alias).expect("alias dir should be created");
        symlink_dir(&root, &alias_root).expect("root alias should be created");
        symlink_dir(&worktree, &alias_worktree).expect("worktree alias should be created");

        let context = Worktree {
            root_path: alias_root,
            worktree_path: alias_worktree,
            default_branch: "main".to_owned(),
            environment: BTreeMap::new(),
        };
        let config = parse_config(
            Path::new(".treeboot.toml"),
            r#"
copy = [{ source = "shared/.env", target = ".env" }]
commands = [{ program = "make", cwd = "app" }]
"#,
            &context,
        )
        .expect("config should parse");

        assert_eq!(
            config.files[0].source_path,
            paths::normalize_maybe_existing(&root.join("shared/.env"))
                .expect("source should normalize through alias")
        );
        assert_eq!(
            config.files[0].target_path,
            paths::normalize_maybe_existing(&worktree.join(".env"))
                .expect("target should normalize through alias")
        );
        assert_eq!(
            config.commands[0].cwd_path,
            Some(
                paths::normalize_maybe_existing(&worktree.join("app"))
                    .expect("cwd should normalize through alias")
            )
        );
    }

    #[cfg(windows)]
    #[test]
    fn parse_config_should_reject_drive_relative_windows_paths() {
        assert_parse_error_contains(
            r#"copy = [{ source = 'C:shared/.env' }]"#,
            "drive-relative paths are not supported",
        );
    }

    #[cfg(windows)]
    #[test]
    fn parse_config_should_reject_root_relative_windows_paths() {
        assert_parse_error_contains(
            r#"commands = [{ program = "git", cwd = '\app' }]"#,
            "root-relative paths without a drive or share are not supported",
        );
    }

    #[test]
    fn parse_config_should_normalize_command_forms() {
        let config = parse(
            r#"
commands = [
  "mise install",
  { run = "bundle install" },
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
        assert_eq!(
            config.commands[2].command,
            CommandKind::Direct {
                program: "npm".to_owned(),
                args: vec!["install".to_owned()]
            }
        );
        assert_eq!(
            config.commands[2].cwd_path,
            Some(
                paths::normalize_maybe_existing(&context().worktree_path.join("web"))
                    .expect("expected cwd should normalize")
            )
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
        assert!(!command.allow_failure);
    }

    #[test]
    fn parse_config_should_reject_async_command_field() {
        assert_parse_error_contains(
            r#"commands = [{ run = "npm install", async = true }]"#,
            "unknown field",
        );
        assert_parse_error_contains(
            r#"commands = [{ run = "npm install", async = false }]"#,
            "unknown field",
        );
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
    fn parse_config_should_reject_delete_on_symlink_file_operations() {
        assert_parse_error_contains(
            r#"symlink = [{ source = ".env", delete = true }]"#,
            "`delete` is only valid for sync file operations",
        );
    }

    #[test]
    fn parse_config_should_reject_legacy_delete_extra_field() {
        assert_parse_error_contains(
            r#"sync = [{ source = "shared", delete_extra = true }]"#,
            "unknown field `delete_extra`",
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

    #[test]
    fn parse_config_should_detect_glob_sources() {
        let config = parse(
            r#"
copy = [
  "certs/*.pem",
  { source = "config/?" },
  { source = "config/[ab]", glob = false },
  ".env",
]
"#,
        );

        let globs = config
            .files
            .iter()
            .map(|operation| operation.glob)
            .collect::<Vec<_>>();

        assert_eq!(globs, vec![true, true, false, false]);
    }
}
