//! Ordered first-error and total-decision model for authorized revocation fencing.

#[cfg(verus_only)]
use crate::{LeaseAggregate, LeaseError};
use vstd::prelude::*;

verus! {

pub(crate) open spec fn revoke_error(
    aggregate: &LeaseAggregate,
    command: crate::RevokeLease,
) -> Option<LeaseError> {
    let claim = crate::transition::active_claim_error(aggregate, command.claim);
    if claim.is_some() {
        claim
    } else {
        let observation = crate::transition::observation_error(
            &aggregate.authority_time,
            command.observed_at,
        );
        if observation.is_some() { observation } else { super::final_fence_error(aggregate) }
    }
}

pub closed spec fn concrete_revoke_decision(
    aggregate: &LeaseAggregate,
    result: crate::LeaseTransitionOutcome,
    command: crate::RevokeLease,
) -> bool {
    super::super::exact_transition_decision(
        aggregate,
        result,
        revoke_error(aggregate, command),
        |accepted: &crate::LeaseTransition| {
            super::super::super::fence_commands::concrete_revoke_transition(
                aggregate,
                accepted,
                command,
            )
        },
    )
}

pub(crate) proof fn establish_revoke_rejection(
    aggregate: &LeaseAggregate,
    failure: &crate::LeaseTransitionFailure,
    command: crate::RevokeLease,
    error: LeaseError,
)
    requires
        revoke_error(aggregate, command) == Some(error),
        failure.spec_error() == error,
        super::super::super::preservation::concrete_rejection_preserves_input(
            aggregate,
            failure,
        ),
    ensures concrete_revoke_decision(
        aggregate,
        crate::LeaseTransitionOutcome::Rejected(*failure),
        command,
    ),
{
}

pub(crate) proof fn establish_revoke_acceptance(
    aggregate: &LeaseAggregate,
    accepted: &crate::LeaseTransition,
    command: crate::RevokeLease,
)
    requires
        revoke_error(aggregate, command).is_none(),
        super::super::super::fence_commands::concrete_revoke_transition(
            aggregate,
            accepted,
            command,
        ),
    ensures concrete_revoke_decision(
        aggregate,
        crate::LeaseTransitionOutcome::Accepted(*accepted),
        command,
    ),
{
}

} // verus!
