//! Linear authority-time advancement and exact transition construction.

use super::validation::map_policy_time;
use super::{
    LeaseTransition, LeaseTransitionKind, LeaseTransitionOutcome, LeaseTransitionRecord,
};
use crate::state::LeaseState;
use crate::{LeaseAggregate, LeaseError, LeaseTransitionFailure};
use peritus_policy::{AuthorityInstant, AuthorityTimeState};
use peritus_types::{CommandId, Generation, RevisionNumber};
use vstd::prelude::*;

verus! {

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AuthorityTimeAdvance {
    Observe(AuthorityInstant),
    Preserve,
    Reset(AuthorityInstant),
}

/// Complete private input to one exact state-edge constructor.
pub(super) struct TransitionPlan {
    pub(super) command_id: CommandId,
    pub(super) version: RevisionNumber,
    pub(super) generation: Generation,
    pub(super) time_advance: AuthorityTimeAdvance,
    pub(super) state: LeaseState,
    pub(super) kind: LeaseTransitionKind,
    pub(super) binding: crate::LeaseCommandBinding,
}

impl TransitionPlan {
    pub(super) const fn new(
        command_id: CommandId,
        version: RevisionNumber,
        generation: Generation,
        time_advance: AuthorityTimeAdvance,
        state: LeaseState,
        kind: LeaseTransitionKind,
        binding: crate::LeaseCommandBinding,
    ) -> (plan: Self)
        ensures
            plan.command_id == command_id,
            plan.version == version,
            plan.generation == generation,
            plan.time_advance == time_advance,
            plan.state == state,
            plan.kind == kind,
            plan.binding == binding,
    {
        Self { command_id, version, generation, time_advance, state, kind, binding }
    }

    fn into_parts(
        self,
    ) -> (parts: (
        CommandId,
        RevisionNumber,
        Generation,
        AuthorityTimeAdvance,
        LeaseState,
        LeaseTransitionKind,
        crate::LeaseCommandBinding,
    ))
        ensures
            parts.0 == self.command_id,
            parts.1 == self.version,
            parts.2 == self.generation,
            parts.3 == self.time_advance,
            parts.4 == self.state,
            parts.5 == self.kind,
            parts.6 == self.binding,
    {
        (
            self.command_id,
            self.version,
            self.generation,
            self.time_advance,
            self.state,
            self.kind,
            self.binding,
        )
    }
}

pub(super) open spec fn authority_time_advance_matches(
    before: &LeaseAggregate,
    after: &LeaseAggregate,
    advance: AuthorityTimeAdvance,
) -> bool {
    match advance {
        AuthorityTimeAdvance::Observe(observed_at) => {
            crate::model::concrete_time_observed(before, after, observed_at)
        }
        AuthorityTimeAdvance::Preserve => after.authority_time == before.authority_time,
        AuthorityTimeAdvance::Reset(observed_at) => {
            after.authority_time.spec_epoch() == observed_at.spec_epoch()
                && after.authority_time.spec_greatest_tick_millis()
                    == observed_at.spec_tick_millis()
        }
    }
}

pub(super) open spec fn transition_result_matches(
    before: &LeaseAggregate,
    result: LeaseTransitionOutcome,
    command_id: CommandId,
    version: RevisionNumber,
    generation: Generation,
    time_advance: AuthorityTimeAdvance,
    state: LeaseState,
    kind: LeaseTransitionKind,
    binding: crate::LeaseCommandBinding,
) -> bool {
    match (transition_error(before, version, time_advance), result) {
        (None, LeaseTransitionOutcome::Accepted(accepted)) => {
            crate::model::concrete_transition_matches(
                before,
                &accepted.next,
                accepted.record,
                command_id,
                generation,
                state,
                kind,
                binding,
            ) && authority_time_advance_matches(before, &accepted.next, time_advance)
        }
        (Some(error), LeaseTransitionOutcome::Rejected(failure)) => {
            crate::model::concrete::rejections::exact_transition_rejection(
                before,
                &failure,
                error,
            )
        }
        _ => false,
    }
}

pub(super) open spec fn transition_result_projection(
    before: &LeaseAggregate,
    result: LeaseTransitionOutcome,
    command_id: CommandId,
    generation: Generation,
    time_advance: AuthorityTimeAdvance,
    state: LeaseState,
    kind: LeaseTransitionKind,
    binding: crate::LeaseCommandBinding,
) -> bool {
    match result {
        LeaseTransitionOutcome::Accepted(accepted) => crate::model::concrete_transition_matches(
            before,
            &accepted.next,
            accepted.record,
            command_id,
            generation,
            state,
            kind,
            binding,
        ) && authority_time_advance_matches(before, &accepted.next, time_advance),
        LeaseTransitionOutcome::Rejected(failure) => {
            crate::model::concrete_rejection_preserves_input(before, &failure)
        }
    }
}

pub(super) open spec fn transition_error(
    before: &LeaseAggregate,
    version: RevisionNumber,
    time_advance: AuthorityTimeAdvance,
) -> Option<LeaseError> {
    if before.version.spec_value() == u64::MAX as int
        || version.spec_value() != before.version.spec_value() + 1
        || !before.internal_is_valid()
    {
        Some(LeaseError::CorruptState)
    } else {
        match time_advance {
            AuthorityTimeAdvance::Observe(observed_at) => {
                super::validation::observation_error(&before.authority_time, observed_at)
            }
            AuthorityTimeAdvance::Preserve | AuthorityTimeAdvance::Reset(_) => None,
        }
    }
}

pub(super) fn transition(
    before: LeaseAggregate,
    plan: TransitionPlan,
) -> (result: LeaseTransitionOutcome)
    ensures
        transition_result_matches(
            &before,
            result,
            plan.command_id,
            plan.version,
            plan.generation,
            plan.time_advance,
            plan.state,
            plan.kind,
            plan.binding,
        ),
        transition_result_projection(
            &before,
            result,
            plan.command_id,
            plan.generation,
            plan.time_advance,
            plan.state,
            plan.kind,
            plan.binding,
        ),
{
    let expected = match before.version.checked_next() {
        Ok(value) => value,
        Err(_error) => {
            assert(transition_error(&before, plan.version, plan.time_advance)
                == Some(LeaseError::CorruptState));
            return LeaseTransitionOutcome::Rejected(rejection(
                before,
                LeaseError::CorruptState,
            ));
        }
    };
    if plan.version.get() != expected.get() {
        assert(transition_error(&before, plan.version, plan.time_advance)
            == Some(LeaseError::CorruptState));
        return LeaseTransitionOutcome::Rejected(rejection(
            before,
            LeaseError::CorruptState,
        ));
    }
    assert(plan.version.spec_value() == before.version.spec_value() + 1);
    transition_verified(before, plan)
}

fn transition_verified(
    before: LeaseAggregate,
    plan: TransitionPlan,
) -> (result: LeaseTransitionOutcome)
    requires plan.version.spec_value() == before.version.spec_value() + 1,
    ensures
        transition_result_matches(
            &before,
            result,
            plan.command_id,
            plan.version,
            plan.generation,
            plan.time_advance,
            plan.state,
            plan.kind,
            plan.binding,
        ),
        transition_result_projection(
            &before,
            result,
            plan.command_id,
            plan.generation,
            plan.time_advance,
            plan.state,
            plan.kind,
            plan.binding,
        ),
{
    let (command_id, version, generation, time_advance, state, kind, binding) = plan.into_parts();
    let ghost before_view = before;
    let _version_value = version.get();
    let _before_version_value = before.version.get();
    proof {
        super::core_proofs::establish_transition_inputs(
            &before_view,
            version,
            _version_value,
            _before_version_value,
        );
    }
    if let Err(error) = before.validate() {
        let failure = LeaseTransitionFailure::new(error, before);
        proof {
            super::core_proofs::establish_corrupt_transition_rejection(
                &before_view,
                version,
                time_advance,
                error,
                &failure,
            );
        }
        return LeaseTransitionOutcome::Rejected(failure);
    }
    let before_phase = before.checked_phase();
    let LeaseAggregate {
        scope: aggregate_scope,
        generation: before_generation,
        version: before_version,
        authority_time: time_floor,
        state: before_state,
    } = before;
    let authority_time = match time_advance {
        AuthorityTimeAdvance::Observe(observed_at) => match time_floor.observe(observed_at) {
            Ok(next) => next,
            Err(time_failure) => {
                assert(time_advance == AuthorityTimeAdvance::Observe(observed_at));
                let error = map_policy_time(time_failure.error());
                let restored = LeaseAggregate::from_parts(
                    aggregate_scope,
                    before_generation,
                    before_version,
                    time_failure.into_state(),
                    before_state,
                );
                let failure = LeaseTransitionFailure::new(error, restored);
                proof {
                    super::core_proofs::establish_time_transition_rejection(
                        &before_view,
                        version,
                        time_advance,
                        observed_at,
                        &time_failure,
                        error,
                        &failure,
                    );
                }
                return LeaseTransitionOutcome::Rejected(failure);
            }
        },
        AuthorityTimeAdvance::Preserve => time_floor,
        AuthorityTimeAdvance::Reset(observed_at) => AuthorityTimeState::new(observed_at),
    };
    let next = LeaseAggregate::from_parts(
        aggregate_scope,
        generation,
        version,
        authority_time,
        state,
    );
    let record = LeaseTransitionRecord {
        command_id,
        scope: next.scope,
        before_version: Some(before_version),
        after_version: next.version,
        before_generation: Some(before_generation),
        after_generation: next.generation,
        before_phase: Some(before_phase),
        after_phase: next.checked_phase(),
        kind,
        binding: Box::new(binding),
    };
    let accepted = LeaseTransition { next, record };
    proof {
        super::core_proofs::establish_accepted_transition(
            &before_view,
            &accepted,
            command_id,
            version,
            generation,
            time_advance,
            state,
            kind,
            binding,
        );
    }
    LeaseTransitionOutcome::Accepted(accepted)
}

pub(super) const fn rejection(
    before: LeaseAggregate,
    error: LeaseError,
) -> (failure: LeaseTransitionFailure)
    ensures
        failure.spec_error() == error,
        crate::model::concrete_rejection_preserves_input(&before, &failure),
{
    let ghost before_view = before;
    let failure = LeaseTransitionFailure::new(error, before);
    proof {
        assert(crate::model::concrete_snapshot_preserved(
            &before_view,
            &failure.spec_aggregate(),
        ));
        crate::model::concrete::establish_rejection_preservation(&before_view, &failure);
    }
    failure
}

} // verus!
