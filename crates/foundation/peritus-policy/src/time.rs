//! Linear greatest-observed authority-time state.

use crate::{AuthorityInstant, PolicyError};
use peritus_types::Generation;
use vstd::prelude::*;

verus! {
/// Greatest accepted authority-clock observation for one aggregate.
#[derive(Debug, Eq, Hash, PartialEq)]
pub struct AuthorityTimeState {
    epoch: Generation,
    greatest_tick_millis: u64,
}

impl AuthorityTimeState {
    /// Returns the exact active epoch used by specifications.
    pub closed spec fn spec_epoch(&self) -> int { self.epoch.spec_value() }

    /// Returns the exact greatest accepted tick used by specifications.
    pub closed spec fn spec_greatest_tick_millis(&self) -> int {
        self.greatest_tick_millis as int
    }

    /// Initializes monotonic time from the first accepted observation.
    #[must_use]
    pub const fn new(initial: AuthorityInstant) -> (state: Self)
        ensures
            state.spec_epoch() == initial.spec_epoch(),
            state.spec_greatest_tick_millis() == initial.spec_tick_millis(),
    {
        Self { epoch: initial.epoch(), greatest_tick_millis: initial.tick_millis() }
    }

    /// Returns the active epoch.
    #[must_use]
    pub const fn epoch(&self) -> (epoch: Generation)
        ensures epoch.spec_value() == self.spec_epoch(),
    { self.epoch }

    /// Returns the greatest accepted tick.
    #[must_use]
    pub const fn greatest_tick_millis(&self) -> (tick: u64)
        ensures tick as int == self.spec_greatest_tick_millis(),
    { self.greatest_tick_millis }

    /// Returns whether an instant is in this floor's epoch and does not regress its tick.
    pub closed spec fn spec_accepts(&self, candidate: AuthorityInstant) -> bool {
        candidate.spec_epoch() == self.spec_epoch()
            && candidate.spec_tick_millis() >= self.spec_greatest_tick_millis()
    }

    pub(crate) const fn validate_observation(
        &self,
        candidate: AuthorityInstant,
    ) -> (result: Result<(), PolicyError>)
        ensures
            match result {
                Ok(()) => {
                    self.spec_accepts(candidate)
                        && candidate.spec_epoch() == self.spec_epoch()
                        && candidate.spec_tick_millis()
                            >= self.spec_greatest_tick_millis()
                }
                Err(error) => {
                    error.spec_dimension().is_none()
                        && error.spec_collection().is_none()
                        && if candidate.spec_epoch() != self.spec_epoch() {
                        error.spec_kind() == crate::PolicyErrorKind::ClockEpochMismatch
                    } else {
                        candidate.spec_tick_millis() < self.spec_greatest_tick_millis()
                            && error.spec_kind() == crate::PolicyErrorKind::ClockRegression
                    }
                }
            },
    {
        if candidate.epoch().get() != self.epoch.get() {
            Err(PolicyError::clock_epoch_mismatch())
        } else if candidate.tick_millis() < self.greatest_tick_millis {
            Err(PolicyError::clock_regression())
        } else {
            Ok(())
        }
    }

    /// Consumes this linear time floor and validates one nondecreasing observation.
    ///
    /// # Errors
    ///
    /// Returns a typed failure that owns the unchanged time floor on epoch mismatch or regression.
    pub const fn observe(
        self,
        candidate: AuthorityInstant,
    ) -> (result: Result<Self, AuthorityTimeFailure>)
        ensures
            match result {
                Ok(next) => {
                    next.spec_epoch() == self.spec_epoch()
                        && next.spec_epoch() == candidate.spec_epoch()
                        && next.spec_greatest_tick_millis() == candidate.spec_tick_millis()
                        && next.spec_greatest_tick_millis()
                            >= self.spec_greatest_tick_millis()
                }
                Err(failure) => {
                    !self.spec_accepts(candidate)
                        && failure.spec_epoch() == self.spec_epoch()
                        && failure.spec_greatest_tick_millis()
                            == self.spec_greatest_tick_millis()
                        && if candidate.spec_epoch() != self.spec_epoch() {
                            failure.spec_error_kind()
                                == crate::PolicyErrorKind::ClockEpochMismatch
                        } else {
                            candidate.spec_tick_millis()
                                < self.spec_greatest_tick_millis()
                                && failure.spec_error_kind()
                                    == crate::PolicyErrorKind::ClockRegression
                        }
                }
            },
    {
        if candidate.epoch().get() != self.epoch.get() {
            Err(AuthorityTimeFailure::new(
                PolicyError::clock_epoch_mismatch(),
                self,
            ))
        } else if candidate.tick_millis() < self.greatest_tick_millis {
            Err(AuthorityTimeFailure::new(
                PolicyError::clock_regression(),
                self,
            ))
        } else {
            assert(candidate.spec_tick_millis() >= self.greatest_tick_millis as int);
            Ok(Self {
                epoch: self.epoch,
                greatest_tick_millis: candidate.tick_millis(),
            })
        }
    }
}

/// Failed authority-time transition that owns the unchanged move-only floor.
#[derive(Debug, Eq, PartialEq)]
pub struct AuthorityTimeFailure {
    error: PolicyError,
    state: AuthorityTimeState,
}

impl AuthorityTimeFailure {
    /// Returns the exact failure category used by specifications.
    pub closed spec fn spec_error_kind(&self) -> crate::PolicyErrorKind {
        self.error.spec_kind()
    }

    /// Returns the unchanged authority-clock epoch used by specifications.
    pub closed spec fn spec_epoch(&self) -> int { self.state.spec_epoch() }

    /// Returns the unchanged greatest authority tick used by specifications.
    pub closed spec fn spec_greatest_tick_millis(&self) -> int {
        self.state.spec_greatest_tick_millis()
    }

    pub(crate) const fn new(error: PolicyError, state: AuthorityTimeState) -> (failure: Self)
        ensures
            failure.spec_error_kind() == error.spec_kind(),
            failure.spec_epoch() == state.spec_epoch(),
            failure.spec_greatest_tick_millis() == state.spec_greatest_tick_millis(),
    {
        Self { error, state }
    }

    /// Returns the exact typed time or evaluation failure.
    #[must_use]
    pub const fn error(&self) -> (error: PolicyError)
        ensures error.spec_kind() == self.spec_error_kind(),
    { self.error }

    /// Borrows the unchanged move-only authority-time floor.
    #[must_use]
    pub const fn state(&self) -> (state: &AuthorityTimeState)
        ensures
            state.spec_epoch() == self.spec_epoch(),
            state.spec_greatest_tick_millis() == self.spec_greatest_tick_millis(),
    { &self.state }

    /// Consumes the failure and returns its unchanged authority-time floor.
    #[must_use]
    pub const fn into_state(self) -> (state: AuthorityTimeState)
        ensures
            state.spec_epoch() == self.spec_epoch(),
            state.spec_greatest_tick_millis() == self.spec_greatest_tick_millis(),
    { self.state }
}

} // verus!
