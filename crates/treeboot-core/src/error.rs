use std::path::PathBuf;
use std::process::ExitStatus;

/// Error type for `treeboot-core` operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The current working directory could not be read.
    #[error("failed to read current directory: {source}")]
    CurrentDir {
        /// Source I/O error.
        #[source]
        source: std::io::Error,
    },

    /// A filesystem path could not be normalized.
    #[error("failed to normalize path {path:?}: {source}")]
    NormalizePath {
        /// Path that failed normalization.
        path: PathBuf,
        /// Source I/O error.
        #[source]
        source: std::io::Error,
    },

    /// The command was not run from a Git worktree.
    #[error("not inside a Git worktree")]
    NotGitWorktree,

    /// A Git command could not be spawned.
    #[error("failed to run {command}: {source}")]
    GitIo {
        /// Human-readable command label.
        command: String,
        /// Source I/O error.
        #[source]
        source: std::io::Error,
    },

    /// A Git command exited unsuccessfully.
    #[error("{command} failed: {stderr}")]
    GitFailed {
        /// Human-readable command label.
        command: String,
        /// Standard error from Git.
        stderr: String,
    },

    /// No root checkout path could be determined.
    #[error("could not determine root checkout path")]
    RootPathNotFound,

    /// A specifically requested config file does not exist.
    #[error("config file not found: {0:?}")]
    ConfigNotFound(PathBuf),

    /// A config file could not be read.
    #[error("failed to read config {path:?}: {source}")]
    ConfigIo {
        /// Config file path.
        path: PathBuf,
        /// Source I/O error.
        #[source]
        source: std::io::Error,
    },

    /// A config file contains invalid TOML.
    #[error("invalid config {path:?}: {message}")]
    ConfigParse {
        /// Config file path.
        path: PathBuf,
        /// Parse error message.
        message: String,
    },

    /// A config file contains unsupported or invalid declarations.
    #[error("invalid config {path:?}: {message}")]
    ConfigInvalid {
        /// Config file path.
        path: PathBuf,
        /// Validation error message.
        message: String,
    },

    /// No config was found while strict mode was enabled.
    #[error("no config detected")]
    NoConfigDetectedStrict,

    /// Declarative config execution has not been implemented yet.
    #[error("declarative config execution is not implemented yet: {0:?}")]
    ConfigExecutionNotImplemented(PathBuf),

    /// An init script could not be spawned.
    #[error("failed to run init script {path:?}: {source}")]
    ScriptIo {
        /// Script path.
        path: PathBuf,
        /// Source I/O error.
        #[source]
        source: std::io::Error,
    },

    /// An init script exited unsuccessfully.
    #[error("init script {path:?} failed with {status}")]
    ScriptFailed {
        /// Script path.
        path: PathBuf,
        /// Script process status.
        status: ExitStatus,
    },

    /// Writing output failed.
    #[error("failed to write output: {source}")]
    Output {
        /// Source I/O error.
        #[source]
        source: std::io::Error,
    },

    /// `treeboot init` needs a concrete output type.
    #[error("treeboot init requires --config or --script")]
    InitTypeRequired,

    /// `treeboot init` received conflicting output types.
    #[error("treeboot init cannot use --config and --script together")]
    ConflictingInitTypes,

    /// `treeboot init` refused to replace an existing target.
    #[error("init target already exists: {0:?}")]
    InitTargetExists(PathBuf),

    /// An init file could not be written.
    #[error("failed to write init target {path:?}: {source}")]
    InitIo {
        /// Init target path.
        path: PathBuf,
        /// Source I/O error.
        #[source]
        source: std::io::Error,
    },
}

impl Error {
    /// Returns the CLI exit code associated with this error.
    #[must_use]
    pub const fn exit_code(&self) -> u8 {
        1
    }
}
