//! Core library for `treeboot`.
//!
//! This crate contains the reusable worktree bootstrap logic. The `treeboot`
//! binary crate provides the command-line interface on top of this API.

#![deny(missing_docs)]

mod commands;
mod config;
mod context;
mod discovery;
mod error;
mod executor;
mod files;
mod git;
mod init;
mod manual;
mod output;
mod run;
mod validation;

pub use context::{Environment, Worktree, WorktreeOptions};
pub use discovery::InitScriptDiscovery;
pub use error::Error;
pub use executor::{ExecuteOptions, ExecutionReport, Executor};
pub use init::{InitKind, InitOptions, InitReport, init};
pub use manual::{
    FileOperationAction, FileOperationCompletionOptions, FileOperationOptions, FileOperationReport,
    ManualFileOperationOptions, file_operation_source_candidates, run_file_operation,
};
pub use output::{OutputEvent, Reporter};
pub use run::{RunAction, RunOptions, RunReport, run};
pub use validation::{
    ActionPlan, ActionPlanOptions, PlanOrigin, PlannedCommand, PlannedFileOperation,
    PlannedFileStatus,
};

/// Convenient result type used by `treeboot-core`.
pub type Result<T> = std::result::Result<T, Error>;
pub use config::{
    CommandKind, CommandOperation, Config, ConfigOptions, ConfigReport, ConfigRuntimeOptions,
    FileOperation, FileOperationKind, LoadedConfig, RuntimeOptionOverrides, SourceSpan,
    SymlinkMode, SyncCompare, inspect_config,
};

/// Parsed treeboot manifest.
pub type Manifest = Config;

/// Options for inspecting a treeboot manifest.
pub type ManifestOptions = ConfigOptions;

/// Result summary for manifest inspection.
pub type ManifestReport = LoadedConfig;

/// Raw file operation intent used to build an action plan.
pub type FileOperationSpec = FileOperation;

/// Raw command intent used to build an action plan.
pub type CommandSpec = CommandOperation;

/// Resolved runtime policy from defaults, config, environment, and CLI flags.
pub type RuntimePolicy = ConfigRuntimeOptions;
