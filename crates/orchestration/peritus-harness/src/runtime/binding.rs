//! Read-only bridge from the stable shared revision tuple to exact E1 revision identity.

use core::fmt;

use peritus_types::RevisionTuple;

use crate::{
    domain::{HarnessRevision, HarnessRevisionIdentity},
    materialization::{MaterializationReceipt, WorkspaceSnapshot},
};

/// Exact governing E1 identity carried beside the unchanged shared seven-field revision tuple.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GoverningHarnessBinding {
    revision: RevisionTuple,
    harness_revision: HarnessRevisionIdentity,
    materialization: MaterializationReceipt,
}

impl GoverningHarnessBinding {
    /// Validates stable lineage, full revision digest/number, and installed C1 snapshot equality.
    ///
    /// # Errors
    /// Rejects a tuple, E1 revision, or retained receipt from another lineage, revision, or
    /// workspace snapshot.
    pub fn new(
        revision: RevisionTuple,
        harness_revision: &HarnessRevision,
        materialization: &MaterializationReceipt,
    ) -> Result<Self, GoverningHarnessBindingError> {
        let identity = harness_revision.identity();
        let installed = materialization.after();
        let mismatched = [
            revision.harness_id() != identity.harness_id(),
            revision.harness_id() != materialization.harness_id(),
            materialization.revision_digest() != identity.digest(),
            revision.workspace_id() != installed.workspace_id(),
            revision.workspace_generation() != installed.generation(),
            revision.workspace_revision() != installed.revision(),
        ]
        .into_iter()
        .any(core::convert::identity);
        if mismatched {
            return Err(GoverningHarnessBindingError);
        }
        Ok(Self { revision, harness_revision: identity, materialization: materialization.clone() })
    }

    /// Returns the unchanged shared authority/evidence tuple.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Returns the exact branch-distinguishing E1 harness revision.
    #[must_use]
    pub const fn harness_revision(&self) -> HarnessRevisionIdentity {
        self.harness_revision
    }
    /// Returns the complete exact materialization evidence.
    #[must_use]
    pub const fn materialization(&self) -> &MaterializationReceipt {
        &self.materialization
    }
    /// Returns the clean installed workspace snapshot.
    #[must_use]
    pub const fn installed_snapshot(&self) -> &WorkspaceSnapshot {
        self.materialization.after()
    }
}

/// Stable mismatch returned by [`GoverningHarnessBinding::new`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct GoverningHarnessBindingError;

impl fmt::Display for GoverningHarnessBindingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("shared revision tuple does not match the exact installed E1 revision")
    }
}

impl std::error::Error for GoverningHarnessBindingError {}
