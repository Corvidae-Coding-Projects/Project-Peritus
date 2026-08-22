//! Human-readable descriptors paired with stable machine identifiers.

use crate::{CaseId, ReportText, SuiteId};

/// Immutable metadata for one conformance suite.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SuiteDescriptor {
    id: SuiteId,
    summary: ReportText,
}

impl SuiteDescriptor {
    /// Creates suite metadata from a validated identifier and human-readable summary.
    #[must_use]
    pub const fn new(id: SuiteId, summary: ReportText) -> Self {
        Self { id, summary }
    }

    /// Returns the stable suite identifier.
    #[must_use]
    pub const fn id(&self) -> &SuiteId {
        &self.id
    }

    /// Returns the human-readable suite summary.
    #[must_use]
    pub const fn summary(&self) -> &ReportText {
        &self.summary
    }
}

/// Immutable metadata for one conformance case.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaseDescriptor {
    id: CaseId,
    summary: ReportText,
}

impl CaseDescriptor {
    /// Creates case metadata from a validated identifier and human-readable summary.
    #[must_use]
    pub const fn new(id: CaseId, summary: ReportText) -> Self {
        Self { id, summary }
    }

    /// Returns the stable case identifier.
    #[must_use]
    pub const fn id(&self) -> &CaseId {
        &self.id
    }

    /// Returns the human-readable case summary.
    #[must_use]
    pub const fn summary(&self) -> &ReportText {
        &self.summary
    }
}

/// Immutable metadata for the implementation under conformance test.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubjectDescriptor {
    name: ReportText,
    implementation: ReportText,
}

impl SubjectDescriptor {
    /// Creates subject metadata.
    #[must_use]
    pub const fn new(name: ReportText, implementation: ReportText) -> Self {
        Self { name, implementation }
    }

    /// Returns the subject's human-readable name.
    #[must_use]
    pub const fn name(&self) -> &ReportText {
        &self.name
    }

    /// Returns the tested implementation identity or revision.
    #[must_use]
    pub const fn implementation(&self) -> &ReportText {
        &self.implementation
    }
}
