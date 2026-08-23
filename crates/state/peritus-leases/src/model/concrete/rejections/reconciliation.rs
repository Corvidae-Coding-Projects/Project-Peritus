//! Ordered first-error and total-decision model for fenced-generation reconciliation.

#[cfg(verus_only)]
use crate::state::LeaseState;
#[cfg(verus_only)]
use crate::{LeaseAggregate, LeaseError, LeasePhase};
use vstd::prelude::*;

verus! {

pub(crate) open spec fn reconciliation_error(
    aggregate: &LeaseAggregate,
    command: crate::ReconcileLease,
) -> Option<LeaseError> {
    match aggregate.state {
        LeaseState::Reconciling(reconciling) => {
            let correlation = crate::transition::correlation_error(
                reconciling.correlation,
                command.observation.correlation,
            );
            if correlation.is_some() {
                correlation
            } else {
                let time = crate::transition::reconciliation_time_error(
                    aggregate,
                    reconciling.cause,
                    command.observed_at,
                );
                if time.is_some() {
                    time
                } else if aggregate.version.spec_value() >= (u64::MAX - 1) as int {
                    Some(LeaseError::VersionExhausted)
                } else if !aggregate.internal_is_valid() {
                    Some(LeaseError::CorruptState)
                } else {
                    None
                }
            }
        }
        _ => Some(LeaseError::IllegalPhase {
            expected: LeasePhase::Reconciling,
            actual: aggregate.internal_phase(),
        }),
    }
}

pub closed spec fn concrete_reconciliation_decision(
    aggregate: &LeaseAggregate,
    result: crate::LeaseTransitionOutcome,
    command: crate::ReconcileLease,
) -> bool {
    super::exact_transition_decision(
        aggregate,
        result,
        reconciliation_error(aggregate, command),
        |accepted: &crate::LeaseTransition| {
            super::super::fencing::concrete_reconcile_transition(
                aggregate,
                accepted,
                command,
            )
        },
    )
}

pub(crate) proof fn establish_reconciliation_rejection(
    aggregate: &LeaseAggregate,
    failure: &crate::LeaseTransitionFailure,
    command: crate::ReconcileLease,
    error: LeaseError,
)
    requires
        reconciliation_error(aggregate, command) == Some(error),
        failure.spec_error() == error,
        super::super::preservation::concrete_rejection_preserves_input(aggregate, failure),
    ensures concrete_reconciliation_decision(
        aggregate,
        crate::LeaseTransitionOutcome::Rejected(*failure),
        command,
    ),
{
}

pub(crate) proof fn establish_reconciliation_acceptance(
    aggregate: &LeaseAggregate,
    accepted: &crate::LeaseTransition,
    command: crate::ReconcileLease,
)
    requires
        reconciliation_error(aggregate, command).is_none(),
        super::super::fencing::concrete_reconcile_transition(aggregate, accepted, command),
    ensures concrete_reconciliation_decision(
        aggregate,
        crate::LeaseTransitionOutcome::Accepted(*accepted),
        command,
    ),
{
}

} // verus!
