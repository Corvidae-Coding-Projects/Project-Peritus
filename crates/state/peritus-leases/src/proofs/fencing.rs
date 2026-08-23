//! Generation-fencing and stale-claim rejection lemmas.

#[cfg(verus_only)]
use crate::{model, LeaseAggregate, LeaseClaim, LeaseTransitionKind, LeaseTransitionRecord};
use vstd::prelude::*;

verus! {

pub(crate) proof fn strict_generation_advance_invalidates_old_claim(
    old_generation: int,
    new_generation: int,
    holder: int,
)
    requires model::generation_is_fenced(old_generation, new_generation),
    ensures
        !model::logical_claim_is_current(
            new_generation,
            holder,
            old_generation,
            holder,
        ),
{
}

pub(crate) proof fn fence_advances_generation_once(old_generation: int)
    ensures model::generation_is_fenced(old_generation, old_generation + 1),
{
}

pub(crate) proof fn accepted_boundary_fence_is_terminal(
    before: &LeaseAggregate,
    after: &LeaseAggregate,
    record: LeaseTransitionRecord,
    command_id: peritus_types::CommandId,
    normal_kind: LeaseTransitionKind,
    cause: Option<crate::FenceCause>,
)
    requires
        model::concrete_fence_decision(
            before,
            after,
            record,
            command_id,
            normal_kind,
            cause,
        ),
        after.version.spec_value() >= (u64::MAX - 1) as int,
    ensures matches!(
        after.state,
        crate::state::LeaseState::Retired(crate::RetirementReason::VersionExhausted),
    ),
{
    model::concrete::fencing::boundary_fence_is_terminal(
        before,
        after,
        record,
        command_id,
        normal_kind,
        cause,
    );
}

pub(crate) proof fn concrete_fence_invalidates_previous_claim(
    before: &LeaseAggregate,
    after: &LeaseAggregate,
    record: LeaseTransitionRecord,
    previous_claim: LeaseClaim,
)
    requires
        model::concrete_claim_is_current(before, previous_claim),
        model::concrete_fence_edge(before, after, record),
    ensures !model::concrete_claim_is_current(after, previous_claim),
{
    model::concrete::fencing::fence_invalidates_claim(before, after, record, previous_claim);
}

pub(crate) proof fn normal_release_is_a_fence()
    ensures model::concrete_fencing_kind(LeaseTransitionKind::ReleasedAvailable),
{
    model::concrete::fencing::establish_fencing_kind(
        LeaseTransitionKind::ReleasedAvailable,
    );
}

pub(crate) proof fn release_without_quiescence_is_a_fence()
    ensures model::concrete_fencing_kind(LeaseTransitionKind::ReleasedReconciling),
{
    model::concrete::fencing::establish_fencing_kind(
        LeaseTransitionKind::ReleasedReconciling,
    );
}

pub(crate) proof fn expiry_is_a_fence()
    ensures model::concrete_fencing_kind(LeaseTransitionKind::Expired),
{
    model::concrete::fencing::establish_fencing_kind(LeaseTransitionKind::Expired);
}

pub(crate) proof fn holder_loss_is_a_fence()
    ensures model::concrete_fencing_kind(LeaseTransitionKind::HolderLost),
{
    model::concrete::fencing::establish_fencing_kind(LeaseTransitionKind::HolderLost);
}

pub(crate) proof fn clock_discontinuity_is_a_fence()
    ensures model::concrete_fencing_kind(LeaseTransitionKind::ClockDiscontinuity),
{
    model::concrete::fencing::establish_fencing_kind(
        LeaseTransitionKind::ClockDiscontinuity,
    );
}

pub(crate) proof fn revocation_is_a_fence()
    ensures model::concrete_fencing_kind(LeaseTransitionKind::Revoked),
{
    model::concrete::fencing::establish_fencing_kind(LeaseTransitionKind::Revoked);
}

pub(crate) proof fn reconciliation_never_creates_an_active_holder(
    before: &LeaseAggregate,
    after: &LeaseAggregate,
    record: LeaseTransitionRecord,
    command: crate::ReconcileLease,
)
    requires model::concrete_reconcile_edge(before, after, record, command),
    ensures !matches!(after.state, crate::state::LeaseState::Active(_)),
{
    model::concrete::fencing::reconcile_is_not_active(before, after, record, command);
}

} // verus!
