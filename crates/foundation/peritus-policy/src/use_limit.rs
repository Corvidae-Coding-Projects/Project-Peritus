//! Exact unlimited or monotonically consumed logical-use bounds.

use crate::PolicyError;

#[doc = " Exact or unlimited logical use count."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct UseLimit {
    remaining: Option<u64>,
}
impl UseLimit {
    #[doc = " Returns an unlimited logical use bound."]
    #[must_use]
    pub const fn unlimited() -> Self {
        Self { remaining: None }
    }
    #[doc = " Creates a positive limited logical use bound."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a zero-use-limit failure when `remaining` is zero."]
    pub const fn limited(remaining: u64) -> Result<Self, PolicyError> {
        if remaining == 0 {
            Err(PolicyError::zero_use_limit())
        } else {
            Ok(Self { remaining: Some(remaining) })
        }
    }
    const fn exhausted() -> Self {
        Self { remaining: Some(0) }
    }
    #[doc = " Returns `None` for unlimited use or the exact remaining count."]
    #[must_use]
    pub const fn remaining(self) -> Option<u64> {
        self.remaining
    }
    #[doc = " Returns whether no limited logical use remains."]
    #[must_use]
    pub const fn is_exhausted(self) -> bool {
        matches!(self.remaining, Some(0))
    }
    #[doc = " Returns whether this bound is no less restrictive than `parent`."]
    #[must_use]
    pub const fn is_within(self, parent: Self) -> bool {
        match (self.remaining, parent.remaining) {
            (_, None) => true,
            (Some(value), Some(bound)) => value <= bound,
            (None, Some(_)) => false,
        }
    }
    #[doc = " Returns the more restrictive of two use bounds."]
    #[must_use]
    pub const fn intersection(self, other: Self) -> Self {
        match (self.remaining, other.remaining) {
            (None, None) => Self::unlimited(),
            (Some(value), None) | (None, Some(value)) => Self { remaining: Some(value) },
            (Some(left), Some(right)) => {
                Self { remaining: Some(if left <= right { left } else { right }) }
            }
        }
    }
    pub(crate) const fn decrement(self) -> Result<Self, PolicyError> {
        match self.remaining {
            None => Ok(self),
            Some(0) => Err(PolicyError::capability_exhausted()),
            Some(1) => Ok(Self::exhausted()),
            Some(value) => Ok(Self { remaining: Some(value - 1) }),
        }
    }
}
