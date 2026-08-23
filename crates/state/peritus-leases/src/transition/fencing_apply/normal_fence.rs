//! Proof that a non-retiring fence plan realizes its exact concrete edge.

use crate::state::{ActiveLease, LeaseState};
use crate::{FenceCause, LeaseAggregate, LeaseTransition, LeaseTransitionKind};
use peritus_types::{CommandId, Generation, RevisionNumber};
use vstd::prelude::*;

verus! {

pub(super) open spec fn normal_fence_state_matches(
    before: &LeaseAggregate,
    active: ActiveLease,
    state: LeaseState,
    cause: Option<FenceCause>,
) -> bool {
    match (cause, state) {
        (None, LeaseState::Available) => true,
        (Some(expected_cause), LeaseState::Reconciling(reconciling)) => {
            reconciling.cause == expected_cause
                && reconciling.correlation.spec_scope() == before.scope
                && reconciling.correlation.spec_fenced_generation() == before.generation
                && reconciling.correlation.spec_prior_holder() == active.holder
        }
        _ => false,
    }
}

pub(super) proof fn establish_normal_fence_decision(
    before: &LeaseAggregate,
    accepted: &LeaseTransition,
    command_id: CommandId,
    version: RevisionNumber,
    generation: Generation,
    state: LeaseState,
    kind: LeaseTransitionKind,
    cause: Option<FenceCause>,
    active: ActiveLease,
)
    requires
        before.state == LeaseState::Active(active),
        version.spec_value() == before.version.spec_value() + 1,
        version.spec_value() < (u64::MAX - 1) as int,
        generation.spec_value() == before.generation.spec_value() + 1,
        before.generation.spec_value() < u64::MAX as int,
        accepted.next.generation == generation,
        accepted.next.state == state,
        accepted.record.command_id == command_id,
        accepted.record.kind == kind,
        crate::model::concrete_record_matches(before, &accepted.next, accepted.record),
        crate::model::concrete_refines_reachability_step(before, &accepted.next),
        crate::model::concrete_fencing_kind(kind),
        normal_fence_state_matches(before, active, state, cause),
    ensures crate::model::concrete_fence_decision(
        before,
        &accepted.next,
        accepted.record,
        command_id,
        kind,
        cause,
    ),
{
    assert(crate::model::concrete_fence_edge(
        before,
        &accepted.next,
        accepted.record,
    ));
    match (cause, state) {
        (None, LeaseState::Available) => {}
        (Some(expected_cause), LeaseState::Reconciling(reconciling)) => {
            assert(reconciling.cause == expected_cause);
        }
        _ => {
            assert(false);
        }
    }
}

} // verus!
