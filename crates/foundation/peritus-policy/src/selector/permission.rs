//! Parent-bounded exact permission selector.

use crate::{Permission, PermissionSet};
use vstd::prelude::*;

verus! {

/// Permission selector whose wildcard is bounded by its containing authority ceiling.
#[derive(Debug, Eq, PartialEq)]
pub struct PermissionSelector {
    pub(crate) exact: Option<PermissionSet>,
}

impl PermissionSelector {
    /// Returns the exact permission sequence, or `None` for the parent-bounded wildcard.
    pub closed spec fn spec_exact_values(&self) -> Option<Seq<Permission>> {
        self.spec_exact_values_internal()
    }

    pub(crate) open spec fn spec_exact_values_internal(&self) -> Option<Seq<Permission>> {
        match self.exact {
            None => None,
            Some(values) => Some(values.spec_values()),
        }
    }

    pub(crate) closed spec fn spec_same_as(&self, other: &Self) -> bool {
        match (&self.exact, &other.exact) {
            (None, None) => true,
            (Some(left), Some(right)) => left.spec_same_as(right),
            _ => false,
        }
    }

    /// Returns exact permission membership used by policy specifications.
    pub closed spec fn spec_contains(&self, permission: &Permission) -> bool {
        match self.exact {
            None => true,
            Some(values) => values.spec_contains(permission),
        }
    }

    /// Selects every exact pair already present in the containing boundary.
    #[must_use]
    pub const fn any_within_parent() -> (selector: Self)
        ensures selector.spec_exact_values().is_none(),
    { Self { exact: None } }

    /// Selects one checked canonical permission set.
    #[must_use]
    pub const fn exact(values: PermissionSet) -> (selector: Self)
        ensures selector.spec_exact_values() == Some(values.spec_values()),
    { Self { exact: Some(values) } }

    /// Returns whether this selector uses its containing boundary.
    #[must_use]
    pub const fn is_any_within_parent(&self) -> bool { self.exact.is_none() }

    /// Returns the checked exact permission set, or `None` for `AnyWithinParent`.
    #[must_use]
    pub const fn exact_values(&self) -> (values: Option<&PermissionSet>)
        ensures match values {
            Some(values) => self.spec_exact_values() == Some(values.spec_values()),
            None => self.spec_exact_values().is_none(),
        },
    { self.exact.as_ref() }

    pub(crate) const fn exact_values_internal(&self) -> (values: Option<&PermissionSet>)
        ensures match values {
            Some(values) => {
                self.spec_exact_values() == Some(values.spec_values())
                    && self.spec_exact_values_internal() == Some(values.spec_values())
            }
            None => {
                self.spec_exact_values().is_none()
                    && self.spec_exact_values_internal().is_none()
            }
        },
    { self.exact.as_ref() }

    pub(crate) fn contains(&self, permission: &Permission) -> (result: bool)
        ensures result == self.spec_contains(permission),
    {
        let Some(values) = &self.exact else {
            return true;
        };
        values.contains(permission)
    }

    pub(crate) fn duplicate(&self) -> (duplicate: Self)
        ensures duplicate.spec_same_as(self),
    {
        reveal(PermissionSelector::spec_same_as);
        Self { exact: self.exact.as_ref().map(PermissionSet::duplicate) }
    }
}

} // verus!
