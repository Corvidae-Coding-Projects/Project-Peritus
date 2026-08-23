//! Epoch-bound authority instants and half-open validity windows.

use crate::PolicyError;
use peritus_types::Generation;
use vstd::prelude::*;

verus! {

/// An observation from one monotonic authority-clock epoch.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AuthorityInstant {
    epoch: Generation,
    tick_millis: u64,
}

impl AuthorityInstant {
    /// Returns the exact epoch used by specifications.
    pub closed spec fn spec_epoch(&self) -> int { self.epoch.spec_value() }

    /// Returns the exact monotonic tick used by specifications.
    pub closed spec fn spec_tick_millis(&self) -> int { self.tick_millis as int }

    /// Creates an exact authority-clock observation.
    #[must_use]
    pub const fn new(epoch: Generation, tick_millis: u64) -> (instant: Self)
        ensures
            instant.spec_epoch() == epoch.spec_value(),
            instant.spec_tick_millis() == tick_millis as int,
    {
        Self { epoch, tick_millis }
    }

    /// Returns the authority-clock epoch.
    #[must_use]
    pub const fn epoch(self) -> (epoch: Generation)
        ensures epoch.spec_value() == self.spec_epoch(),
    { self.epoch }

    /// Returns the monotonic millisecond tick within the epoch.
    #[must_use]
    pub const fn tick_millis(self) -> (tick: u64)
        ensures tick as int == self.spec_tick_millis(),
    { self.tick_millis }

    /// Adds a duration without wrapping or crossing epochs.
    ///
    /// # Errors
    ///
    /// Returns a time-overflow failure when the exact tick is not representable.
    pub const fn checked_add(
        self,
        duration_millis: u64,
    ) -> (result: Result<Self, PolicyError>)
        ensures
            match result {
                Ok(next) => {
                    next.spec_epoch() == self.spec_epoch()
                        && next.spec_tick_millis()
                            == self.spec_tick_millis() + duration_millis as int
                }
                Err(error) => {
                    error.spec_kind() == crate::PolicyErrorKind::TimeOverflow
                        && error.spec_dimension().is_none()
                        && self.spec_tick_millis() + duration_millis as int
                            > u64::MAX as int
                }
            },
    {
        if duration_millis > u64::MAX - self.tick_millis {
            assert(self.tick_millis as int + duration_millis as int
                > u64::MAX as int);
            Err(PolicyError::time_overflow())
        } else {
            let tick_millis = self.tick_millis + duration_millis;
            assert(tick_millis as int
                == self.tick_millis as int + duration_millis as int);
            let next = Self { epoch: self.epoch, tick_millis };
            assert(next.spec_epoch() == self.spec_epoch());
            assert(next.spec_tick_millis()
                == self.spec_tick_millis() + duration_millis as int);
            Ok(next)
        }
    }
}

/// A nonempty half-open authority interval, `not_before <= now < expires_at`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ValidityWindow {
    not_before: AuthorityInstant,
    expires_at: AuthorityInstant,
}

impl ValidityWindow {
    /// Returns the exact inclusive bound used by specifications.
    pub closed spec fn spec_not_before(&self) -> AuthorityInstant { self.not_before }

    /// Returns the exact exclusive bound used by specifications.
    pub closed spec fn spec_expires_at(&self) -> AuthorityInstant { self.expires_at }

    /// Returns whether an exact authority instant is in this half-open interval.
    pub open spec fn spec_contains(&self, instant: AuthorityInstant) -> bool {
        instant.spec_epoch() == self.spec_not_before().spec_epoch()
            && self.spec_not_before().spec_tick_millis() <= instant.spec_tick_millis()
            && instant.spec_tick_millis() < self.spec_expires_at().spec_tick_millis()
    }

    /// Returns exact interval containment used by scope-boundary specifications.
    pub open spec fn spec_is_within(&self, parent: Self) -> bool {
        self.spec_not_before().spec_epoch() == parent.spec_not_before().spec_epoch()
            && self.spec_not_before().spec_tick_millis()
                >= parent.spec_not_before().spec_tick_millis()
            && self.spec_expires_at().spec_tick_millis()
                <= parent.spec_expires_at().spec_tick_millis()
    }

    /// Creates a nonempty half-open interval in one authority-clock epoch.
    ///
    /// # Errors
    ///
    /// Returns an invalid-window failure for different epochs or when
    /// `not_before >= expires_at`.
    pub const fn new(
        not_before: AuthorityInstant,
        expires_at: AuthorityInstant,
    ) -> (result: Result<Self, PolicyError>)
        ensures
            match result {
                Ok(window) => {
                    window.spec_not_before() == not_before
                        && window.spec_expires_at() == expires_at
                        && not_before.spec_epoch() == expires_at.spec_epoch()
                        && not_before.spec_tick_millis() < expires_at.spec_tick_millis()
                }
                Err(error) => {
                    error.spec_kind() == crate::PolicyErrorKind::InvalidValidityWindow
                        && (not_before.spec_epoch() != expires_at.spec_epoch()
                        || not_before.spec_tick_millis() >= expires_at.spec_tick_millis()
                        )
                }
            },
    {
        if not_before.epoch.get() != expires_at.epoch.get()
            || not_before.tick_millis >= expires_at.tick_millis
        {
            Err(PolicyError::invalid_validity_window())
        } else {
            Ok(Self { not_before, expires_at })
        }
    }

    /// Returns the inclusive lower bound.
    #[must_use]
    pub const fn not_before(self) -> (instant: AuthorityInstant)
        ensures instant == self.spec_not_before(),
    { self.not_before }

    /// Returns the exclusive upper bound.
    #[must_use]
    pub const fn expires_at(self) -> (instant: AuthorityInstant)
        ensures instant == self.spec_expires_at(),
    { self.expires_at }

    /// Checks an instant against the half-open interval.
    ///
    /// # Errors
    ///
    /// Returns a clock-epoch mismatch when the observation is from another epoch.
    pub const fn contains(
        self,
        instant: AuthorityInstant,
    ) -> (result: Result<bool, PolicyError>)
        ensures
            match result {
                Ok(contains) => {
                    instant.spec_epoch() == self.spec_not_before().spec_epoch()
                        && contains == self.spec_contains(instant)
                }
                Err(error) => {
                    instant.spec_epoch() != self.spec_not_before().spec_epoch()
                        && error.spec_kind() == crate::PolicyErrorKind::ClockEpochMismatch
                        && error.spec_dimension().is_none()
                        && error.spec_collection().is_none()
                }
            },
    {
        if instant.epoch.get() == self.not_before.epoch.get() {
            Ok(self.not_before.tick_millis <= instant.tick_millis
                && instant.tick_millis < self.expires_at.tick_millis)
        } else {
            Err(PolicyError::clock_epoch_mismatch())
        }
    }

    /// Returns whether this entire interval is contained by `parent`.
    #[must_use]
    pub const fn is_within(self, parent: Self) -> (contained: bool)
        ensures contained == self.spec_is_within(parent),
    {
        self.not_before.epoch.get() == parent.not_before.epoch.get()
            && self.not_before.tick_millis >= parent.not_before.tick_millis
            && self.expires_at.tick_millis <= parent.expires_at.tick_millis
    }

    /// Returns the exact nonempty intersection of two windows.
    ///
    /// # Errors
    ///
    /// Returns a typed time failure when epochs differ or the intersection is empty.
    pub const fn intersection(self, other: Self) -> (result: Result<Self, PolicyError>)
        ensures
            match result {
                Ok(value) => {
                    !crate::approval_model::window_intersection_conflict(self, other)
                        && value.spec_not_before().spec_epoch()
                            == value.spec_expires_at().spec_epoch()
                        && value.spec_not_before().spec_tick_millis()
                            < value.spec_expires_at().spec_tick_millis()
                        && value.spec_not_before().spec_epoch()
                            == crate::approval_model::intersection_not_before_epoch(self, other)
                        && value.spec_not_before().spec_tick_millis()
                            == crate::model::maximum_int(
                                self.spec_not_before().spec_tick_millis(),
                                other.spec_not_before().spec_tick_millis(),
                            )
                        && value.spec_expires_at().spec_tick_millis()
                            == crate::model::minimum_int(
                                self.spec_expires_at().spec_tick_millis(),
                                other.spec_expires_at().spec_tick_millis(),
                            )
                        && value.spec_expires_at().spec_epoch()
                            == crate::approval_model::intersection_expires_epoch(self, other)
                }
                Err(error) => {
                    crate::approval_model::window_intersection_conflict(self, other)
                        && if self.spec_not_before().spec_epoch()
                            != other.spec_not_before().spec_epoch()
                        {
                            error.spec_kind() == crate::PolicyErrorKind::ClockEpochMismatch
                        } else {
                            error.spec_kind() == crate::PolicyErrorKind::InvalidValidityWindow
                        }
                }
            },
    {
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
        assert(not_before.spec_epoch()
            == crate::approval_model::intersection_not_before_epoch(self, other));
        assert(not_before.spec_tick_millis() == crate::model::maximum_int(
            self.spec_not_before().spec_tick_millis(),
            other.spec_not_before().spec_tick_millis(),
        ));
        assert(expires_at.spec_tick_millis() == crate::model::minimum_int(
            self.spec_expires_at().spec_tick_millis(),
            other.spec_expires_at().spec_tick_millis(),
        ));
        assert(expires_at.spec_epoch()
            == crate::approval_model::intersection_expires_epoch(self, other));
        let result = Self::new(not_before, expires_at);
        match &result {
            Ok(_value) => {
                assert(!crate::approval_model::window_intersection_conflict(self, other));
                assert(_value.spec_not_before() == not_before);
                assert(_value.spec_expires_at() == expires_at);
            }
            Err(_) => {
                assert(crate::approval_model::window_intersection_conflict(self, other));
            }
        }
        result
    }
}

} // verus!
