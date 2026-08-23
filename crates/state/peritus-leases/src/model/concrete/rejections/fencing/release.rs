//! Ordered first-error and total-decision model for voluntary release.

#[cfg(verus_only)]
use crate::{LeaseAggregate, LeaseError};
use vstd::prelude::*;

verus! {

pub(crate) open spec fn quiescence_error(
    command: crate::ReleaseLease,
) -> Option<LeaseError> {
    match command.spec_quiescence() {
        Some(evidence) if !super::super::super::identity::concrete_claim_matches(
            evidence.spec_claim(),
            command.claim,
        ) => Some(LeaseError::HolderQuiescenceMismatch),
        _ => None,
    }
}

pub(crate) open spec fn release_error(
    aggregate: &LeaseAggregate,
    command: crate::ReleaseLease,
) -> Option<LeaseError> {
    let claim = crate::transition::active_claim_error(aggregate, command.claim);
    if claim.is_some() {
        claim
    } else {
        let observation = crate::transition::observation_error(
            &aggregate.authority_time,
            command.observed_at,
        );
        if observation.is_some() {
            observation
        } else {
            let evidence = quiescence_error(command);
            if evidence.is_some() { evidence } else { super::final_fence_error(aggregate) }
        }
    }
}

pub closed spec fn concrete_release_decision(
    aggregate: &LeaseAggregate,
    result: crate::LeaseTransitionOutcome,
    command: crate::ReleaseLease,
) -> bool {
    super::super::exact_transition_decision(
        aggregate,
        result,
        release_error(aggregate, command),
        |accepted: &crate::LeaseTransition| {
            super::super::super::fence_commands::concrete_release_transition(
                aggregate,
                accepted,
                command,
            )
        },
    )
}

pub(crate) proof fn establish_release_rejection(
    aggregate: &LeaseAggregate,
    failure: &crate::LeaseTransitionFailure,
    command: crate::ReleaseLease,
    error: LeaseError,
)
    requires
        release_error(aggregate, command) == Some(error),
        failure.spec_error() == error,
        super::super::super::preservation::concrete_rejection_preserves_input(
            aggregate,
            failure,
        ),
    ensures concrete_release_decision(
        aggregate,
        crate::LeaseTransitionOutcome::Rejected(*failure),
        command,
    ),
{
}

pub(crate) proof fn establish_release_acceptance(
    aggregate: &LeaseAggregate,
    accepted: &crate::LeaseTransition,
    command: crate::ReleaseLease,
)
    requires
        release_error(aggregate, command).is_none(),
        super::super::super::fence_commands::concrete_release_transition(
            aggregate,
            accepted,
            command,
        ),
    ensures concrete_release_decision(
        aggregate,
        crate::LeaseTransitionOutcome::Accepted(*accepted),
        command,
    ),
{
}

} // verus!
