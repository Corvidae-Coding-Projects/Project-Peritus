//! Shared fence application after command-specific validation.

use super::{
    AuthorityTimeAdvance, LeaseAggregate, LeaseError, LeaseState, LeaseTransitionKind,
    TransitionPlan, rejection, transition, validate_observation,
};

use super::fencing_retire::retire;
use crate::state::{ActiveLease, ReconciliationState};
use crate::{FenceCause, LeaseTransitionOutcome, ReconciliationCorrelation, RetirementReason};
use peritus_policy::AuthorityInstant;
use peritus_types::CommandId;

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
    ) -> Self {
        Self { command_id, time_advance, observed_at, reconciliation_cause, kind, binding }
    }
    pub(super) fn into_parts(
        self,
    ) -> (
        CommandId,
        AuthorityTimeAdvance,
        AuthorityInstant,
        Option<FenceCause>,
        LeaseTransitionKind,
        crate::LeaseCommandBinding,
    ) {
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
) -> LeaseTransitionOutcome {
    let LeaseState::Active(active) = before.state else {
        return LeaseTransitionOutcome::Rejected(rejection(before, LeaseError::CorruptState));
    };
    if !fencing_kind_is_valid(kind) {
        return LeaseTransitionOutcome::Rejected(rejection(before, LeaseError::CorruptState));
    }
    let time_advance = match reconciliation_cause {
        Some(FenceCause::ClockDiscontinuity) => {
            let observed_epoch = observed_at.epoch().get();
            let floor_epoch = before.authority_time.epoch().get();
            if observed_epoch == floor_epoch {
                if observed_at.tick_millis() >= before.authority_time.greatest_tick_millis() {
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
                return LeaseTransitionOutcome::Rejected(rejection(before, error));
            }
            AuthorityTimeAdvance::Observe(observed_at)
        }
    };
    fence_verified(
        before,
        active,
        FencePlan::new(command_id, time_advance, observed_at, reconciliation_cause, kind, binding),
    )
}
fn fence_verified(
    before: LeaseAggregate,
    active: ActiveLease,
    plan: FencePlan,
) -> LeaseTransitionOutcome {
    let (command_id, time_advance, observed_at, reconciliation_cause, kind, binding) =
        plan.into_parts();
    let version = match before.version.checked_next() {
        Ok(value) => value,
        Err(_error) => {
            return LeaseTransitionOutcome::Rejected(rejection(before, LeaseError::CorruptState));
        }
    };
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
    let accepted = match transition(
        before,
        TransitionPlan::new(command_id, version, generation, time_advance, state, kind, binding),
    ) {
        LeaseTransitionOutcome::Accepted(accepted) => accepted,
        LeaseTransitionOutcome::Rejected(failure) => {
            return LeaseTransitionOutcome::Rejected(failure);
        }
    };
    LeaseTransitionOutcome::Accepted(accepted)
}
const fn fencing_kind_is_valid(kind: LeaseTransitionKind) -> bool {
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
