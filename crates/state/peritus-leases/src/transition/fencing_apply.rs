//! Shared fence application after command-specific validation.

#[cfg(verus_only)]
mod normal_fence;

#[cfg(verus_only)]
use self::normal_fence::establish_normal_fence_decision;

use super::{
    rejection, transition, validate_observation, AuthorityTimeAdvance, LeaseAggregate, LeaseError,
    LeaseState, LeaseTransitionKind, TransitionPlan,
};
#[cfg(verus_only)]
use super::fencing_model::{fence_error, fence_time_advance_matches,
    transition_error_is_final_fence_error};
use super::fencing_retire::retire;
use crate::state::{ActiveLease, ReconciliationState};
use crate::{
    FenceCause, LeaseTransitionOutcome, ReconciliationCorrelation, RetirementReason,
};
use peritus_policy::AuthorityInstant;
use peritus_types::CommandId;
use vstd::prelude::*;

verus! {

pub(super) struct FencePlan {
    pub(super) command_id: CommandId,
    pub(super) time_advance: AuthorityTimeAdvance,
    pub(super) observed_at: AuthorityInstant,
    pub(super) reconciliation_cause: Option<FenceCause>,
    pub(super) kind: LeaseTransitionKind,
    pub(super) binding: crate::LeaseCommandBinding,
}

impl FencePlan {
    const fn new(
        command_id: CommandId,
        time_advance: AuthorityTimeAdvance,
        observed_at: AuthorityInstant,
        reconciliation_cause: Option<FenceCause>,
        kind: LeaseTransitionKind,
        binding: crate::LeaseCommandBinding,
    ) -> (plan: Self)
        ensures
            plan.command_id == command_id,
            plan.time_advance == time_advance,
            plan.observed_at == observed_at,
            plan.reconciliation_cause == reconciliation_cause,
            plan.kind == kind,
            plan.binding == binding,
    {
        Self { command_id, time_advance, observed_at, reconciliation_cause, kind, binding }
    }

    pub(super) fn into_parts(
        self,
    ) -> (parts: (
        CommandId,
        AuthorityTimeAdvance,
        AuthorityInstant,
        Option<FenceCause>,
        LeaseTransitionKind,
        crate::LeaseCommandBinding,
    ))
        ensures
            parts.0 == self.command_id,
            parts.1 == self.time_advance,
            parts.2 == self.observed_at,
            parts.3 == self.reconciliation_cause,
            parts.4 == self.kind,
            parts.5 == self.binding,
    {
        (
            self.command_id,
            self.time_advance,
            self.observed_at,
            self.reconciliation_cause,
            self.kind,
            self.binding,
        )
    }
}

pub(super) fn fence(
    before: LeaseAggregate,
    command_id: CommandId,
    observed_at: AuthorityInstant,
    reconciliation_cause: Option<FenceCause>,
    kind: LeaseTransitionKind,
    binding: crate::LeaseCommandBinding,
) -> (result: LeaseTransitionOutcome)
    ensures
        match result {
            LeaseTransitionOutcome::Accepted(accepted) => {
                fence_error(&before, observed_at, reconciliation_cause, kind).is_none()
                    && crate::model::concrete_fence_decision(
                    &before,
                    &accepted.next,
                    accepted.record,
                    command_id,
                    kind,
                    reconciliation_cause,
                ) && crate::model::concrete_fence_time_observed(
                    &before,
                    &accepted.next,
                    observed_at,
                    reconciliation_cause,
                )
                    && *accepted.record.binding == binding
            }
            LeaseTransitionOutcome::Rejected(failure) => {
                fence_error(&before, observed_at, reconciliation_cause, kind)
                    == Some(failure.spec_error())
                    && crate::model::concrete_rejection_preserves_input(&before, &failure)
            }
        },
{
    let LeaseState::Active(active) = before.state else {
        assert(fence_error(&before, observed_at, reconciliation_cause, kind)
            == Some(LeaseError::CorruptState));
        return LeaseTransitionOutcome::Rejected(rejection(
            before,
            LeaseError::CorruptState,
        ));
    };
    if !fencing_kind_is_valid(kind) {
        assert(fence_error(&before, observed_at, reconciliation_cause, kind)
            == Some(LeaseError::CorruptState));
        return LeaseTransitionOutcome::Rejected(rejection(
            before,
            LeaseError::CorruptState,
        ));
    }
    let time_advance = match reconciliation_cause {
        Some(FenceCause::ClockDiscontinuity) => {
            let observed_epoch = observed_at.epoch().get();
            let floor_epoch = before.authority_time.epoch().get();
            if observed_epoch == floor_epoch {
                if observed_at.tick_millis()
                    >= before.authority_time.greatest_tick_millis()
                {
                    assert(fence_error(&before, observed_at, reconciliation_cause, kind)
                        == Some(LeaseError::NoClockDiscontinuity));
                    return LeaseTransitionOutcome::Rejected(rejection(
                        before,
                        LeaseError::NoClockDiscontinuity,
                    ));
                }
                AuthorityTimeAdvance::Preserve
            } else {
                AuthorityTimeAdvance::Reset(observed_at)
            }
        }
        None
        | Some(
            FenceCause::ReleasedWithoutQuiescence
            | FenceCause::Expired
            | FenceCause::HolderLost
            | FenceCause::Revoked,
        ) => {
            if let Err(error) = validate_observation(&before.authority_time, observed_at) {
                assert(fence_error(&before, observed_at, reconciliation_cause, kind)
                    == Some(error));
                return LeaseTransitionOutcome::Rejected(rejection(before, error));
            }
            AuthorityTimeAdvance::Observe(observed_at)
        }
    };
    assert(before.state == LeaseState::Active(active));
    assert(fence_time_advance_matches(
        &before,
        time_advance,
        observed_at,
        reconciliation_cause,
    ));
    assert(crate::model::concrete_fencing_kind(kind));
    fence_verified(
        before,
        active,
        FencePlan::new(
            command_id,
            time_advance,
            observed_at,
            reconciliation_cause,
            kind,
            binding,
        ),
    )
}

fn fence_verified(
    before: LeaseAggregate,
    active: ActiveLease,
    plan: FencePlan,
) -> (result: LeaseTransitionOutcome)
    requires
        before.state == LeaseState::Active(active),
        fence_time_advance_matches(
            &before,
            plan.time_advance,
            plan.observed_at,
            plan.reconciliation_cause,
        ),
        crate::model::concrete_fencing_kind(plan.kind),
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
                ) && crate::model::concrete_fence_time_observed(
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
    let (command_id, time_advance, observed_at, reconciliation_cause, kind, binding) =
        plan.into_parts();
    let ghost before_view = before;
    let version = match before.version.checked_next() {
        Ok(value) => value,
        Err(_error) => {
            assert(crate::model::concrete::rejections::fencing::final_fence_error(&before)
                == Some(LeaseError::CorruptState));
            return LeaseTransitionOutcome::Rejected(rejection(
                before,
                LeaseError::CorruptState,
            ));
        }
    };
    proof {
        assert(before.version.spec_value() < u64::MAX as int);
        transition_error_is_final_fence_error(
            &before,
            version,
            time_advance,
            observed_at,
            reconciliation_cause,
        );
    }
    if version.get() >= u64::MAX - 1 {
        let generation = before.generation;
        return retire(
            before,
            version,
            generation,
            RetirementReason::VersionExhausted,
            FencePlan::new(
                command_id,
                time_advance,
                observed_at,
                reconciliation_cause,
                kind,
                binding,
            ),
        );
    }
    let generation = match before.generation.checked_next() {
        Ok(value) => value,
        Err(_error) => {
            assert(before.generation.spec_value() == u64::MAX as int);
            let generation = before.generation;
            return retire(
                before,
                version,
                generation,
                RetirementReason::GenerationExhausted,
                FencePlan::new(
                    command_id,
                    time_advance,
                    observed_at,
                    reconciliation_cause,
                    kind,
                    binding,
                ),
            );
        }
    };
    let state = match reconciliation_cause {
        None => LeaseState::Available,
        Some(cause) => LeaseState::Reconciling(ReconciliationState {
            correlation: ReconciliationCorrelation::new(
                before.scope,
                before.generation,
                active.holder,
            ),
            cause,
        }),
    };
    let accepted = match transition(before, TransitionPlan::new(
        command_id,
        version,
        generation,
        time_advance,
        state,
        kind,
        binding,
    )) {
        LeaseTransitionOutcome::Accepted(accepted) => accepted,
        LeaseTransitionOutcome::Rejected(failure) =>
            return LeaseTransitionOutcome::Rejected(failure),
    };
    proof {
        establish_normal_fence_decision(
            &before_view,
            &accepted,
            command_id,
            version,
            generation,
            state,
            kind,
            reconciliation_cause,
            active,
        );
    }
    LeaseTransitionOutcome::Accepted(accepted)
}

const fn fencing_kind_is_valid(kind: LeaseTransitionKind) -> (result: bool)
    ensures result == crate::model::concrete_fencing_kind(kind),
{
    match kind {
        LeaseTransitionKind::ReleasedAvailable
        | LeaseTransitionKind::ReleasedReconciling
        | LeaseTransitionKind::Expired
        | LeaseTransitionKind::HolderLost
        | LeaseTransitionKind::ClockDiscontinuity
        | LeaseTransitionKind::Revoked
        | LeaseTransitionKind::Retired(_) => true,
        LeaseTransitionKind::Minted
        | LeaseTransitionKind::Acquired
        | LeaseTransitionKind::Renewed
        | LeaseTransitionKind::Used { .. }
        | LeaseTransitionKind::ReconciledAvailable
        | LeaseTransitionKind::ReconciledQuarantined => false,
    }
}

} // verus!
