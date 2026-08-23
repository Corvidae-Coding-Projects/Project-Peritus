//! Complete borrowed request for one target-owned authorization decision.

use peritus_journal::{
    CommittedCapabilityUse, CommittedKernelTransition, CommittedLeaseTransition,
    CurrentAuthorityEpoch,
};
use peritus_policy::AuthorityInstant;
use peritus_protocol::ActionIntentDto;
use peritus_types::{Generation, RevisionNumber, RevisionTuple, SessionId};

/// Exact C0 receipts and current facts required by one workspace mutation.
pub struct WorkspaceAuthorizationRequest<'a> {
    intent: &'a ActionIntentDto,
    kernel: &'a CommittedKernelTransition,
    capability: &'a CommittedCapabilityUse,
    lease: &'a CommittedLeaseTransition,
    current_epoch: &'a CurrentAuthorityEpoch,
    revision: RevisionTuple,
    session_id: SessionId,
    expected_generation: Generation,
    expected_revision: RevisionNumber,
    observed_at: AuthorityInstant,
}

impl<'a> WorkspaceAuthorizationRequest<'a> {
    /// Creates one complete request. Construction is unprivileged; the target gateway checks it.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        intent: &'a ActionIntentDto,
        kernel: &'a CommittedKernelTransition,
        capability: &'a CommittedCapabilityUse,
        lease: &'a CommittedLeaseTransition,
        current_epoch: &'a CurrentAuthorityEpoch,
        revision: RevisionTuple,
        session_id: SessionId,
        expected_generation: Generation,
        expected_revision: RevisionNumber,
        observed_at: AuthorityInstant,
    ) -> Self {
        Self {
            intent,
            kernel,
            capability,
            lease,
            current_epoch,
            revision,
            session_id,
            expected_generation,
            expected_revision,
            observed_at,
        }
    }

    pub(crate) const fn intent(&self) -> &ActionIntentDto {
        self.intent
    }
    pub(crate) const fn kernel(&self) -> &CommittedKernelTransition {
        self.kernel
    }
    pub(crate) const fn capability(&self) -> &CommittedCapabilityUse {
        self.capability
    }
    pub(crate) const fn lease(&self) -> &CommittedLeaseTransition {
        self.lease
    }
    pub(crate) const fn current_epoch(&self) -> &CurrentAuthorityEpoch {
        self.current_epoch
    }
    pub(crate) const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    pub(crate) const fn session_id(&self) -> SessionId {
        self.session_id
    }
    pub(crate) const fn expected_generation(&self) -> Generation {
        self.expected_generation
    }
    pub(crate) const fn expected_revision(&self) -> RevisionNumber {
        self.expected_revision
    }
    pub(crate) const fn observed_at(&self) -> AuthorityInstant {
        self.observed_at
    }
}
