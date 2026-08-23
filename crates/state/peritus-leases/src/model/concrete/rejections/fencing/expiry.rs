//! Ordered first-error and total-decision model for deadline expiry fencing.

#[cfg(verus_only)]
use crate::state::LeaseState;
#[cfg(verus_only)]
use crate::{LeaseAggregate, LeaseError};
use vstd::prelude::*;

verus! {

pub(crate) open spec fn expiry_error(
    aggregate: &LeaseAggregate,
    command: crate::ExpireLease,
) -> Option<LeaseError> {
    let active_error = crate::transition::active_error(aggregate);
    if active_error.is_some() {
        active_error
    } else {
        let observation = crate::transition::observation_error(
            &aggregate.authority_time,
            command.observed_at,
        );
        if observation.is_some() {
            observation
        } else {
            match aggregate.state {
                LeaseState::Active(active) => {
                    if command.observed_at.spec_epoch() != active.expires_at.spec_epoch() {
                        Some(LeaseError::CorruptState)
                    } else if command.observed_at.spec_tick_millis()
                        < active.expires_at.spec_tick_millis()
                    {
                        Some(LeaseError::LeaseNotExpired)
                    } else {
                        super::final_fence_error(aggregate)
                    }
                }
                _ => active_error,
            }
        }
    }
}

pub closed spec fn concrete_expiry_decision(
    aggregate: &LeaseAggregate,
    result: crate::LeaseTransitionOutcome,
    command: crate::ExpireLease,
) -> bool {
    super::super::exact_transition_decision(
        aggregate,
        result,
        expiry_error(aggregate, command),
        |accepted: &crate::LeaseTransition| {
            super::super::super::fence_commands::concrete_expire_transition(
                aggregate,
                accepted,
                command,
            )
        },
    )
}

pub(crate) proof fn establish_expiry_rejection(
    aggregate: &LeaseAggregate,
    failure: &crate::LeaseTransitionFailure,
    command: crate::ExpireLease,
    error: LeaseError,
)
    requires
        expiry_error(aggregate, command) == Some(error),
        failure.spec_error() == error,
        super::super::super::preservation::concrete_rejection_preserves_input(
            aggregate,
            failure,
        ),
    ensures concrete_expiry_decision(
        aggregate,
        crate::LeaseTransitionOutcome::Rejected(*failure),
        command,
    ),
{
}

pub(crate) proof fn establish_expiry_acceptance(
    aggregate: &LeaseAggregate,
    accepted: &crate::LeaseTransition,
    command: crate::ExpireLease,
)
    requires
        expiry_error(aggregate, command).is_none(),
        super::super::super::fence_commands::concrete_expire_transition(
            aggregate,
            accepted,
            command,
        ),
    ensures concrete_expiry_decision(
        aggregate,
        crate::LeaseTransitionOutcome::Accepted(*accepted),
        command,
    ),
{
}

} // verus!
