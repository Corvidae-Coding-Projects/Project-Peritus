//! Terminal fencing transition when version or generation space is exhausted.

#[cfg(verus_only)]
use super::fencing_model::fence_time_advance_matches;
use super::fencing_apply::FencePlan;
use super::{
    transition, LeaseAggregate, LeaseState, LeaseTransitionKind, TransitionPlan,
};
use crate::{LeaseTransitionOutcome, RetirementReason};
use vstd::prelude::*;

verus! {

pub(super) fn retire(
    before: LeaseAggregate,
    version: peritus_types::RevisionNumber,
    generation: peritus_types::Generation,
    reason: RetirementReason,
    plan: FencePlan,
) -> (result: LeaseTransitionOutcome)
    requires
        version.spec_value() == before.version.spec_value() + 1,
        generation == before.generation,
        matches!(before.state, LeaseState::Active(_)),
        fence_time_advance_matches(
            &before,
            plan.time_advance,
            plan.observed_at,
            plan.reconciliation_cause,
        ),
        reason == RetirementReason::VersionExhausted
            ==> version.spec_value() >= (u64::MAX - 1) as int,
        reason == RetirementReason::GenerationExhausted
            ==> version.spec_value() < (u64::MAX - 1) as int,
        reason == RetirementReason::GenerationExhausted
            ==> before.generation.spec_value() == u64::MAX as int,
    ensures
        match result {
            LeaseTransitionOutcome::Accepted(accepted) => {
                crate::model::concrete::rejections::fencing::final_fence_error(&before).is_none()
                    && crate::model::concrete_fence_decision(
                        &before,
                        &accepted.next,
                        accepted.record,
                        plan.command_id,
                        plan.kind,
                        plan.reconciliation_cause,
                    )
                    && crate::model::concrete_fence_time_observed(
                        &before,
                        &accepted.next,
                        plan.observed_at,
                        plan.reconciliation_cause,
                    )
                    && *accepted.record.binding == plan.binding
            }
            LeaseTransitionOutcome::Rejected(failure) => {
                crate::model::concrete::rejections::fencing::final_fence_error(&before)
                    == Some(failure.spec_error())
                    && crate::model::concrete_rejection_preserves_input(&before, &failure)
            }
        },
{
    let (command_id, time_advance, _observed_at, _cause, _normal_kind, binding) =
        plan.into_parts();
    let ghost before_view = before;
    let accepted = match transition(before, TransitionPlan::new(
        command_id,
        version,
        generation,
        time_advance,
        LeaseState::Retired(reason),
        LeaseTransitionKind::Retired(reason),
        binding,
    )) {
        LeaseTransitionOutcome::Accepted(accepted) => accepted,
        LeaseTransitionOutcome::Rejected(failure) => {
            return LeaseTransitionOutcome::Rejected(failure);
        }
    };
    proof {
        assert(accepted.next.generation == before_view.generation);
        assert(accepted.next.state == LeaseState::Retired(reason));
        assert(accepted.record.command_id == command_id);
        assert(accepted.record.kind == LeaseTransitionKind::Retired(reason));
        assert(crate::model::concrete_record_matches(
            &before_view,
            &accepted.next,
            accepted.record,
        ));
        assert(crate::model::concrete_refines_reachability_step(
            &before_view,
            &accepted.next,
        ));
        match reason {
            RetirementReason::VersionExhausted => {
                assert(before_view.version.spec_value() + 1
                    >= (u64::MAX - 1) as int);
            }
            RetirementReason::GenerationExhausted => {
                assert(before_view.version.spec_value() + 1
                    < (u64::MAX - 1) as int);
                assert(before_view.generation.spec_value() == u64::MAX as int);
            }
        }
        assert(crate::model::concrete_fence_decision(
            &before_view,
            &accepted.next,
            accepted.record,
            command_id,
            _normal_kind,
            _cause,
        ));
        assert(crate::model::concrete_fence_time_observed(
            &before_view,
            &accepted.next,
            _observed_at,
            _cause,
        ));
    }
    LeaseTransitionOutcome::Accepted(accepted)
}

} // verus!
