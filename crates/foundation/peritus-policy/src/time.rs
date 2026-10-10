//! Linear greatest-observed authority-time state.

use crate::{AuthorityInstant, PolicyError};
use peritus_types::Generation;

#[doc = " Greatest accepted authority-clock observation for one aggregate."]
#[derive(Debug, Eq, Hash, PartialEq)]
pub struct AuthorityTimeState {
    epoch: Generation,
    greatest_tick_millis: u64,
}
impl AuthorityTimeState {
    #[doc = " Initializes monotonic time from the first accepted observation."]
    #[must_use]
    pub const fn new(initial: AuthorityInstant) -> Self {
        Self { epoch: initial.epoch(), greatest_tick_millis: initial.tick_millis() }
    }
    #[doc = " Returns the active epoch."]
    #[must_use]
    pub const fn epoch(&self) -> Generation {
        self.epoch
    }
    #[doc = " Returns the greatest accepted tick."]
    #[must_use]
    pub const fn greatest_tick_millis(&self) -> u64 {
        self.greatest_tick_millis
    }
    pub(crate) const fn validate_observation(
        &self,
        candidate: AuthorityInstant,
    ) -> Result<(), PolicyError> {
        if candidate.epoch().get() != self.epoch.get() {
            Err(PolicyError::clock_epoch_mismatch())
        } else if candidate.tick_millis() < self.greatest_tick_millis {
            Err(PolicyError::clock_regression())
        } else {
            Ok(())
        }
    }
    #[doc = " Consumes this linear time floor and validates one nondecreasing observation."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a typed failure that owns the unchanged time floor on epoch mismatch or regression."]
    pub const fn observe(self, candidate: AuthorityInstant) -> Result<Self, AuthorityTimeFailure> {
        if candidate.epoch().get() != self.epoch.get() {
            Err(AuthorityTimeFailure::new(PolicyError::clock_epoch_mismatch(), self))
        } else if candidate.tick_millis() < self.greatest_tick_millis {
            Err(AuthorityTimeFailure::new(PolicyError::clock_regression(), self))
        } else {
            Ok(Self { epoch: self.epoch, greatest_tick_millis: candidate.tick_millis() })
        }
    }
}
#[doc = " Failed authority-time transition that owns the unchanged move-only floor."]
#[derive(Debug, Eq, PartialEq)]
pub struct AuthorityTimeFailure {
    error: PolicyError,
    state: AuthorityTimeState,
}
impl AuthorityTimeFailure {
    pub(crate) const fn new(error: PolicyError, state: AuthorityTimeState) -> Self {
        Self { error, state }
    }
    #[doc = " Returns the exact typed time or evaluation failure."]
    #[must_use]
    pub const fn error(&self) -> PolicyError {
        self.error
    }
    #[doc = " Borrows the unchanged move-only authority-time floor."]
    #[must_use]
    pub const fn state(&self) -> &AuthorityTimeState {
        &self.state
    }
    #[doc = " Consumes the failure and returns its unchanged authority-time floor."]
    #[must_use]
    pub const fn into_state(self) -> AuthorityTimeState {
        self.state
    }
}
