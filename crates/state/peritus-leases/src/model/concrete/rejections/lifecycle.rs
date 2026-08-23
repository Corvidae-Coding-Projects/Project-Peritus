//! Ordered first-error and total-decision models for mint, acquire, and renew.

#[cfg(verus_only)]
use crate::state::LeaseState;
#[cfg(verus_only)]
use crate::{LeaseAggregate, LeaseError, LeasePhase};
use vstd::prelude::*;

verus! {

pub(crate) open spec fn deadline_overflows(
    observed_at: peritus_policy::AuthorityInstant,
    duration: crate::LeaseDuration,
) -> bool {
    observed_at.spec_tick_millis() + duration.spec_millis() > u64::MAX as int
}

pub(crate) open spec fn active_version_error(
    aggregate: &LeaseAggregate,
) -> Option<LeaseError> {
    if aggregate.version.spec_value() >= (u64::MAX - 1) as int {
        Some(LeaseError::VersionExhausted)
    } else {
        None
    }
}

/// Exact ordered first rejection selected by acquisition.
pub(crate) open spec fn acquire_error(
    aggregate: &LeaseAggregate,
    command: crate::AcquireLease,
) -> Option<LeaseError> {
    let phase = crate::transition::phase_error(aggregate, LeasePhase::Available);
    if phase.is_some() {
        phase
    } else {
        let observation = crate::transition::observation_error(
            &aggregate.authority_time,
            command.observed_at,
        );
        if observation.is_some() {
            observation
        } else if deadline_overflows(command.observed_at, command.duration) {
            Some(LeaseError::TimeOverflow)
        } else {
            active_version_error(aggregate)
        }
    }
}

/// Exact total acquisition decision, including the complete rejected snapshot.
pub closed spec fn concrete_acquire_decision(
    aggregate: &LeaseAggregate,
    result: crate::LeaseTransitionOutcome,
    command: crate::AcquireLease,
) -> bool {
    super::exact_transition_decision(
        aggregate,
        result,
        acquire_error(aggregate, command),
        |accepted: &crate::LeaseTransition| {
            super::super::concrete_acquire_transition(aggregate, accepted, command)
        },
    )
}

pub(crate) proof fn establish_acquire_rejection(
    aggregate: &LeaseAggregate,
    failure: &crate::LeaseTransitionFailure,
    command: crate::AcquireLease,
    error: LeaseError,
)
    requires
        acquire_error(aggregate, command) == Some(error),
        failure.spec_error() == error,
        super::super::preservation::concrete_rejection_preserves_input(aggregate, failure),
    ensures concrete_acquire_decision(
        aggregate,
        crate::LeaseTransitionOutcome::Rejected(*failure),
        command,
    ),
{
}

pub(crate) proof fn establish_acquire_acceptance(
    aggregate: &LeaseAggregate,
    accepted: &crate::LeaseTransition,
    command: crate::AcquireLease,
)
    requires
        acquire_error(aggregate, command).is_none(),
        super::super::concrete_acquire_transition(aggregate, accepted, command),
    ensures concrete_acquire_decision(
        aggregate,
        crate::LeaseTransitionOutcome::Accepted(*accepted),
        command,
    ),
{
}

pub(crate) open spec fn renew_after_claim_error(
    aggregate: &LeaseAggregate,
    active: crate::state::ActiveLease,
    command: crate::RenewLease,
) -> Option<LeaseError> {
    let observation = crate::transition::observation_error(
        &aggregate.authority_time,
        command.observed_at,
    );
    if observation.is_some() {
        observation
    } else {
        let expiry = crate::transition::before_expiry_error(
            active.expires_at,
            command.observed_at,
        );
        if expiry.is_some() {
            expiry
        } else if deadline_overflows(command.observed_at, command.duration) {
            Some(LeaseError::TimeOverflow)
        } else if command.observed_at.spec_tick_millis() + command.duration.spec_millis()
            <= active.expires_at.spec_tick_millis()
        {
            Some(LeaseError::DeadlineNotExtended)
        } else if active.claim_version.spec_value() == u64::MAX as int {
            Some(LeaseError::ClaimVersionExhausted)
        } else {
            let version = active_version_error(aggregate);
            if version.is_some() {
                version
            } else if !aggregate.internal_is_valid() {
                Some(LeaseError::CorruptState)
            } else {
                None
            }
        }
    }
}

/// Exact ordered first rejection selected by renewal.
pub(crate) open spec fn renew_error(
    aggregate: &LeaseAggregate,
    command: crate::RenewLease,
) -> Option<LeaseError> {
    let claim = crate::transition::active_claim_error(aggregate, command.claim);
    if claim.is_some() {
        claim
    } else {
        match aggregate.state {
            LeaseState::Active(active) => renew_after_claim_error(aggregate, active, command),
            _ => Some(LeaseError::IllegalPhase {
                expected: LeasePhase::Active,
                actual: aggregate.internal_phase(),
            }),
        }
    }
}

/// Exact total renewal decision, including the complete rejected snapshot.
pub closed spec fn concrete_renew_decision(
    aggregate: &LeaseAggregate,
    result: crate::LeaseTransitionOutcome,
    command: crate::RenewLease,
) -> bool {
    super::exact_transition_decision(
        aggregate,
        result,
        renew_error(aggregate, command),
        |accepted: &crate::LeaseTransition| {
            super::super::concrete_renew_transition(aggregate, accepted, command)
        },
    )
}

pub(crate) proof fn establish_renew_rejection(
    aggregate: &LeaseAggregate,
    failure: &crate::LeaseTransitionFailure,
    command: crate::RenewLease,
    error: LeaseError,
)
    requires
        renew_error(aggregate, command) == Some(error),
        failure.spec_error() == error,
        super::super::preservation::concrete_rejection_preserves_input(aggregate, failure),
    ensures concrete_renew_decision(
        aggregate,
        crate::LeaseTransitionOutcome::Rejected(*failure),
        command,
    ),
{
}

pub(crate) proof fn establish_renew_acceptance(
    aggregate: &LeaseAggregate,
    accepted: &crate::LeaseTransition,
    command: crate::RenewLease,
)
    requires
        renew_error(aggregate, command).is_none(),
        super::super::concrete_renew_transition(aggregate, accepted, command),
    ensures concrete_renew_decision(
        aggregate,
        crate::LeaseTransitionOutcome::Accepted(*accepted),
        command,
    ),
{
}

} // verus!
