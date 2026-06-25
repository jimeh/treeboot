//! Core library for `treeboot`.
//!
//! This crate contains the reusable worktree bootstrap logic. The `treeboot`
//! binary crate provides the command-line interface on top of this API.

#![deny(missing_docs)]

mod check;
mod commands;
mod config;
mod context;
mod discovery;
mod doctor;
mod env;
mod error;
mod executor;
mod files;
mod git;
mod init;
mod manual;
mod metadata;
mod output;
mod run;
mod status;
mod validation;

pub use check::{CheckAction, CheckOptions, CheckReport, WorktreeSnapshot, check};
pub use context::{Environment, Worktree, WorktreeOptions};
pub use discovery::{IgnoredInitScript, InitScriptDiscovery};
pub use doctor::{Diagnostic, DiagnosticStatus, DoctorOptions, DoctorReport, diagnose};
pub use env::{EnvOptions, EnvReport, inspect_env};
pub use error::Error;
pub use executor::{ExecuteOptions, ExecutionReport, Executor};
pub use init::{InitKind, InitOptions, InitReport, init};
pub use manual::{
    FileOperationAction, FileOperationCompletionOptions, FileOperationOptions, FileOperationReport,
    ManualFileOperationOptions, file_operation_source_candidates, run_file_operation,
};
pub use metadata::{SPEC_VERSION, VersionInfo, config_schema_json, version_info};
pub use output::{OutputEvent, Reporter};
pub use run::{RunAction, RunOptions, RunReport, run};
pub use status::{InitScriptStatus, StatusOptions, StatusReport, inspect_status};
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
