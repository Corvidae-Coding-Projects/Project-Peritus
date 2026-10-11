//! Move-only logical approve-once consumption outputs.

use peritus_policy::AuthorityInstant;
use peritus_types::{ActionId, ApprovalRequestId, CommandId, RevisionNumber, RevisionTuple};

#[doc = " One unprivileged logical action-approval transition."]
#[doc = ""]
#[doc = " This value is neither a durable receipt nor an effect permit."]
#[doc = " Callers cannot forge it from public fields:"]
#[doc = ""]
#[doc = " ```compile_fail"]
#[doc = " use peritus_approval::{ActionDigest, ApprovalDecisionDigest, ApprovalRequestDigest,"]
#[doc = "     ApprovedActionTransition};"]
#[doc = " use peritus_policy::AuthorityInstant;"]
#[doc = " use peritus_types::{ActionId, ApprovalRequestId, CommandId, RevisionNumber, RevisionTuple};"]
#[doc = ""]
#[doc = " fn forge("]
#[doc = "     request_id: ApprovalRequestId,"]
#[doc = "     request_digest: ApprovalRequestDigest,"]
#[doc = "     action_id: ActionId,"]
#[doc = "     action_digest: ActionDigest,"]
#[doc = "     revision: RevisionTuple,"]
#[doc = "     decision_digest: ApprovalDecisionDigest,"]
#[doc = "     command_id: CommandId,"]
#[doc = "     registry_revision: RevisionNumber,"]
#[doc = "     valid_until: AuthorityInstant,"]
#[doc = " ) -> ApprovedActionTransition {"]
#[doc = "     ApprovedActionTransition { request_id, request_digest, action_id, action_digest, revision,"]
#[doc = "         decision_digest, command_id, registry_revision, valid_until }"]
#[doc = " }"]
#[doc = " ```"]
#[doc = ""]
#[doc = " Successful transitions are intentionally non-duplicable:"]
#[doc = ""]
#[doc = " ```compile_fail"]
#[doc = " use peritus_approval::ApprovedActionTransition;"]
#[doc = ""]
#[doc = " fn duplicate(value: ApprovedActionTransition) {"]
#[doc = "     let _copy = value.clone();"]
#[doc = " }"]
#[doc = " ```"]
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
    ) -> Self {
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
    #[doc = " Returns the request identity."]
    #[must_use]
    pub const fn request_id(&self) -> ApprovalRequestId {
        self.request_id
    }
    #[doc = " Returns the request digest."]
    #[must_use]
    pub const fn request_digest(&self) -> crate::ApprovalRequestDigest {
        self.request_digest
    }
    #[doc = " Returns the exact action identity."]
    #[must_use]
    pub const fn action_id(&self) -> ActionId {
        self.action_id
    }
    #[doc = " Returns the exact action digest."]
    #[must_use]
    pub const fn action_digest(&self) -> crate::ActionDigest {
        self.action_digest
    }
    #[doc = " Returns the exact authority revision."]
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    #[doc = " Returns the authenticated decision digest."]
    #[must_use]
    pub const fn decision_digest(&self) -> crate::ApprovalDecisionDigest {
        self.decision_digest
    }
    #[doc = " Returns the unique decision command identity."]
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
    #[doc = " Returns the non-authoritative supplied registry revision."]
    #[must_use]
    pub const fn registry_revision(&self) -> RevisionNumber {
        self.registry_revision
    }
    #[doc = " Returns the exclusive earliest validity bound."]
    #[must_use]
    pub const fn valid_until(&self) -> AuthorityInstant {
        self.valid_until
    }
}
#[doc = " Checked record that one exact approve-once decision was consumed."]
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
    ) -> Self {
        Self { request_id, decision_digest, action_id }
    }
    #[doc = " Returns the consumed request identity."]
    #[must_use]
    pub const fn request_id(&self) -> ApprovalRequestId {
        self.request_id
    }
    #[doc = " Returns the consumed decision digest."]
    #[must_use]
    pub const fn decision_digest(&self) -> crate::ApprovalDecisionDigest {
        self.decision_digest
    }
    #[doc = " Returns the exact consumed action identity."]
    #[must_use]
    pub const fn action_id(&self) -> ActionId {
        self.action_id
    }
}
#[doc = " Successful move-only approve-once consumption."]
#[derive(Debug, Eq, PartialEq)]
pub struct ApprovalUseOutcome {
    pub(crate) aggregate: crate::ApprovalAggregate,
    pub(crate) transition: ApprovedActionTransition,
    pub(crate) consumed: ConsumedApproval,
}
impl ApprovalUseOutcome {
    #[doc = " Borrows the exact successor aggregate."]
    #[must_use]
    pub const fn aggregate(&self) -> &crate::ApprovalAggregate {
        &self.aggregate
    }
    #[doc = " Borrows the logical action transition."]
    #[must_use]
    pub const fn transition(&self) -> &ApprovedActionTransition {
        &self.transition
    }
    #[doc = " Borrows the exact consumption record."]
    #[must_use]
    pub const fn consumed(&self) -> &ConsumedApproval {
        &self.consumed
    }
    #[doc = " Consumes the outcome into its move-only parts."]
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (crate::ApprovalAggregate, ApprovedActionTransition, ConsumedApproval) {
        (self.aggregate, self.transition, self.consumed)
    }
}
