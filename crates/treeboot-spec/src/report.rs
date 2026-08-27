use serde::Serialize;

use crate::{CaseMetadata, CommandReport};

/// Serializable result of one conformance case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CaseResult {
    /// Stable case metadata.
    pub case: CaseMetadata,
    /// Elapsed case duration in milliseconds.
    pub duration_ms: u64,
    /// Case outcome and any diagnostic detail.
    pub outcome: CaseOutcome,
}

/// Outcome classification for a conformance case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum CaseOutcome {
    /// The candidate satisfied the case.
    Passed,
    /// The candidate produced observable non-conforming behavior.
    Failed {
        /// Assertion or behavior failure detail.
        details: String,
    },
    /// The host or runner lacks a required capability.
    Skipped {
        /// Explicit platform or capability reason.
        reason: String,
    },
    /// The runner, fixture, or candidate launch failed.
    Error {
        /// Execution or fixture failure detail.
        details: String,
    },
    /// A candidate invocation exceeded its timeout.
    TimedOut {
        /// Timeout detail, including retained partial output when available.
        details: String,
    },
}

impl CaseOutcome {
    /// Returns whether this outcome establishes that the case passed.
    pub const fn is_passed(&self) -> bool {
        matches!(self, Self::Passed)
    }

    /// Returns whether this outcome is an explicit host or capability skip.
    pub const fn is_skipped(&self) -> bool {
        matches!(self, Self::Skipped { .. })
    }
}

/// Serializable report for one suite execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SuiteReport {
    /// `treeboot-spec` package version.
    pub crate_version: &'static str,
    /// Treeboot specification version exercised by the suite.
    pub specification_version: &'static str,
    /// Host platform reported as Rust's operating-system and architecture names.
    pub host_platform: String,
    /// Lossy report projection of the native candidate command.
    pub candidate: CommandReport,
    /// Ordered case results.
    pub cases: Vec<CaseResult>,
}

impl SuiteReport {
    /// Returns true when every executed case passed and all other cases skipped.
    pub fn passed(&self) -> bool {
        self.cases
            .iter()
            .all(|result| result.outcome.is_passed() || result.outcome.is_skipped())
    }

    /// Returns the number of passed cases.
    pub fn passed_count(&self) -> usize {
        self.cases
            .iter()
            .filter(|result| result.outcome.is_passed())
            .count()
    }

    /// Returns the number of skipped cases.
    pub fn skipped_count(&self) -> usize {
        self.cases
            .iter()
            .filter(|result| result.outcome.is_skipped())
            .count()
    }
}
