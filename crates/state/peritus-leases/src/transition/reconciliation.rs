//! Exactly correlated reconciliation and terminal retirement reducer.

mod correlation;

#[cfg(verus_only)]
pub(crate) use self::correlation::correlation_error;
use self::correlation::validate_correlation;

use super::{
    next_non_fence_version, rejection, transition, validate_observation,
    AuthorityTimeAdvance, LeaseAggregate, LeaseError, LeaseState, LeaseTransitionKind,
    TransitionPlan,
};
use crate::state::QuarantinedState;
use crate::{
    FenceCause, LeasePhase, LeaseTransitionOutcome, ReconcileLease, ReconciliationDisposition,
    RetirementReason,
};
use peritus_policy::AuthorityInstant;
use vstd::prelude::*;

verus! {

impl LeaseAggregate {
    /// Resolves one exactly correlated fenced generation.
    ///
    /// At the non-fencing version boundary, a valid observation retires the aggregate instead of
    /// leaving an available or reconciling state with no representable safe successor.
    ///
    /// # Errors
    ///
    /// Returns a typed phase, correlation, authority-time, version, or state failure.
    pub fn reconcile(
        self,
        command: ReconcileLease,
    ) -> (result: LeaseTransitionOutcome)
        ensures crate::model::concrete::rejections::reconciliation::concrete_reconciliation_decision(
            &self,
            result,
            command,
        ),
    {
        let ghost before = self;
        let LeaseState::Reconciling(reconciling) = self.state else {
            let actual = self.checked_phase();
            let error = LeaseError::IllegalPhase {
                expected: LeasePhase::Reconciling,
                actual,
            };
            assert(crate::model::concrete::rejections::reconciliation::reconciliation_error(
                &self,
                command,
            ) == Some(error));
            return reconciliation_rejection(self, command, error);
        };
        if let Err(error) = validate_correlation(
            reconciling.correlation,
            command.observation.correlation,
        ) {
            assert(crate::model::concrete::rejections::reconciliation::reconciliation_error(
                &self,
                command,
            ) == Some(error));
            return reconciliation_rejection(self, command, error);
        }
        let time_advance = match reconciliation_time_advance(
            &self,
            reconciling.cause,
            command.observed_at(),
        ) {
            Ok(value) => value,
            Err(error) => {
                assert(crate::model::concrete::rejections::reconciliation::reconciliation_error(
                    &self,
                    command,
                ) == Some(error));
                return reconciliation_rejection(self, command, error);
            }
        };
        let version = match next_non_fence_version(self.version) {
            Ok(value) => value,
            Err(error) => {
                assert(crate::model::concrete::rejections::reconciliation::reconciliation_error(
                    &self,
                    command,
                ) == Some(error));
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
        let accepted = match transition(self, TransitionPlan::new(
            command_id,
            version,
            generation,
            time_advance,
            state,
            kind,
            binding,
        )) {
            LeaseTransitionOutcome::Accepted(accepted) => accepted,
            LeaseTransitionOutcome::Rejected(failure) => {
                proof {
                    establish_late_reconciliation_rejection(&before, &failure, command);
                }
                return LeaseTransitionOutcome::Rejected(failure);
            }
        };
        proof {
            establish_reconciliation_acceptance(&before, &accepted, command);
        }
        LeaseTransitionOutcome::Accepted(accepted)
    }
}

proof fn establish_late_reconciliation_rejection(
    before: &LeaseAggregate,
    failure: &crate::LeaseTransitionFailure,
    command: ReconcileLease,
)
    requires
        failure.spec_error() == LeaseError::CorruptState,
        crate::model::concrete::rejections::reconciliation::reconciliation_error(
            before,
            command,
        ) == Some(failure.spec_error()),
        crate::model::concrete_rejection_preserves_input(before, failure),
    ensures crate::model::concrete::rejections::reconciliation::concrete_reconciliation_decision(
        before,
        LeaseTransitionOutcome::Rejected(*failure),
        command,
    ),
{
    crate::model::concrete::rejections::reconciliation::establish_reconciliation_rejection(
        before,
        failure,
        command,
        failure.spec_error(),
    );
}

proof fn establish_reconciliation_acceptance(
    before: &LeaseAggregate,
    accepted: &crate::LeaseTransition,
    command: ReconcileLease,
)
    requires
        crate::model::concrete_record_matches(before, &accepted.next, accepted.record),
        crate::model::concrete_refines_reachability_step(before, &accepted.next),
        crate::model::concrete::fencing::reconciliation_time_matches(
            before,
            &accepted.next,
            command,
        ),
        crate::model::concrete_reconcile_edge(
            before,
            &accepted.next,
            accepted.record,
            command,
        ),
        crate::model::concrete::rejections::reconciliation::reconciliation_error(
            before,
            command,
        ).is_none(),
    ensures crate::model::concrete::rejections::reconciliation::concrete_reconciliation_decision(
        before,
        LeaseTransitionOutcome::Accepted(*accepted),
        command,
    ),
{
    crate::model::concrete::fencing::establish_reconcile_transition(before, accepted, command);
    crate::model::concrete::rejections::reconciliation::establish_reconciliation_acceptance(
        before,
        accepted,
        command,
    );
}

const fn reconciliation_rejection(
    aggregate: LeaseAggregate,
    _command: ReconcileLease,
    error: LeaseError,
) -> (result: LeaseTransitionOutcome)
    requires crate::model::concrete::rejections::reconciliation::reconciliation_error(
        &aggregate,
        _command,
    ) == Some(error),
    ensures crate::model::concrete::rejections::reconciliation::concrete_reconciliation_decision(
        &aggregate,
        result,
        _command,
    ),
{
    let ghost before = aggregate;
    let failure = rejection(aggregate, error);
    proof {
        crate::model::concrete::rejections::reconciliation::establish_reconciliation_rejection(
            &before,
            &failure,
            _command,
            error,
        );
    }
    LeaseTransitionOutcome::Rejected(failure)
}

const fn reconciliation_time_advance(
    aggregate: &LeaseAggregate,
    cause: FenceCause,
    observed_at: AuthorityInstant,
) -> (result: Result<AuthorityTimeAdvance, LeaseError>)
    ensures
        match result {
            Ok(advance) => reconciliation_advance_matches(
                aggregate,
                cause,
                observed_at,
                advance,
            ) && reconciliation_time_error(aggregate, cause, observed_at).is_none(),
            Err(error) => {
                reconciliation_time_error(aggregate, cause, observed_at) == Some(error)
            }
        },
{
    let observed_generation = observed_at.epoch();
    let floor_generation = aggregate.authority_time.epoch();
    let observed_epoch = observed_generation.get();
    let floor_epoch = floor_generation.get();
    assert(observed_epoch == observed_at.spec_epoch());
    assert(floor_epoch == aggregate.authority_time.spec_epoch());
    match cause {
        FenceCause::ClockDiscontinuity if observed_epoch != floor_epoch => {
            Ok(AuthorityTimeAdvance::Reset(observed_at))
        }
        _ => {
            match validate_observation(&aggregate.authority_time, observed_at) {
                Ok(()) => Ok(AuthorityTimeAdvance::Observe(observed_at)),
                Err(error) => Err(error),
            }
        }
    }
}

pub(crate) open spec fn reconciliation_time_error(
    aggregate: &LeaseAggregate,
    cause: FenceCause,
    observed_at: AuthorityInstant,
) -> Option<LeaseError> {
    match cause {
        FenceCause::ClockDiscontinuity
            if observed_at.spec_epoch() != aggregate.authority_time.spec_epoch() => None,
        _ => super::validation::observation_error(&aggregate.authority_time, observed_at),
    }
}

pub(super) open spec fn reconciliation_advance_matches(
    aggregate: &LeaseAggregate,
    cause: FenceCause,
    observed_at: AuthorityInstant,
    advance: AuthorityTimeAdvance,
) -> bool {
    match cause {
        FenceCause::ClockDiscontinuity
            if observed_at.spec_epoch() != aggregate.authority_time.spec_epoch() =>
        {
            advance == AuthorityTimeAdvance::Reset(observed_at)
        }
        _ => {
            advance == AuthorityTimeAdvance::Observe(observed_at)
                && observed_at.spec_epoch() == aggregate.authority_time.spec_epoch()
                && observed_at.spec_tick_millis()
                    >= aggregate.authority_time.spec_greatest_tick_millis()
        }
    }
}

} // verus!
