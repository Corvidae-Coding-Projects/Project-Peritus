//! Exact unlimited or monotonically consumed logical-use bounds.

use crate::PolicyError;
use vstd::prelude::*;

verus! {

/// Exact or unlimited logical use count.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct UseLimit {
    remaining: Option<u64>,
}

impl UseLimit {
    /// Returns the exact remaining count used by specifications.
    pub closed spec fn spec_remaining(&self) -> Option<int> {
        match self.remaining {
            None => None,
            Some(value) => Some(value as int),
        }
    }

    /// Returns exact authority containment for finite and unlimited bounds.
    pub open spec fn spec_is_within(&self, parent: Self) -> bool {
        match (self.spec_remaining(), parent.spec_remaining()) {
            (_, None) => true,
            (Some(value), Some(bound)) => value <= bound,
            (None, Some(_)) => false,
        }
    }

    /// Returns an unlimited logical use bound.
    #[must_use]
    pub const fn unlimited() -> (limit: Self)
        ensures limit.spec_remaining().is_none(),
    { Self { remaining: None } }

    /// Creates a positive limited logical use bound.
    ///
    /// # Errors
    ///
    /// Returns a zero-use-limit failure when `remaining` is zero.
    pub const fn limited(remaining: u64) -> Result<Self, PolicyError> {
        if remaining == 0 {
            Err(PolicyError::zero_use_limit())
        } else {
            Ok(Self { remaining: Some(remaining) })
        }
    }

    const fn exhausted() -> (limit: Self)
        ensures limit.spec_remaining() == Some(0),
    { Self { remaining: Some(0) } }

    /// Returns `None` for unlimited use or the exact remaining count.
    #[must_use]
    pub const fn remaining(self) -> (remaining: Option<u64>)
        ensures
            match remaining {
                None => self.spec_remaining().is_none(),
                Some(value) => self.spec_remaining() == Some(value as int),
            },
    { self.remaining }

    /// Returns whether no limited logical use remains.
    #[must_use]
    pub const fn is_exhausted(self) -> bool { matches!(self.remaining, Some(0)) }

    /// Returns whether this bound is no less restrictive than `parent`.
    #[must_use]
    pub const fn is_within(self, parent: Self) -> (contained: bool)
        ensures contained == self.spec_is_within(parent),
    {
        match (self.remaining, parent.remaining) {
            (_, None) => true,
            (Some(value), Some(bound)) => value <= bound,
            (None, Some(_)) => false,
        }
    }

    /// Returns the more restrictive of two use bounds.
    #[must_use]
    pub const fn intersection(self, other: Self) -> (result: Self)
        ensures
            result.spec_remaining()
                == crate::model::minimum_use_limit(
                    self.spec_remaining(),
                    other.spec_remaining(),
                ),
    {
        match (self.remaining, other.remaining) {
            (None, None) => Self::unlimited(),
            (Some(value), None) | (None, Some(value)) => Self { remaining: Some(value) },
            (Some(left), Some(right)) => {
                Self { remaining: Some(if left <= right { left } else { right }) }
            }
        }
    }

    pub(crate) const fn decrement(self) -> (result: Result<Self, PolicyError>)
        ensures
            match result {
                Ok(next) => crate::model::use_limit_successor(
                    self.spec_remaining(),
                    next.spec_remaining(),
                ),
                Err(error) => {
                    self.spec_remaining() == Some(0)
                        && error.spec_kind() == crate::PolicyErrorKind::CapabilityExhausted
                        && error.spec_dimension().is_none()
                        && error.spec_collection().is_none()
                }
            },
    {
        match self.remaining {
            None => {
                assert(self.spec_remaining().is_none());
                Ok(self)
            }
            Some(0) => {
                assert(self.spec_remaining() == Some(0));
                Err(PolicyError::capability_exhausted())
            }
            Some(1) => {
                assert(self.spec_remaining() == Some(1));
                Ok(Self::exhausted())
            }
            Some(value) => {
                assert(value > 1);
                assert(self.spec_remaining() == Some(value as int));
                Ok(Self { remaining: Some(value - 1) })
            }
        }
    }
}

} // verus!
