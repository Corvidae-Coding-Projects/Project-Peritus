//! Active guidance records and content-free durable tombstones.

use super::{
    AppProtocolError, ControlOperationId, Sha256Digest, WorkbenchGuidanceContent,
    WorkbenchGuidanceIdentity, WorkbenchGuidanceLifecycle, WorkbenchGuidanceReason,
    WorkbenchGuidanceScope, WorkbenchGuidanceValidation, WorkbenchGuidanceVersion, invalid,
};

/// Active reusable guidance at one exact project-local revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchGuidanceRecord {
    pub(super) identity: WorkbenchGuidanceIdentity,
    pub(super) version: WorkbenchGuidanceVersion,
    pub(super) content: WorkbenchGuidanceContent,
    pub(super) pinned: bool,
    pub(super) last_validation: WorkbenchGuidanceValidation,
}

/// Prior active state bound into a content-free tombstone.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchGuidancePrior {
    revision: u64,
    digest: Sha256Digest,
    scope: WorkbenchGuidanceScope,
    pinned: bool,
}

impl WorkbenchGuidancePrior {
    /// Validates a positive prior revision.
    ///
    /// # Errors
    /// Rejects revision zero.
    pub const fn new(
        revision: u64,
        digest: Sha256Digest,
        scope: WorkbenchGuidanceScope,
        pinned: bool,
    ) -> Result<Self, AppProtocolError> {
        if revision == 0 {
            return Err(invalid());
        }
        Ok(Self { revision, digest, scope, pinned })
    }

    /// Returns the last active record revision.
    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision
    }

    /// Returns the digest of forgotten content, never the content itself.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.digest
    }

    /// Returns the last active within-project scope.
    #[must_use]
    pub const fn scope(self) -> WorkbenchGuidanceScope {
        self.scope
    }

    /// Returns the last active pin state.
    #[must_use]
    pub const fn pinned(self) -> bool {
        self.pinned
    }
}

/// Durable content-free forget marker retained for inspection and replay.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchGuidanceTombstone {
    pub(super) identity: WorkbenchGuidanceIdentity,
    pub(super) prior: WorkbenchGuidancePrior,
    pub(super) forgotten_by: ControlOperationId,
    pub(super) reason: WorkbenchGuidanceReason,
    pub(super) dependency_revision: u64,
}

impl WorkbenchGuidanceTombstone {
    /// Reconstructs a checked durable tombstone.
    ///
    /// # Errors
    /// Rejects zero dependency revision.
    pub fn new(
        identity: WorkbenchGuidanceIdentity,
        prior: WorkbenchGuidancePrior,
        forgotten_by: ControlOperationId,
        reason: WorkbenchGuidanceReason,
        dependency_revision: u64,
    ) -> Result<Self, AppProtocolError> {
        if dependency_revision == 0 {
            return Err(invalid());
        }
        Ok(Self { identity, prior, forgotten_by, reason, dependency_revision })
    }

    /// Returns stable guidance identity and workspace binding.
    #[must_use]
    pub const fn identity(&self) -> WorkbenchGuidanceIdentity {
        self.identity
    }

    /// Returns the last active revision metadata without retaining text.
    #[must_use]
    pub const fn prior(&self) -> WorkbenchGuidancePrior {
        self.prior
    }

    /// Returns the authenticated forget operation.
    #[must_use]
    pub const fn forgotten_by(&self) -> ControlOperationId {
        self.forgotten_by
    }

    /// Borrows the visible reason.
    #[must_use]
    pub const fn reason(&self) -> &WorkbenchGuidanceReason {
        &self.reason
    }

    /// Returns the dependent-view revision produced by forgetting.
    #[must_use]
    pub const fn dependency_revision(&self) -> u64 {
        self.dependency_revision
    }

    /// Returns the forgotten lifecycle.
    #[must_use]
    pub const fn lifecycle(&self) -> WorkbenchGuidanceLifecycle {
        WorkbenchGuidanceLifecycle::Forgotten
    }
}
