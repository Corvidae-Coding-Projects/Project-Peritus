//! Linear authority-time advancement and exact transition construction.

use super::validation::map_policy_time;
use super::{LeaseTransition, LeaseTransitionKind, LeaseTransitionOutcome, LeaseTransitionRecord};
use crate::state::LeaseState;
use crate::{LeaseAggregate, LeaseError, LeaseTransitionFailure};
use peritus_policy::{AuthorityInstant, AuthorityTimeState};
use peritus_types::{CommandId, Generation, RevisionNumber};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AuthorityTimeAdvance {
    Observe(AuthorityInstant),
    Preserve,
    Reset(AuthorityInstant),
}
#[doc = " Complete private input to one exact state-edge constructor."]
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
    ) -> Self {
        Self { command_id, version, generation, time_advance, state, kind, binding }
    }
    fn into_parts(
        self,
    ) -> (
        CommandId,
        RevisionNumber,
        Generation,
        AuthorityTimeAdvance,
        LeaseState,
        LeaseTransitionKind,
        crate::LeaseCommandBinding,
    ) {
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
pub(super) fn transition(before: LeaseAggregate, plan: TransitionPlan) -> LeaseTransitionOutcome {
    let expected = match before.version.checked_next() {
        Ok(value) => value,
        Err(_error) => {
            return LeaseTransitionOutcome::Rejected(rejection(before, LeaseError::CorruptState));
        }
    };
    if plan.version.get() != expected.get() {
        return LeaseTransitionOutcome::Rejected(rejection(before, LeaseError::CorruptState));
    }
    transition_verified(before, plan)
}
fn transition_verified(before: LeaseAggregate, plan: TransitionPlan) -> LeaseTransitionOutcome {
    let (command_id, version, generation, time_advance, state, kind, binding) = plan.into_parts();
    let _version_value = version.get();
    let _before_version_value = before.version.get();
    if let Err(error) = before.validate() {
        let failure = LeaseTransitionFailure::new(error, before);
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
                let error = map_policy_time(time_failure.error());
                let restored = LeaseAggregate::from_parts(
                    aggregate_scope,
                    before_generation,
                    before_version,
                    time_failure.into_state(),
                    before_state,
                );
                let failure = LeaseTransitionFailure::new(error, restored);
                return LeaseTransitionOutcome::Rejected(failure);
            }
        },
        AuthorityTimeAdvance::Preserve => time_floor,
        AuthorityTimeAdvance::Reset(observed_at) => AuthorityTimeState::new(observed_at),
    };
    let next =
        LeaseAggregate::from_parts(aggregate_scope, generation, version, authority_time, state);
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
    LeaseTransitionOutcome::Accepted(accepted)
}
pub(super) const fn rejection(before: LeaseAggregate, error: LeaseError) -> LeaseTransitionFailure {
    let failure = LeaseTransitionFailure::new(error, before);
    failure
}
