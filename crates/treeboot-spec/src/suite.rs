use std::any::Any;
use std::sync::Arc;
use std::time::{Duration, Instant};

use thiserror::Error;

use crate::cases::support::{CaseContext, ExecutionFailure, install_case_panic_hook, with_context};
use crate::{
    CaseMetadata, CaseOutcome, CaseResult, CommandTemplate, LocalProcessRunner, Runner, SuiteReport,
};

/// Options controlling one suite execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOptions {
    /// Optional substring matched against stable case identifiers.
    pub filter: Option<String>,
    /// Default timeout applied to each candidate invocation.
    pub invocation_timeout: Duration,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            filter: None,
            invocation_timeout: Duration::from_secs(30),
        }
    }
}

/// The case registry for one specification version.
#[derive(Debug, Clone, Copy, Default)]
pub struct Suite;

impl Suite {
    /// Returns the registry for the specification embedded in this crate.
    pub const fn current() -> Self {
        Self
    }

    /// Returns ordered metadata for every conformance case.
    pub fn cases(self) -> impl Iterator<Item = CaseMetadata> {
        crate::cases::definitions().map(|definition| definition.metadata)
    }

    /// Runs the suite using the default local-process runner.
    ///
    /// The command is resolved before any case changes its working directory.
    ///
    /// # Errors
    ///
    /// Returns [`SuiteError`] when a path-like candidate command cannot be
    /// resolved before execution.
    pub fn run(
        self,
        command: &CommandTemplate,
        options: RunOptions,
    ) -> Result<SuiteReport, SuiteError> {
        let command = command
            .resolve()
            .map_err(|source| SuiteError::ResolveCandidate { source })?;
        Ok(self.run_with(Arc::new(LocalProcessRunner::new(command)), options))
    }

    /// Runs the same registry through a custom execution adapter.
    ///
    /// The adapter must make the candidate see fixture files and honor each
    /// invocation's native arguments, working directory, environment changes,
    /// input mode, and timeout.
    pub fn run_with(self, runner: Arc<dyn Runner>, options: RunOptions) -> SuiteReport {
        install_case_panic_hook();
        let mut results = Vec::new();
        for definition in crate::cases::definitions() {
            if options
                .filter
                .as_ref()
                .is_some_and(|filter| !definition.metadata.id().contains(filter))
            {
                continue;
            }

            if let Some(reason) = definition.skip_reason {
                results.push(CaseResult {
                    case: definition.metadata,
                    duration_ms: 0,
                    outcome: CaseOutcome::Skipped {
                        reason: reason.to_owned(),
                    },
                });
                continue;
            }

            let context = Arc::new(CaseContext::new(
                Arc::clone(&runner),
                options.invocation_timeout,
            ));
            let started = Instant::now();
            let Some(run) = definition.run else {
                results.push(CaseResult {
                    case: definition.metadata,
                    duration_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
                    outcome: malformed_definition_outcome(definition.metadata),
                });
                continue;
            };
            let execution = std::panic::catch_unwind(std::panic::AssertUnwindSafe({
                let context = Arc::clone(&context);
                move || with_context(context, run)
            }));
            let recorded = context.take_failure();
            let outcome = match (execution, recorded) {
                (Ok(()), None) => CaseOutcome::Passed,
                (_, Some(ExecutionFailure::Skipped(reason))) => CaseOutcome::Skipped { reason },
                (_, Some(ExecutionFailure::Runner(details))) => CaseOutcome::Error { details },
                (_, Some(ExecutionFailure::TimedOut(details))) => CaseOutcome::TimedOut { details },
                (Err(payload), None) if !context.candidate_invoked() => CaseOutcome::Error {
                    details: format!("fixture setup failed: {}", panic_message(payload)),
                },
                (Err(payload), None) => CaseOutcome::Failed {
                    details: panic_message(payload),
                },
            };
            results.push(CaseResult {
                case: definition.metadata,
                duration_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
                outcome,
            });
        }

        SuiteReport {
            crate_version: env!("CARGO_PKG_VERSION"),
            specification_version: crate::SPEC_VERSION,
            host_platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            candidate: runner.command().report(),
            cases: results,
        }
    }
}

fn malformed_definition_outcome(metadata: CaseMetadata) -> CaseOutcome {
    CaseOutcome::Error {
        details: format!(
            "malformed conformance registry: case '{}' has neither a function nor a skip reason",
            metadata.id()
        ),
    }
}

fn panic_message(payload: Box<dyn Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    if let Some(message) = payload.downcast_ref::<&str>() {
        return (*message).to_owned();
    }
    "conformance assertion panicked without a message".to_owned()
}

/// Suite setup failure that occurs before any case executes.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SuiteError {
    /// A path-like candidate could not be resolved before fixture execution.
    #[error("failed to resolve candidate command: {source}")]
    ResolveCandidate {
        /// Candidate path resolution failure.
        #[source]
        source: std::io::Error,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_definition_should_be_an_infrastructure_error() {
        let metadata = CaseMetadata::closure("malformed.case", &["#test"]);

        let outcome = malformed_definition_outcome(metadata);

        assert!(matches!(outcome, CaseOutcome::Error { .. }));
    }
}
