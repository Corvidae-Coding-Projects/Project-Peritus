//! Epoch-bound authority instants and half-open validity windows.

use crate::PolicyError;
use peritus_types::Generation;

#[doc = " An observation from one monotonic authority-clock epoch."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AuthorityInstant {
    epoch: Generation,
    tick_millis: u64,
}
impl AuthorityInstant {
    #[doc = " Creates an exact authority-clock observation."]
    #[must_use]
    pub const fn new(epoch: Generation, tick_millis: u64) -> Self {
        Self { epoch, tick_millis }
    }
    #[doc = " Returns the authority-clock epoch."]
    #[must_use]
    pub const fn epoch(self) -> Generation {
        self.epoch
    }
    #[doc = " Returns the monotonic millisecond tick within the epoch."]
    #[must_use]
    pub const fn tick_millis(self) -> u64 {
        self.tick_millis
    }
    #[doc = " Adds a duration without wrapping or crossing epochs."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a time-overflow failure when the exact tick is not representable."]
    pub const fn checked_add(self, duration_millis: u64) -> Result<Self, PolicyError> {
        if duration_millis > u64::MAX - self.tick_millis {
            Err(PolicyError::time_overflow())
        } else {
            let tick_millis = self.tick_millis + duration_millis;
            let next = Self { epoch: self.epoch, tick_millis };
            Ok(next)
        }
    }
}
#[doc = " A nonempty half-open authority interval, `not_before <= now < expires_at`."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ValidityWindow {
    not_before: AuthorityInstant,
    expires_at: AuthorityInstant,
}
impl ValidityWindow {
    #[doc = " Creates a nonempty half-open interval in one authority-clock epoch."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns an invalid-window failure for different epochs or when"]
    #[doc = " `not_before >= expires_at`."]
    pub const fn new(
        not_before: AuthorityInstant,
        expires_at: AuthorityInstant,
    ) -> Result<Self, PolicyError> {
        if not_before.epoch.get() != expires_at.epoch.get()
            || not_before.tick_millis >= expires_at.tick_millis
        {
            Err(PolicyError::invalid_validity_window())
        } else {
            Ok(Self { not_before, expires_at })
        }
    }
    #[doc = " Returns the inclusive lower bound."]
    #[must_use]
    pub const fn not_before(self) -> AuthorityInstant {
        self.not_before
    }
    #[doc = " Returns the exclusive upper bound."]
    #[must_use]
    pub const fn expires_at(self) -> AuthorityInstant {
        self.expires_at
    }
    #[doc = " Checks an instant against the half-open interval."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a clock-epoch mismatch when the observation is from another epoch."]
    pub const fn contains(self, instant: AuthorityInstant) -> Result<bool, PolicyError> {
        if instant.epoch.get() == self.not_before.epoch.get() {
            Ok(self.not_before.tick_millis <= instant.tick_millis
                && instant.tick_millis < self.expires_at.tick_millis)
        } else {
            Err(PolicyError::clock_epoch_mismatch())
        }
    }
    #[doc = " Returns whether this entire interval is contained by `parent`."]
    #[must_use]
    pub const fn is_within(self, parent: Self) -> bool {
        self.not_before.epoch.get() == parent.not_before.epoch.get()
            && self.not_before.tick_millis >= parent.not_before.tick_millis
            && self.expires_at.tick_millis <= parent.expires_at.tick_millis
    }
    #[doc = " Returns the exact nonempty intersection of two windows."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a typed time failure when epochs differ or the intersection is empty."]
    pub const fn intersection(self, other: Self) -> Result<Self, PolicyError> {
        if self.not_before.epoch.get() != other.not_before.epoch.get() {
            return Err(PolicyError::clock_epoch_mismatch());
        }
        let not_before = if self.not_before.tick_millis >= other.not_before.tick_millis {
            self.not_before
        } else {
            other.not_before
        };
        let expires_at = if self.expires_at.tick_millis <= other.expires_at.tick_millis {
            self.expires_at
        } else {
            other.expires_at
        };
        let result = Self::new(not_before, expires_at);
        if let Ok(_value) = &result {}
        result
    }
}
