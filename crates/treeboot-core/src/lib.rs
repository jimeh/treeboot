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
mod file_actions;
mod file_execution;
mod file_operations;
mod file_planning;
mod file_system;
mod git;
mod glob;
mod ignore_rules;
mod init;
mod manual;
mod metadata;
mod output;
mod paths;
mod run;
mod runtime;
mod status;
#[cfg(test)]
mod test_support;
mod validation;

pub use check::{CheckAction, CheckOptions, CheckReport, WorktreeSnapshot, check};
pub use context::{Environment, EnvironmentInput, Worktree, WorktreeOptions};
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
pub use metadata::{
    SPEC_VERSION, TREEBOOT_PACKAGE, TREEBOOT_VERSION, VersionInfo, config_schema_json,
    treeboot_version_info, treeboot_version_summary, version_info,
};
pub use output::{FileOperationSummary, OutputEvent, Reporter};
pub use run::{RunAction, RunOptions, RunReport, run};
pub use runtime::{ResolvedRuntimePolicy, RuntimeOptionOverrides, RuntimePolicy};
pub use status::{
    InitScriptStatus, StatusOptions, StatusReport, StatusSnapshotReport, inspect_status,
    inspect_status_snapshot,
};
pub use validation::{
    ActionPlan, ActionPlanOptions, PlanOrigin, PlannedCommand, PlannedFileOperation,
    PlannedFileStatus,
};

/// Convenient result type used by `treeboot-core`.
pub type Result<T> = std::result::Result<T, Error>;
pub use config::{
    CommandKind, CommandOperation, Config, ConfigOptions, ConfigReport, ConfigRuntimeOptions,
    FileOperation, FileOperationKind, LoadedConfig, MetadataField, SourceSpan, SymlinkMode,
    SyncCompare, inspect_config,
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
