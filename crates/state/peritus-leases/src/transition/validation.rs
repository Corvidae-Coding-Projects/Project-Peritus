//! Shared validation and checked-successor helpers for pure reducers.

mod claim;

#[cfg(verus_only)]
pub(crate) use self::claim::active_claim_error;
pub(super) use self::claim::{require_active_claim, validate_claim_identity};

use crate::state::{ActiveLease, LeaseState};
use crate::{LeaseAggregate, LeaseError, LeasePhase};
use peritus_policy::{
    AuthorityInstant, AuthorityTimeState, PolicyError, PolicyErrorKind,
};
use peritus_types::RevisionNumber;
use vstd::prelude::*;

verus! {

pub(super) open spec fn aggregate_has_phase(
    aggregate: &LeaseAggregate,
    expected: LeasePhase,
) -> bool {
    match (aggregate.state, expected) {
        (LeaseState::Available, LeasePhase::Available)
        | (LeaseState::Active(_), LeasePhase::Active)
        | (LeaseState::Reconciling(_), LeasePhase::Reconciling)
        | (LeaseState::Quarantined(_), LeasePhase::Quarantined)
        | (LeaseState::Retired(_), LeasePhase::Retired) => true,
        _ => false,
    }
}

pub(crate) open spec fn phase_error(
    aggregate: &LeaseAggregate,
    expected: LeasePhase,
) -> Option<LeaseError> {
    match (aggregate.state, expected) {
        (LeaseState::Available, LeasePhase::Available)
        | (LeaseState::Active(_), LeasePhase::Active)
        | (LeaseState::Reconciling(_), LeasePhase::Reconciling)
        | (LeaseState::Quarantined(_), LeasePhase::Quarantined)
        | (LeaseState::Retired(_), LeasePhase::Retired) => None,
        _ => Some(LeaseError::IllegalPhase {
            expected,
            actual: aggregate.internal_phase(),
        }),
    }
}

pub(super) const fn require_phase(
    aggregate: &LeaseAggregate,
    expected: LeasePhase,
) -> (result: Result<(), LeaseError>)
    ensures
        match result {
            Ok(()) => phase_error(aggregate, expected).is_none(),
            Err(error) => phase_error(aggregate, expected) == Some(error),
        },
{
    let actual = aggregate.checked_phase();
    match (aggregate.state, expected) {
        (LeaseState::Available, LeasePhase::Available)
        | (LeaseState::Active(_), LeasePhase::Active)
        | (LeaseState::Reconciling(_), LeasePhase::Reconciling)
        | (LeaseState::Quarantined(_), LeasePhase::Quarantined)
        | (LeaseState::Retired(_), LeasePhase::Retired) => {
            assert(phase_error(aggregate, expected).is_none());
            Ok(())
        }
        _ => {
            assert(actual == aggregate.internal_phase());
            assert(phase_error(aggregate, expected)
                == Some(LeaseError::IllegalPhase { expected, actual }));
            Err(LeaseError::IllegalPhase { expected, actual })
        }
    }
}

pub(crate) open spec fn active_error(aggregate: &LeaseAggregate) -> Option<LeaseError> {
    match aggregate.state {
        LeaseState::Active(_) => None,
        _ => Some(LeaseError::IllegalPhase {
            expected: LeasePhase::Active,
            actual: aggregate.internal_phase(),
        }),
    }
}

pub(super) const fn require_active(
    aggregate: &LeaseAggregate,
) -> (result: Result<ActiveLease, LeaseError>)
    ensures
        match result {
            Ok(active) => {
                aggregate.state == LeaseState::Active(active)
                    && active_error(aggregate).is_none()
            }
            Err(error) => active_error(aggregate) == Some(error),
        },
{
    if let LeaseState::Active(active) = aggregate.state {
        assert(active_error(aggregate).is_none());
        Ok(active)
    } else {
        let actual = aggregate.checked_phase();
        assert(actual == aggregate.internal_phase());
        assert(active_error(aggregate) == Some(LeaseError::IllegalPhase {
            expected: LeasePhase::Active,
            actual,
        }));
        Err(LeaseError::IllegalPhase {
            expected: LeasePhase::Active,
            actual,
        })
    }
}

pub(crate) open spec fn observation_error(
    floor: &AuthorityTimeState,
    observed_at: AuthorityInstant,
) -> Option<LeaseError> {
    if observed_at.spec_epoch() != floor.spec_epoch() {
        Some(LeaseError::ClockEpochMismatch)
    } else if observed_at.spec_tick_millis() < floor.spec_greatest_tick_millis() {
        Some(LeaseError::ClockRegression)
    } else {
        None
    }
}

pub(super) const fn validate_observation(
    floor: &AuthorityTimeState,
    observed_at: AuthorityInstant,
) -> (result: Result<(), LeaseError>)
    ensures
        match result {
            Ok(()) => {
                observation_error(floor, observed_at).is_none()
                    &&
                floor.spec_epoch() == observed_at.spec_epoch()
                    && observed_at.spec_tick_millis()
                        >= floor.spec_greatest_tick_millis()
            }
            Err(error) => observation_error(floor, observed_at) == Some(error),
        },
{
    if observed_at.epoch().get() != floor.epoch().get() {
        Err(LeaseError::ClockEpochMismatch)
    } else if observed_at.tick_millis() < floor.greatest_tick_millis() {
        Err(LeaseError::ClockRegression)
    } else {
        Ok(())
    }
}

pub(crate) open spec fn before_expiry_error(
    expires_at: AuthorityInstant,
    observed_at: AuthorityInstant,
) -> Option<LeaseError> {
    if observed_at.spec_epoch() != expires_at.spec_epoch() {
        Some(LeaseError::ClockEpochMismatch)
    } else if observed_at.spec_tick_millis() >= expires_at.spec_tick_millis() {
        Some(LeaseError::ClaimExpired)
    } else {
        None
    }
}

pub(super) open spec fn mapped_policy_error(error: PolicyError) -> LeaseError {
    match error.spec_kind() {
        PolicyErrorKind::ClockEpochMismatch => LeaseError::ClockEpochMismatch,
        PolicyErrorKind::ClockRegression => LeaseError::ClockRegression,
        PolicyErrorKind::TimeOverflow => LeaseError::TimeOverflow,
        _ => LeaseError::PolicyUseInvalid,
    }
}

pub(super) const fn map_policy_time(error: PolicyError) -> (mapped: LeaseError)
    ensures mapped == mapped_policy_error(error),
{
    match error.kind() {
        PolicyErrorKind::ClockEpochMismatch => LeaseError::ClockEpochMismatch,
        PolicyErrorKind::ClockRegression => LeaseError::ClockRegression,
        PolicyErrorKind::TimeOverflow => LeaseError::TimeOverflow,
        _ => LeaseError::PolicyUseInvalid,
    }
}

pub(super) const fn ensure_before_expiry(
    expires_at: AuthorityInstant,
    observed_at: AuthorityInstant,
) -> (result: Result<(), LeaseError>)
    ensures
        match result {
            Ok(()) => {
                before_expiry_error(expires_at, observed_at).is_none()
                    && observed_at.spec_epoch() == expires_at.spec_epoch()
                    && observed_at.spec_tick_millis() < expires_at.spec_tick_millis()
            }
            Err(error) => before_expiry_error(expires_at, observed_at) == Some(error),
        },
{
    let observed_epoch = observed_at.epoch().get();
    let expires_epoch = expires_at.epoch().get();
    let observed_tick = observed_at.tick_millis();
    let expires_tick = expires_at.tick_millis();
    if observed_epoch != expires_epoch {
        Err(LeaseError::ClockEpochMismatch)
    } else if observed_tick >= expires_tick {
        Err(LeaseError::ClaimExpired)
    } else {
        assert(observed_at.spec_epoch() == expires_at.spec_epoch());
        assert(observed_at.spec_tick_millis() < expires_at.spec_tick_millis());
        Ok(())
    }
}

pub(super) fn next_active_version(
    version: RevisionNumber,
) -> (result: Result<RevisionNumber, LeaseError>)
    ensures
        match result {
            Ok(next) => {
                next.spec_value() == version.spec_value() + 1
                    && version.spec_value() < (u64::MAX - 1) as int
            }
            Err(error) => {
                error == LeaseError::VersionExhausted
                    && version.spec_value() >= (u64::MAX - 1) as int
            }
        },
{
    next_non_fence_version(version)
}

pub(super) fn next_non_fence_version(
    version: RevisionNumber,
) -> (result: Result<RevisionNumber, LeaseError>)
    ensures
        match result {
            Ok(next) => {
                next.spec_value() == version.spec_value() + 1
                    && version.spec_value() < (u64::MAX - 1) as int
            }
            Err(error) => {
                error == LeaseError::VersionExhausted
                    && version.spec_value() >= (u64::MAX - 1) as int
            }
        },
{
    if version.get() >= u64::MAX - 1 {
        Err(LeaseError::VersionExhausted)
    } else {
        version.checked_next().map_err(|_error| LeaseError::VersionExhausted)
    }
}

pub(super) const fn earlier(
    left: AuthorityInstant,
    right: AuthorityInstant,
) -> (result: AuthorityInstant)
    ensures
        left.spec_tick_millis() <= right.spec_tick_millis() ==> result == left,
        left.spec_tick_millis() > right.spec_tick_millis() ==> result == right,
{
    if left.tick_millis() <= right.tick_millis() { left } else { right }
}

} // verus!
