//! Treeboot's executable, language-agnostic conformance specification.

#![deny(missing_docs)]

mod case;
mod cases;
mod command;
mod report;
mod runner;
mod suite;

pub use case::{CaseMetadata, CaseRequirement};
pub use command::{CommandReport, CommandTemplate};
pub use report::{CaseOutcome, CaseResult, SuiteReport};
pub use runner::{
    EnvironmentChange, Invocation, InvocationResult, LocalProcessRunner, Runner,
    RunnerCapabilities, RunnerError, StdinMode, Termination,
};
pub use suite::{ConformanceProfile, RunOptions, Suite, SuiteError, SuiteEvent};

/// Treeboot specification version implemented by this crate.
pub const SPEC_VERSION: &str = "2.6.0";

/// Canonical Treeboot specification Markdown.
pub const SPEC_MARKDOWN: &str = include_str!("../SPEC.md");

/// Canonical Treeboot configuration JSON Schema document.
pub const CONFIG_SCHEMA_JSON: &str = include_str!("../assets/treeboot.schema.json");
