//! Complete borrowed request for one target-owned execution decision.

use peritus_journal::{
    CommittedBudgetTransition, CommittedCapabilityUse, CommittedKernelTransition,
    CommittedLeaseTransition, CurrentAuthorityEpoch,
};
use peritus_policy::AuthorityInstant;
use peritus_protocol::ActionIntentDto;
use peritus_types::{Generation, RevisionNumber, RevisionTuple, SessionId, Sha256Digest};

/// Exact committed observations required to authorize one execution plan.
pub struct ExecutionAuthorizationRequest<'a> {
    intent: &'a ActionIntentDto,
    kernel: &'a CommittedKernelTransition,
    capability: &'a CommittedCapabilityUse,
    budget: &'a CommittedBudgetTransition,
    lease: Option<&'a CommittedLeaseTransition>,
    current_epoch: &'a CurrentAuthorityEpoch,
    revision: RevisionTuple,
    session_id: SessionId,
    expected_generation: Generation,
    expected_revision: RevisionNumber,
    observed_at: AuthorityInstant,
    expected_plan_digest: Sha256Digest,
}

impl<'a> ExecutionAuthorizationRequest<'a> {
    /// Creates one complete unprivileged request; the target-owned gateway checks every field.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        intent: &'a ActionIntentDto,
        kernel: &'a CommittedKernelTransition,
        capability: &'a CommittedCapabilityUse,
        budget: &'a CommittedBudgetTransition,
        lease: Option<&'a CommittedLeaseTransition>,
        current_epoch: &'a CurrentAuthorityEpoch,
        revision: RevisionTuple,
        session_id: SessionId,
        expected_generation: Generation,
        expected_revision: RevisionNumber,
        observed_at: AuthorityInstant,
        expected_plan_digest: Sha256Digest,
    ) -> Self {
        Self {
            intent,
            kernel,
            capability,
            budget,
            lease,
            current_epoch,
            revision,
            session_id,
            expected_generation,
            expected_revision,
            observed_at,
            expected_plan_digest,
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
    pub(crate) const fn budget(&self) -> &CommittedBudgetTransition {
        self.budget
    }
    pub(crate) const fn lease(&self) -> Option<&CommittedLeaseTransition> {
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
    pub(crate) const fn expected_plan_digest(&self) -> Sha256Digest {
        self.expected_plan_digest
    }
}
