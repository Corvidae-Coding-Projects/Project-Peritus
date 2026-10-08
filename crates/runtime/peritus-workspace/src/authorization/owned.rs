//! Move-only authority observations for an owned background workspace operation.

use super::{
    ActionIntentDto, AuthorityInstant, CommittedCapabilityUse, CommittedKernelTransition,
    CommittedLeaseTransition, CurrentAuthorityEpoch, Generation, RevisionNumber, RevisionTuple,
    SessionId, WorkspaceAuthorizationRequest, WorkspaceCallerBinding,
};

/// Owns the original committed receipts so a worker can validate them without borrowed lifetimes.
/// Construction grants no authority; the workspace gateway still checks every receipt.
pub struct OwnedWorkspaceAuthorization {
    intent: ActionIntentDto,
    kernel: CommittedKernelTransition,
    capability: CommittedCapabilityUse,
    lease: CommittedLeaseTransition,
    current_epoch: CurrentAuthorityEpoch,
    revision: RevisionTuple,
    session_id: SessionId,
    expected_generation: Generation,
    expected_revision: RevisionNumber,
    observed_at: AuthorityInstant,
    caller: Option<WorkspaceCallerBinding>,
}

impl OwnedWorkspaceAuthorization {
    /// Transfers the complete original receipt set into one owned request.
    #[must_use]
    #[allow(
        clippy::too_many_arguments,
        reason = "every original authority observation stays explicit"
    )]
    pub const fn new(
        intent: ActionIntentDto,
        kernel: CommittedKernelTransition,
        capability: CommittedCapabilityUse,
        lease: CommittedLeaseTransition,
        current_epoch: CurrentAuthorityEpoch,
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
            caller: None,
        }
    }

    /// Adds the exact C4 caller projection; the gateway independently cross-checks it.
    #[must_use]
    pub fn with_caller_binding(mut self, caller: WorkspaceCallerBinding) -> Self {
        self.caller = Some(caller);
        self
    }

    /// Borrows the original receipts through the ordinary target-owned validation boundary.
    #[must_use]
    pub fn as_request(&self) -> WorkspaceAuthorizationRequest<'_> {
        WorkspaceAuthorizationRequest {
            intent: &self.intent,
            kernel: &self.kernel,
            capability: &self.capability,
            lease: &self.lease,
            current_epoch: &self.current_epoch,
            revision: self.revision,
            session_id: self.session_id,
            expected_generation: self.expected_generation,
            expected_revision: self.expected_revision,
            observed_at: self.observed_at,
            caller: self.caller.clone(),
        }
    }
}
