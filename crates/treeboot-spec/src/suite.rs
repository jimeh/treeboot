use std::any::Any;
use std::sync::Arc;
use std::time::{Duration, Instant};

use thiserror::Error;

use crate::cases::support::{CaseContext, ExecutionFailure, install_case_panic_hook, with_context};
use crate::{
    CaseMetadata, CaseOutcome, CaseRequirement, CaseResult, CommandTemplate, LocalProcessRunner,
    Runner, SuiteReport,
};

/// Selects the compatibility guarantees exercised by a suite run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ConformanceProfile {
    /// Runs functional behavior and exact specification identity cases.
    #[default]
    Full,
    /// Runs portable behavior while allowing specification-version drift.
    Functional,
}

impl ConformanceProfile {
    fn selects(self, requirement: CaseRequirement) -> bool {
        self == Self::Full || requirement == CaseRequirement::Functional
    }
}

/// A synchronous progress event emitted while a suite runs.
#[derive(Debug)]
#[non_exhaustive]
pub enum SuiteEvent<'a> {
    /// The final case selection is known and execution is about to begin.
    SuiteStarted {
        /// Number of selected cases, including cases that will skip.
        selected_cases: usize,
    },
    /// A selected case is about to run or report its static skip.
    CaseStarted {
        /// One-based position in the selected case order.
        index: usize,
        /// Total number of selected cases.
        total: usize,
        /// Stable metadata for the selected case.
        case: CaseMetadata,
    },
    /// A selected case has produced its final result.
    CaseFinished {
        /// One-based position in the selected case order.
        index: usize,
        /// Total number of selected cases.
        total: usize,
        /// Final case result. The reference remains valid for this callback only.
        result: &'a CaseResult,
    },
}

/// Options controlling one suite execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOptions {
    /// Compatibility profile to execute.
    pub profile: ConformanceProfile,
    /// Optional substring matched against stable case identifiers.
    pub filter: Option<String>,
    /// Default timeout applied to each candidate invocation.
    pub invocation_timeout: Duration,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            profile: ConformanceProfile::Full,
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
        self.run_observed(command, options, |_| {})
    }

    /// Runs the suite through the local-process runner and synchronously reports progress.
    ///
    /// The observer runs on the suite thread. It must not retain borrowed event data.
    ///
    /// # Errors
    ///
    /// Returns [`SuiteError`] when a path-like candidate command cannot be
    /// resolved before execution.
    pub fn run_observed<F>(
        self,
        command: &CommandTemplate,
        options: RunOptions,
        observer: F,
    ) -> Result<SuiteReport, SuiteError>
    where
        F: FnMut(SuiteEvent<'_>),
    {
        let command = command
            .resolve()
            .map_err(|source| SuiteError::ResolveCandidate { source })?;
        Ok(self.run_with_observer(
            Arc::new(LocalProcessRunner::new(command)),
            options,
            observer,
        ))
    }

    /// Runs the same registry through a custom execution adapter.
    ///
    /// The adapter must make the candidate see fixture files and honor each
    /// invocation's native arguments, working directory, environment changes,
    /// input mode, and timeout.
    pub fn run_with(self, runner: Arc<dyn Runner>, options: RunOptions) -> SuiteReport {
        self.run_with_observer(runner, options, |_| {})
    }

    /// Runs the registry through a custom adapter and synchronously reports progress.
    ///
    /// Selection by case identifier and profile completes before the first event.
    /// The observer runs on the suite thread and must not retain borrowed event data.
    pub fn run_with_observer<F>(
        self,
        runner: Arc<dyn Runner>,
        options: RunOptions,
        mut observer: F,
    ) -> SuiteReport
    where
        F: FnMut(SuiteEvent<'_>),
    {
        install_case_panic_hook();
        let matching = crate::cases::definitions()
            .filter(|definition| {
                options
                    .filter
                    .as_ref()
                    .is_none_or(|filter| definition.metadata.id().contains(filter))
            })
            .collect::<Vec<_>>();
        let omitted_exact_case_count = matching
            .iter()
            .filter(|definition| !options.profile.selects(definition.metadata.requirement()))
            .count();
        let selected = matching
            .into_iter()
            .filter(|definition| options.profile.selects(definition.metadata.requirement()))
            .collect::<Vec<_>>();
        let total = selected.len();
        observer(SuiteEvent::SuiteStarted {
            selected_cases: total,
        });
        let mut results = Vec::new();
        for (offset, definition) in selected.into_iter().enumerate() {
            let index = offset + 1;
            observer(SuiteEvent::CaseStarted {
                index,
                total,
                case: definition.metadata,
            });

            if let Some(reason) = definition.skip_reason {
                let result = CaseResult {
                    case: definition.metadata,
                    duration_ms: 0,
                    outcome: CaseOutcome::Skipped {
                        reason: reason.to_owned(),
                    },
                };
                observer(SuiteEvent::CaseFinished {
                    index,
                    total,
                    result: &result,
                });
                results.push(result);
                continue;
            }

            let context = Arc::new(CaseContext::new(
                Arc::clone(&runner),
                options.invocation_timeout,
            ));
            let started = Instant::now();
            let Some(run) = definition.run else {
                let result = CaseResult {
                    case: definition.metadata,
                    duration_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
                    outcome: malformed_definition_outcome(definition.metadata),
                };
                observer(SuiteEvent::CaseFinished {
                    index,
                    total,
                    result: &result,
                });
                results.push(result);
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
                (_, Some(ExecutionFailure::Fixture(details))) => CaseOutcome::Error {
                    details: format!("fixture setup failed: {details}"),
                },
                (_, Some(ExecutionFailure::TimedOut(details))) => CaseOutcome::TimedOut { details },
                (Err(payload), None) if !context.candidate_invoked() => CaseOutcome::Error {
                    details: format!("fixture setup failed: {}", panic_message(payload)),
                },
                (Err(payload), None) => CaseOutcome::Failed {
                    details: panic_message(payload),
                },
            };
            let result = CaseResult {
                case: definition.metadata,
                duration_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
                outcome,
            };
            observer(SuiteEvent::CaseFinished {
                index,
                total,
                result: &result,
            });
            results.push(result);
        }

        SuiteReport {
            crate_version: env!("CARGO_PKG_VERSION"),
            specification_version: crate::SPEC_VERSION,
            host_platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            candidate: runner.command().report(),
            cases: results,
            profile: options.profile,
            omitted_exact_case_count,
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

    struct PassingRunner {
        command: CommandTemplate,
    }

    impl Runner for PassingRunner {
        fn command(&self) -> &CommandTemplate {
            &self.command
        }

        fn run(
            &self,
            _invocation: &crate::Invocation,
        ) -> Result<crate::InvocationResult, crate::RunnerError> {
            Ok(crate::InvocationResult::new(
                crate::Termination::Exited { code: 0 },
                Vec::new(),
                Vec::new(),
                Duration::from_millis(1),
            ))
        }
    }

    #[test]
    fn malformed_definition_should_be_an_infrastructure_error() {
        let metadata = CaseMetadata::closure("malformed.case", &["#test"]);

        let outcome = malformed_definition_outcome(metadata);

        assert!(matches!(outcome, CaseOutcome::Error { .. }));
    }

    #[test]
    fn fixture_failure_after_candidate_should_be_an_infrastructure_error() {
        let report = Suite::current().run_with(
            Arc::new(PassingRunner {
                command: CommandTemplate::new("test-candidate"),
            }),
            RunOptions {
                filter: Some("test.fixture-failure.after-candidate".to_owned()),
                ..RunOptions::default()
            },
        );

        assert_eq!(report.cases.len(), 1);
        assert!(matches!(
            &report.cases[0].outcome,
            CaseOutcome::Error { details } if details.starts_with("fixture setup failed:")
        ));
    }
}
