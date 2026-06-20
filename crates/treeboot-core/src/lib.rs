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
mod files;
mod git;
mod init;
mod manual;
mod output;
mod run;
mod validation;

pub use context::{Environment, RunContext};
pub use error::Error;
pub use init::{InitKind, InitOptions, InitReport, init};
pub use manual::{
    FileOperationAction, FileOperationCompletionOptions, FileOperationOptions, FileOperationReport,
    file_operation_source_candidates, run_file_operation,
};
pub use output::{OutputEvent, OutputStream, Reporter};
pub use run::{RunAction, RunOptions, RunReport, run};
pub use validation::{
    PlannedCommand, PlannedFileOperation, PlannedFileStatus, RunPlan, RunPlanOptions,
    plan_run_config,
};

/// Convenient result type used by `treeboot-core`.
pub type Result<T> = std::result::Result<T, Error>;
pub use config::{
    CommandKind, CommandOperation, Config, ConfigOptions, ConfigReport, ConfigRuntimeOptions,
    FileOperation, FileOperationKind, RuntimeOptionOverrides, SourceSpan, SymlinkMode, SyncCompare,
    inspect_config,
};
