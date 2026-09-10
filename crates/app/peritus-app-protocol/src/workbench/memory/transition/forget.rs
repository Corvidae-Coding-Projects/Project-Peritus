//! Explicit revision-fenced guidance forgetting.

use super::super::WorkbenchGuidanceReason;
use super::WorkbenchGuidanceSelection;

/// Explicit forget request; this is not a historical purge request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchGuidanceForget {
    selection: WorkbenchGuidanceSelection,
    expected_dependency_revision: u64,
    reason: WorkbenchGuidanceReason,
}

impl WorkbenchGuidanceForget {
    /// Creates a revision-fenced tombstone request.
    #[must_use]
    pub const fn new(
        selection: WorkbenchGuidanceSelection,
        expected_dependency_revision: u64,
        reason: WorkbenchGuidanceReason,
    ) -> Self {
        Self { selection, expected_dependency_revision, reason }
    }

    /// Returns exact selected identity and record revision.
    #[must_use]
    pub const fn selection(&self) -> WorkbenchGuidanceSelection {
        self.selection
    }

    /// Returns the exact inspected dependent-view revision.
    #[must_use]
    pub const fn expected_dependency_revision(&self) -> u64 {
        self.expected_dependency_revision
    }

    /// Borrows the visible reason for excluding future retrieval.
    #[must_use]
    pub const fn reason(&self) -> &WorkbenchGuidanceReason {
        &self.reason
    }

    pub(super) fn into_reason(self) -> WorkbenchGuidanceReason {
        self.reason
    }
}
