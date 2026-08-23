//! Closed executable-state projection used by reducer refinement contracts.

use vstd::prelude::*;

verus! {

impl super::ApprovalAggregate {
    /// Returns the closed logical model projection used by reducer contracts.
    pub closed spec fn spec_model(&self) -> crate::model::ApprovalModelState {
        match self.state {
            super::ApprovalState::Pending => crate::model::ApprovalModelState {
                phase: crate::ApprovalPhase::Pending,
                decision: None,
                resolution_count: 0,
                use_count: 0,
            },
            super::ApprovalState::ApprovedOnce(value) => crate::model::ApprovalModelState {
                phase: crate::ApprovalPhase::ApprovedOnce,
                decision: Some(value.decision_digest),
                resolution_count: 1,
                use_count: 0,
            },
            super::ApprovalState::AmendmentAuthorized(value) => crate::model::ApprovalModelState {
                phase: crate::ApprovalPhase::AmendmentAuthorized,
                decision: Some(value.decision_digest),
                resolution_count: 1,
                use_count: 0,
            },
            super::ApprovalState::Consumed(value) => crate::model::ApprovalModelState {
                phase: crate::ApprovalPhase::Consumed,
                decision: Some(value.decision_digest),
                resolution_count: 1,
                use_count: 1,
            },
            super::ApprovalState::Amended(value) => crate::model::ApprovalModelState {
                phase: crate::ApprovalPhase::Amended,
                decision: Some(value.decision_digest),
                resolution_count: 1,
                use_count: 0,
            },
            super::ApprovalState::Denied(value) => crate::model::ApprovalModelState {
                phase: crate::ApprovalPhase::Denied,
                decision: Some(value.decision_digest),
                resolution_count: 1,
                use_count: 0,
            },
            super::ApprovalState::Expired(Some(value)) => crate::model::ApprovalModelState {
                phase: crate::ApprovalPhase::Expired,
                decision: Some(value.decision_digest),
                resolution_count: 1,
                use_count: 0,
            },
            super::ApprovalState::Expired(None) => crate::model::ApprovalModelState {
                phase: crate::ApprovalPhase::Expired,
                decision: None,
                resolution_count: 0,
                use_count: 0,
            },
            super::ApprovalState::Cancelled => crate::model::ApprovalModelState {
                phase: crate::ApprovalPhase::Cancelled,
                decision: None,
                resolution_count: 0,
                use_count: 0,
            },
        }
    }

    /// Returns exact structural snapshot equality for rejection-preservation contracts.
    pub closed spec fn spec_same_snapshot(&self, other: &Self) -> bool {
        self.request == other.request && self.state == other.state
    }
}

pub(super) proof fn aggregate_satisfies_inv(aggregate: &super::ApprovalAggregate)
    ensures crate::model::inv_009(aggregate.spec_model()),
{
}

pub(super) proof fn initial_refines(aggregate: &super::ApprovalAggregate)
    requires aggregate.state == super::ApprovalState::Pending,
    ensures
        aggregate.spec_model() == crate::model::initial(),
        crate::model::inv_009(aggregate.spec_model()),
{
}

pub(super) proof fn pending_resolution_refines(
    before: &super::ApprovalAggregate,
    after: &super::ApprovalAggregate,
    choice: crate::ApprovalChoice,
    digest: crate::ApprovalDecisionDigest,
    resolution: super::Resolution,
)
    requires
        before.state == super::ApprovalState::Pending,
        resolution.choice == choice,
        resolution.decision_digest == digest,
        after.state == match choice {
            crate::ApprovalChoice::Deny => super::ApprovalState::Denied(resolution),
            crate::ApprovalChoice::ApproveOnce => super::ApprovalState::ApprovedOnce(resolution),
            crate::ApprovalChoice::Amend(_) => {
                super::ApprovalState::AmendmentAuthorized(resolution)
            }
        },
    ensures
        crate::model::inv_009(before.spec_model()),
        after.spec_model() == crate::model::next(
            before.spec_model(),
            crate::model::resolution_step(choice, digest),
        ),
        crate::model::inv_009(after.spec_model()),
{
    aggregate_satisfies_inv(before);
}

pub(super) proof fn replay_refines(
    before: &super::ApprovalAggregate,
    after: &super::ApprovalAggregate,
    digest: crate::ApprovalDecisionDigest,
)
    requires
        before.state != super::ApprovalState::Pending,
        after == before,
    ensures
        before.spec_model().phase != crate::ApprovalPhase::Pending,
        crate::model::inv_009(before.spec_model()),
        after.spec_model() == crate::model::next(
            before.spec_model(),
            crate::model::ApprovalModelStep::Replay(digest),
        ),
        crate::model::inv_009(after.spec_model()),
{
    aggregate_satisfies_inv(before);
}

pub(super) proof fn cancel_refines(
    before: &super::ApprovalAggregate,
    after: &super::ApprovalAggregate,
)
    requires
        before.state == super::ApprovalState::Pending,
        after.state == super::ApprovalState::Cancelled,
    ensures
        crate::model::inv_009(before.spec_model()),
        after.spec_model() == crate::model::next(
            before.spec_model(),
            crate::model::ApprovalModelStep::Cancel,
        ),
        crate::model::inv_009(after.spec_model()),
{
    aggregate_satisfies_inv(before);
}

pub(super) proof fn expire_refines(
    before: &super::ApprovalAggregate,
    after: &super::ApprovalAggregate,
    resolution: Option<super::Resolution>,
)
    requires
        after.state == super::ApprovalState::Expired(resolution),
        match before.state {
            super::ApprovalState::Pending => resolution.is_none(),
            super::ApprovalState::ApprovedOnce(value)
            | super::ApprovalState::AmendmentAuthorized(value) => resolution == Some(value),
            _ => false,
        },
    ensures
        crate::model::inv_009(before.spec_model()),
        after.spec_model() == crate::model::next(
            before.spec_model(),
            crate::model::ApprovalModelStep::Expire,
        ),
        crate::model::inv_009(after.spec_model()),
{
    aggregate_satisfies_inv(before);
}

pub(super) proof fn consume_once_refines(
    before: &super::ApprovalAggregate,
    after: &super::ApprovalAggregate,
    resolution: super::Resolution,
)
    requires
        before.state == super::ApprovalState::ApprovedOnce(resolution),
        after.state == super::ApprovalState::Consumed(resolution),
    ensures
        before.spec_model().phase == crate::ApprovalPhase::ApprovedOnce,
        before.spec_model().resolution_count == 1,
        crate::model::inv_009(before.spec_model()),
        after.spec_model() == crate::model::next(
            before.spec_model(),
            crate::model::ApprovalModelStep::ConsumeOnce,
        ),
        crate::model::inv_009(after.spec_model()),
{
    aggregate_satisfies_inv(before);
}

pub(super) proof fn consume_amendment_refines(
    before: &super::ApprovalAggregate,
    after: &super::ApprovalAggregate,
    resolution: super::Resolution,
)
    requires
        before.state == super::ApprovalState::AmendmentAuthorized(resolution),
        after.state == super::ApprovalState::Amended(resolution),
    ensures
        crate::model::inv_009(before.spec_model()),
        after.spec_model() == crate::model::next(
            before.spec_model(),
            crate::model::ApprovalModelStep::ConsumeAmendment,
        ),
        crate::model::inv_009(after.spec_model()),
{
    aggregate_satisfies_inv(before);
}

} // verus!
