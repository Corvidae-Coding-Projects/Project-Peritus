//! Ordered first-error and total-decision model for holder-loss fencing.

#[cfg(verus_only)]
use crate::{LeaseAggregate, LeaseError};
use vstd::prelude::*;

verus! {

pub(crate) open spec fn holder_loss_error(
    aggregate: &LeaseAggregate,
    command: crate::FenceHolderLoss,
) -> Option<LeaseError> {
    let active = crate::transition::active_error(aggregate);
    if active.is_some() {
        active
    } else if crate::transition::active_claim_error(
        aggregate,
        command.evidence.spec_claim(),
    ).is_some() {
        Some(LeaseError::HolderLossMismatch)
    } else {
        let observation = crate::transition::observation_error(
            &aggregate.authority_time,
            command.observed_at,
        );
        if observation.is_some() { observation } else { super::final_fence_error(aggregate) }
    }
}

pub closed spec fn concrete_holder_loss_decision(
    aggregate: &LeaseAggregate,
    result: crate::LeaseTransitionOutcome,
    command: crate::FenceHolderLoss,
) -> bool {
    super::super::exact_transition_decision(
        aggregate,
        result,
        holder_loss_error(aggregate, command),
        |accepted: &crate::LeaseTransition| {
            super::super::super::fence_commands::concrete_holder_loss_transition(
                aggregate,
                accepted,
                command,
            )
        },
    )
}

pub(crate) proof fn establish_holder_loss_rejection(
    aggregate: &LeaseAggregate,
    failure: &crate::LeaseTransitionFailure,
    command: crate::FenceHolderLoss,
    error: LeaseError,
)
    requires
        holder_loss_error(aggregate, command) == Some(error),
        failure.spec_error() == error,
        super::super::super::preservation::concrete_rejection_preserves_input(
            aggregate,
            failure,
        ),
    ensures concrete_holder_loss_decision(
        aggregate,
        crate::LeaseTransitionOutcome::Rejected(*failure),
        command,
    ),
{
}

pub(crate) proof fn establish_holder_loss_acceptance(
    aggregate: &LeaseAggregate,
    accepted: &crate::LeaseTransition,
    command: crate::FenceHolderLoss,
)
    requires
        holder_loss_error(aggregate, command).is_none(),
        super::super::super::fence_commands::concrete_holder_loss_transition(
            aggregate,
            accepted,
            command,
        ),
    ensures concrete_holder_loss_decision(
        aggregate,
        crate::LeaseTransitionOutcome::Accepted(*accepted),
        command,
    ),
{
}

} // verus!
