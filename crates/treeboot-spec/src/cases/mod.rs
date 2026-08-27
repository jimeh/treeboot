mod closure;
mod generated;
mod ported;
pub(crate) mod support;

use crate::case::CaseDefinition;

pub(crate) fn definitions() -> impl Iterator<Item = &'static CaseDefinition> {
    generated::DEFINITIONS.iter().chain(closure::DEFINITIONS)
}
