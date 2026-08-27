use serde::Serialize;

/// The compatibility requirement exercised by a conformance case.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum CaseRequirement {
    /// Observable behavior that can remain compatible across specification versions.
    #[default]
    Functional,
    /// Exact identity with this crate's specification version or canonical assets.
    Exact,
}

/// Public metadata for one stable conformance case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CaseMetadata {
    id: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_test: Option<&'static str>,
    spec_references: &'static [&'static str],
    #[serde(skip)]
    requirement: CaseRequirement,
}

impl CaseMetadata {
    pub(crate) const fn new(
        id: &'static str,
        source_test: &'static str,
        spec_references: &'static [&'static str],
    ) -> Self {
        Self {
            id,
            source_test: Some(source_test),
            spec_references,
            requirement: CaseRequirement::Functional,
        }
    }

    /// Returns the stable, dot-separated case identifier.
    pub const fn id(self) -> &'static str {
        self.id
    }

    /// Returns the Phase 1 source test key, or `None` for a closure case added
    /// after the inventory audit.
    pub const fn source_test(self) -> Option<&'static str> {
        self.source_test
    }

    pub(crate) const fn closure(
        id: &'static str,
        spec_references: &'static [&'static str],
    ) -> Self {
        Self {
            id,
            source_test: None,
            spec_references,
            requirement: CaseRequirement::Functional,
        }
    }

    pub(crate) const fn exact(id: &'static str, spec_references: &'static [&'static str]) -> Self {
        Self {
            id,
            source_test: None,
            spec_references,
            requirement: CaseRequirement::Exact,
        }
    }

    /// Returns specification section anchors or requirement identifiers.
    pub const fn spec_references(self) -> &'static [&'static str] {
        self.spec_references
    }

    /// Returns whether the case checks functional behavior or exact identity.
    pub const fn requirement(self) -> CaseRequirement {
        self.requirement
    }
}

pub(crate) type CaseFn = fn();

#[derive(Clone, Copy)]
pub(crate) struct CaseDefinition {
    pub metadata: CaseMetadata,
    pub run: Option<CaseFn>,
    pub skip_reason: Option<&'static str>,
}

impl CaseDefinition {
    pub const fn new(metadata: CaseMetadata, run: CaseFn) -> Self {
        Self {
            metadata,
            run: Some(run),
            skip_reason: None,
        }
    }

    pub const fn skipped(metadata: CaseMetadata, reason: &'static str) -> Self {
        Self {
            metadata,
            run: None,
            skip_reason: Some(reason),
        }
    }
}
