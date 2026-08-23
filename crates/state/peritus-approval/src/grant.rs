//! Move-only logical approve-once consumption outputs.

use peritus_policy::AuthorityInstant;
use peritus_types::{ActionId, ApprovalRequestId, CommandId, RevisionNumber, RevisionTuple};
use vstd::prelude::*;

verus! {

/// One unprivileged logical action-approval transition.
///
/// This value is neither a durable receipt nor an effect permit.
/// Callers cannot forge it from public fields:
///
/// ```compile_fail
/// use peritus_approval::{ActionDigest, ApprovalDecisionDigest, ApprovalRequestDigest,
///     ApprovedActionTransition};
/// use peritus_policy::AuthorityInstant;
/// use peritus_types::{ActionId, ApprovalRequestId, CommandId, RevisionNumber, RevisionTuple};
///
/// fn forge(
///     request_id: ApprovalRequestId,
///     request_digest: ApprovalRequestDigest,
///     action_id: ActionId,
///     action_digest: ActionDigest,
///     revision: RevisionTuple,
///     decision_digest: ApprovalDecisionDigest,
///     command_id: CommandId,
///     registry_revision: RevisionNumber,
///     valid_until: AuthorityInstant,
/// ) -> ApprovedActionTransition {
///     ApprovedActionTransition { request_id, request_digest, action_id, action_digest, revision,
///         decision_digest, command_id, registry_revision, valid_until }
/// }
/// ```
///
/// Successful transitions are intentionally non-duplicable:
///
/// ```compile_fail
/// use peritus_approval::ApprovedActionTransition;
///
/// fn duplicate(value: ApprovedActionTransition) {
///     let _copy = value.clone();
/// }
/// ```
#[derive(Debug, Eq, PartialEq)]
pub struct ApprovedActionTransition {
    pub(crate) request_id: ApprovalRequestId,
    pub(crate) request_digest: crate::ApprovalRequestDigest,
    pub(crate) action_id: ActionId,
    pub(crate) action_digest: crate::ActionDigest,
    pub(crate) revision: RevisionTuple,
    pub(crate) decision_digest: crate::ApprovalDecisionDigest,
    pub(crate) command_id: CommandId,
    pub(crate) registry_revision: RevisionNumber,
    pub(crate) valid_until: AuthorityInstant,
}

impl ApprovedActionTransition {
    #[allow(clippy::too_many_arguments, reason = "all logical authority bindings are explicit")]
    pub(crate) const fn new(
        request_id: ApprovalRequestId,
        request_digest: crate::ApprovalRequestDigest,
        action_id: ActionId,
        action_digest: crate::ActionDigest,
        revision: RevisionTuple,
        decision_digest: crate::ApprovalDecisionDigest,
        command_id: CommandId,
        registry_revision: RevisionNumber,
        valid_until: AuthorityInstant,
    ) -> (transition: Self)
        ensures
            transition.request_id == request_id,
            transition.request_digest == request_digest,
            transition.action_id == action_id,
            transition.action_digest == action_digest,
            transition.revision == revision,
            transition.decision_digest == decision_digest,
            transition.command_id == command_id,
            transition.registry_revision == registry_revision,
            transition.valid_until == valid_until,
    {
        Self {
            request_id,
            request_digest,
            action_id,
            action_digest,
            revision,
            decision_digest,
            command_id,
            registry_revision,
            valid_until,
        }
    }

    /// Returns the request identity.
    #[must_use]
    pub const fn request_id(&self) -> ApprovalRequestId { self.request_id }

    /// Returns the request digest.
    #[must_use]
    pub const fn request_digest(&self) -> crate::ApprovalRequestDigest { self.request_digest }

    /// Returns the exact action identity.
    #[must_use]
    pub const fn action_id(&self) -> ActionId { self.action_id }

    /// Returns the exact action digest.
    #[must_use]
    pub const fn action_digest(&self) -> crate::ActionDigest { self.action_digest }

    /// Returns the exact authority revision.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple { self.revision }

    /// Returns the authenticated decision digest.
    #[must_use]
    pub const fn decision_digest(&self) -> crate::ApprovalDecisionDigest { self.decision_digest }

    /// Returns the unique decision command identity.
    #[must_use]
    pub const fn command_id(&self) -> CommandId { self.command_id }

    /// Returns the non-authoritative supplied registry revision.
    #[must_use]
    pub const fn registry_revision(&self) -> RevisionNumber { self.registry_revision }

    /// Returns the exclusive earliest validity bound.
    #[must_use]
    pub const fn valid_until(&self) -> AuthorityInstant { self.valid_until }
}

/// Checked record that one exact approve-once decision was consumed.
#[derive(Debug, Eq, PartialEq)]
pub struct ConsumedApproval {
    pub(crate) request_id: ApprovalRequestId,
    pub(crate) decision_digest: crate::ApprovalDecisionDigest,
    pub(crate) action_id: ActionId,
}

impl ConsumedApproval {
    pub(crate) const fn new(
        request_id: ApprovalRequestId,
        decision_digest: crate::ApprovalDecisionDigest,
        action_id: ActionId,
    ) -> (consumed: Self)
        ensures
            consumed.request_id == request_id,
            consumed.decision_digest == decision_digest,
            consumed.action_id == action_id,
    {
        Self { request_id, decision_digest, action_id }
    }

    /// Returns the consumed request identity.
    #[must_use]
    pub const fn request_id(&self) -> ApprovalRequestId { self.request_id }

    /// Returns the consumed decision digest.
    #[must_use]
    pub const fn decision_digest(&self) -> crate::ApprovalDecisionDigest { self.decision_digest }

    /// Returns the exact consumed action identity.
    #[must_use]
    pub const fn action_id(&self) -> ActionId { self.action_id }
}

/// Successful move-only approve-once consumption.
#[derive(Debug, Eq, PartialEq)]
pub struct ApprovalUseOutcome {
    pub(crate) aggregate: crate::ApprovalAggregate,
    pub(crate) transition: ApprovedActionTransition,
    pub(crate) consumed: ConsumedApproval,
}

impl ApprovalUseOutcome {
    /// Returns the accepted aggregate's closed logical model projection.
    pub closed spec fn spec_model(&self) -> crate::model::ApprovalModelState {
        self.aggregate.spec_model()
    }

    pub(crate) proof fn prove_model(&self)
        ensures self.spec_model() == self.aggregate.spec_model(),
    {
    }

    /// Borrows the exact successor aggregate.
    #[must_use]
    pub const fn aggregate(&self) -> &crate::ApprovalAggregate { &self.aggregate }

    /// Borrows the logical action transition.
    #[must_use]
    pub const fn transition(&self) -> &ApprovedActionTransition { &self.transition }

    /// Borrows the exact consumption record.
    #[must_use]
    pub const fn consumed(&self) -> &ConsumedApproval { &self.consumed }

    /// Consumes the outcome into its move-only parts.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (crate::ApprovalAggregate, ApprovedActionTransition, ConsumedApproval) {
        (self.aggregate, self.transition, self.consumed)
    }
}

} // verus!
