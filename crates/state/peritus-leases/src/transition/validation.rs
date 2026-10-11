//! Shared validation and checked-successor helpers for pure reducers.

mod claim;

pub(super) use self::claim::{require_active_claim, validate_claim_identity};

use crate::state::{ActiveLease, LeaseState};
use crate::{LeaseAggregate, LeaseError, LeasePhase};
use peritus_policy::{AuthorityInstant, AuthorityTimeState, PolicyError, PolicyErrorKind};
use peritus_types::RevisionNumber;

pub(super) const fn require_phase(
    aggregate: &LeaseAggregate,
    expected: LeasePhase,
) -> Result<(), LeaseError> {
    let actual = aggregate.checked_phase();
    match (aggregate.state, expected) {
        (LeaseState::Available, LeasePhase::Available)
        | (LeaseState::Active(_), LeasePhase::Active)
        | (LeaseState::Reconciling(_), LeasePhase::Reconciling)
        | (LeaseState::Quarantined(_), LeasePhase::Quarantined)
        | (LeaseState::Retired(_), LeasePhase::Retired) => Ok(()),
        _ => Err(LeaseError::IllegalPhase { expected, actual }),
    }
}
pub(super) const fn require_active(aggregate: &LeaseAggregate) -> Result<ActiveLease, LeaseError> {
    if let LeaseState::Active(active) = aggregate.state {
        Ok(active)
    } else {
        let actual = aggregate.checked_phase();
        Err(LeaseError::IllegalPhase { expected: LeasePhase::Active, actual })
    }
}
pub(super) const fn validate_observation(
    floor: &AuthorityTimeState,
    observed_at: AuthorityInstant,
) -> Result<(), LeaseError> {
    if observed_at.epoch().get() != floor.epoch().get() {
        Err(LeaseError::ClockEpochMismatch)
    } else if observed_at.tick_millis() < floor.greatest_tick_millis() {
        Err(LeaseError::ClockRegression)
    } else {
        Ok(())
    }
}
pub(super) const fn map_policy_time(error: PolicyError) -> LeaseError {
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
) -> Result<(), LeaseError> {
    let observed_epoch = observed_at.epoch().get();
    let expires_epoch = expires_at.epoch().get();
    let observed_tick = observed_at.tick_millis();
    let expires_tick = expires_at.tick_millis();
    if observed_epoch != expires_epoch {
        Err(LeaseError::ClockEpochMismatch)
    } else if observed_tick >= expires_tick {
        Err(LeaseError::ClaimExpired)
    } else {
        Ok(())
    }
}
pub(super) fn next_active_version(version: RevisionNumber) -> Result<RevisionNumber, LeaseError> {
    next_non_fence_version(version)
}
pub(super) fn next_non_fence_version(
    version: RevisionNumber,
) -> Result<RevisionNumber, LeaseError> {
    if version.get() >= u64::MAX - 1 {
        Err(LeaseError::VersionExhausted)
    } else {
        version.checked_next().map_err(|_error| LeaseError::VersionExhausted)
    }
}
pub(super) const fn earlier(left: AuthorityInstant, right: AuthorityInstant) -> AuthorityInstant {
    if left.tick_millis() <= right.tick_millis() { left } else { right }
}
