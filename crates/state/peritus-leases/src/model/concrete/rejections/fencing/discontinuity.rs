//! Ordered first-error and total-decision model for authority-clock discontinuity fencing.

#[cfg(verus_only)]
use crate::{LeaseAggregate, LeaseError};
use vstd::prelude::*;

verus! {

pub(crate) open spec fn discontinuity_error(
    aggregate: &LeaseAggregate,
    command: crate::FenceClockDiscontinuity,
) -> Option<LeaseError> {
    let active = crate::transition::active_error(aggregate);
    if active.is_some() {
        active
    } else if command.observed_at.spec_epoch() == aggregate.authority_time.spec_epoch()
        && command.observed_at.spec_tick_millis()
            >= aggregate.authority_time.spec_greatest_tick_millis()
    {
        Some(LeaseError::NoClockDiscontinuity)
    } else {
        super::final_fence_error(aggregate)
    }
}

pub closed spec fn concrete_discontinuity_decision(
    aggregate: &LeaseAggregate,
    result: crate::LeaseTransitionOutcome,
    command: crate::FenceClockDiscontinuity,
) -> bool {
    super::super::exact_transition_decision(
        aggregate,
        result,
        discontinuity_error(aggregate, command),
        |accepted: &crate::LeaseTransition| {
            super::super::super::fence_commands::concrete_discontinuity_transition(
                aggregate,
                accepted,
                command,
            )
        },
    )
}

pub(crate) proof fn establish_discontinuity_rejection(
    aggregate: &LeaseAggregate,
    failure: &crate::LeaseTransitionFailure,
    command: crate::FenceClockDiscontinuity,
    error: LeaseError,
)
    requires
        discontinuity_error(aggregate, command) == Some(error),
        failure.spec_error() == error,
        super::super::super::preservation::concrete_rejection_preserves_input(
            aggregate,
            failure,
        ),
    ensures concrete_discontinuity_decision(
        aggregate,
        crate::LeaseTransitionOutcome::Rejected(*failure),
        command,
    ),
{
}

pub(crate) proof fn establish_discontinuity_acceptance(
    aggregate: &LeaseAggregate,
    accepted: &crate::LeaseTransition,
    command: crate::FenceClockDiscontinuity,
)
    requires
        discontinuity_error(aggregate, command).is_none(),
        super::super::super::fence_commands::concrete_discontinuity_transition(
            aggregate,
            accepted,
            command,
        ),
    ensures concrete_discontinuity_decision(
        aggregate,
        crate::LeaseTransitionOutcome::Accepted(*accepted),
        command,
    ),
{
}

} // verus!
