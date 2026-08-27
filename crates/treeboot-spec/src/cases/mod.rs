mod closure;
mod generated;
mod ported;
pub(crate) mod support;

use crate::case::CaseDefinition;

pub(crate) fn definitions() -> impl Iterator<Item = &'static CaseDefinition> {
    generated::DEFINITIONS
        .iter()
        .chain(closure::DEFINITIONS)
        .chain(SUITE_TEST_DEFINITION.iter())
}

#[cfg(test)]
const SUITE_TEST_DEFINITION: Option<CaseDefinition> = Some(CaseDefinition::new(
    crate::CaseMetadata::closure("test.fixture-failure.after-candidate", &["#test"]),
    support::fixture_failure_after_candidate_for_suite_regression,
));

#[cfg(not(test))]
const SUITE_TEST_DEFINITION: Option<CaseDefinition> = None;
