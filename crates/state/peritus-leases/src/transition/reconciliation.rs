//! Exactly correlated reconciliation and terminal retirement reducer.

mod correlation;

use self::correlation::validate_correlation;

use super::{
    AuthorityTimeAdvance, LeaseAggregate, LeaseError, LeaseState, LeaseTransitionKind,
    TransitionPlan, next_non_fence_version, rejection, transition, validate_observation,
};
use crate::state::QuarantinedState;
use crate::{
    FenceCause, LeasePhase, LeaseTransitionOutcome, ReconcileLease, ReconciliationDisposition,
    RetirementReason,
};
use peritus_policy::AuthorityInstant;

impl LeaseAggregate {
    #[doc = " Resolves one exactly correlated fenced generation."]
    #[doc = ""]
    #[doc = " At the non-fencing version boundary, a valid observation retires the aggregate instead of"]
    #[doc = " leaving an available or reconciling state with no representable safe successor."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a typed phase, correlation, authority-time, version, or state failure."]
    pub fn reconcile(self, command: ReconcileLease) -> LeaseTransitionOutcome {
        let LeaseState::Reconciling(reconciling) = self.state else {
            let actual = self.checked_phase();
            let error = LeaseError::IllegalPhase { expected: LeasePhase::Reconciling, actual };
            return reconciliation_rejection(self, command, error);
        };
        if let Err(error) =
            validate_correlation(reconciling.correlation, command.observation.correlation)
        {
            return reconciliation_rejection(self, command, error);
        }
        let time_advance =
            match reconciliation_time_advance(&self, reconciling.cause, command.observed_at()) {
                Ok(value) => value,
                Err(error) => {
                    return reconciliation_rejection(self, command, error);
                }
            };
        let version = match next_non_fence_version(self.version) {
            Ok(value) => value,
            Err(error) => {
                return reconciliation_rejection(self, command, error);
            }
        };
        let (state, kind) = if version.get() >= u64::MAX - 1 {
            let reason = RetirementReason::VersionExhausted;
            (LeaseState::Retired(reason), LeaseTransitionKind::Retired(reason))
        } else {
            match command.observation.disposition {
                ReconciliationDisposition::SafeToAcquire { .. } => {
                    (LeaseState::Available, LeaseTransitionKind::ReconciledAvailable)
                }
                disposition @ (ReconciliationDisposition::Dirty { .. }
                | ReconciliationDisposition::Indeterminate { .. }) => (
                    LeaseState::Quarantined(QuarantinedState {
                        correlation: reconciling.correlation,
                        cause: reconciling.cause,
                        disposition,
                    }),
                    LeaseTransitionKind::ReconciledQuarantined,
                ),
            }
        };
        let command_id = command.command_id();
        let generation = self.generation;
        let binding = crate::LeaseCommandBinding::reconcile(&command);
        let accepted = match transition(
            self,
            TransitionPlan::new(
                command_id,
                version,
                generation,
                time_advance,
                state,
                kind,
                binding,
            ),
        ) {
            LeaseTransitionOutcome::Accepted(accepted) => accepted,
            LeaseTransitionOutcome::Rejected(failure) => {
                return LeaseTransitionOutcome::Rejected(failure);
            }
        };
        LeaseTransitionOutcome::Accepted(accepted)
    }
}
const fn reconciliation_rejection(
    aggregate: LeaseAggregate,
    _command: ReconcileLease,
    error: LeaseError,
) -> LeaseTransitionOutcome {
    let failure = rejection(aggregate, error);
    LeaseTransitionOutcome::Rejected(failure)
}
const fn reconciliation_time_advance(
    aggregate: &LeaseAggregate,
    cause: FenceCause,
    observed_at: AuthorityInstant,
) -> Result<AuthorityTimeAdvance, LeaseError> {
    let observed_generation = observed_at.epoch();
    let floor_generation = aggregate.authority_time.epoch();
    let observed_epoch = observed_generation.get();
    let floor_epoch = floor_generation.get();
    match cause {
        FenceCause::ClockDiscontinuity if observed_epoch != floor_epoch => {
            Ok(AuthorityTimeAdvance::Reset(observed_at))
        }
        _ => match validate_observation(&aggregate.authority_time, observed_at) {
            Ok(()) => Ok(AuthorityTimeAdvance::Observe(observed_at)),
            Err(error) => Err(error),
        },
    }
}
