//! Core library for `treeboot`.
//!
//! This crate contains the reusable worktree bootstrap logic. The `treeboot`
//! binary crate provides the command-line interface on top of this API.

#![deny(missing_docs)]

mod config;
mod context;
mod discovery;
mod error;
mod git;
mod init;
mod output;
mod run;

pub use context::{Environment, RunContext};
pub use error::Error;
pub use init::{InitKind, InitOptions, InitReport, init};
pub use output::{OutputEvent, Reporter};
pub use run::{RunAction, RunOptions, RunReport, run};

/// Convenient result type used by `treeboot-core`.
pub type Result<T> = std::result::Result<T, Error>;
pub use config::{
    CommandKind, CommandOperation, Config, ConfigOptions, ConfigReport, FileOperation,
    FileOperationKind, SourceSpan, SymlinkMode, SyncCompare, ValidationOptions, inspect_config,
};
