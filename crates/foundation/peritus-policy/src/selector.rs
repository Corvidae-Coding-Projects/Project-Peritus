//! Parent-relative selectors with exact finite wildcard semantics.

use crate::{
    identity::{
        actor_values_contain, environment_values_contain, role_values_contain,
    },
    ActorRole,
};
#[cfg(verus_only)]
use crate::identity::{
    actor_values_spec_contains, environment_values_spec_contains, role_values_spec_contains,
};
use peritus_types::{ActorId, EnvironmentId};
use vstd::prelude::*;

mod permission;
pub use permission::PermissionSelector;
mod construction;

verus! {

#[derive(Debug, Eq, PartialEq)]
pub enum SelectorValues<T> {
    AnyWithinParent,
    Exact(Vec<T>),
}

/// Actor selector whose wildcard is bounded by its containing authority ceiling.
#[derive(Debug, Eq, PartialEq)]
pub struct ActorSelector(pub(crate) SelectorValues<ActorId>);

impl ActorSelector {
    /// Returns the exact actor selector values, or `None` for the parent-bounded wildcard.
    pub closed spec fn spec_exact_values(&self) -> Option<Seq<ActorId>> {
        self.spec_exact_values_internal()
    }

    pub(crate) open spec fn spec_exact_values_internal(&self) -> Option<Seq<ActorId>> {
        match self.0 {
            SelectorValues::AnyWithinParent => None,
            SelectorValues::Exact(values) => Some(values@),
        }
    }

    pub(crate) closed spec fn spec_same_as(&self, other: &Self) -> bool {
        self.spec_exact_values() == other.spec_exact_values()
    }
    /// Returns exact finite actor membership used by policy specifications.
    pub closed spec fn spec_contains(&self, value: ActorId) -> bool {
        match self.0 {
            SelectorValues::AnyWithinParent => true,
            SelectorValues::Exact(values) => actor_values_spec_contains(values@, value),
        }
    }

    /// Selects every actor already present in the containing boundary.
    #[must_use]
    pub const fn any_within_parent() -> (selector: Self)
        ensures selector.spec_exact_values().is_none(),
    {
        Self(SelectorValues::AnyWithinParent)
    }

    /// Returns whether this selector uses its containing boundary.
    #[must_use]
    pub const fn is_any_within_parent(&self) -> bool {
        matches!(self.0, SelectorValues::AnyWithinParent)
    }

    /// Borrows exact values, or returns `None` for `AnyWithinParent`.
    #[must_use]
    pub const fn exact_values(&self) -> (values: Option<&[ActorId]>)
        ensures match values {
            Some(values) => self.spec_exact_values() == Some(values@),
            None => self.spec_exact_values().is_none(),
        },
    {
        match &self.0 {
            SelectorValues::AnyWithinParent => None,
            SelectorValues::Exact(values) => Some(values.as_slice()),
        }
    }

    pub(crate) const fn exact_values_checked(&self) -> (values: Option<&[ActorId]>)
        ensures match values {
            Some(values) => self.spec_exact_values_internal() == Some(values@),
            None => self.spec_exact_values_internal().is_none(),
        },
    {
        match &self.0 {
            SelectorValues::AnyWithinParent => None,
            SelectorValues::Exact(values) => Some(values.as_slice()),
        }
    }

    pub(crate) fn contains(&self, value: ActorId) -> (result: bool)
        ensures result == self.spec_contains(value),
    {
        match &self.0 {
            SelectorValues::AnyWithinParent => true,
            SelectorValues::Exact(values) => actor_values_contain(values.as_slice(), value),
        }
    }

    pub(crate) fn duplicate(&self) -> (duplicate: Self)
        ensures duplicate.spec_same_as(self),
    {
        reveal(ActorSelector::spec_same_as);
        reveal(ActorSelector::spec_exact_values);
        match &self.0 {
            SelectorValues::AnyWithinParent => {
                assert(self.0 is AnyWithinParent);
                assert(self.spec_exact_values().is_none());
                let duplicate = Self::any_within_parent();
                assert(duplicate.spec_same_as(self));
                duplicate
            }
            SelectorValues::Exact(values) => {
                let mut copied = Vec::new();
                let mut index = 0;
                while index < values.len()
                    invariant
                        0 <= index <= values.len(),
                        copied@ == values@.subrange(0, index as int),
                    decreases values.len() - index,
                {
                    copied.push(values[index]);
                    index += 1;
                }
                assert(copied@ == values@);
                let duplicate = Self(SelectorValues::Exact(copied));
                assert(duplicate.spec_same_as(self));
                duplicate
            }
        }
    }
}

/// Role selector whose wildcard is bounded by its containing authority ceiling.
#[derive(Debug, Eq, PartialEq)]
pub struct RoleSelector(pub(crate) SelectorValues<ActorRole>);

impl RoleSelector {
    /// Returns the exact role selector values, or `None` for the parent-bounded wildcard.
    pub closed spec fn spec_exact_values(&self) -> Option<Seq<ActorRole>> {
        self.spec_exact_values_internal()
    }

    pub(crate) open spec fn spec_exact_values_internal(&self) -> Option<Seq<ActorRole>> {
        match self.0 {
            SelectorValues::AnyWithinParent => None,
            SelectorValues::Exact(values) => Some(values@),
        }
    }

    pub(crate) closed spec fn spec_same_as(&self, other: &Self) -> bool {
        self.spec_exact_values() == other.spec_exact_values()
    }
    /// Returns exact finite role membership used by policy specifications.
    pub closed spec fn spec_contains(&self, value: ActorRole) -> bool {
        match self.0 {
            SelectorValues::AnyWithinParent => true,
            SelectorValues::Exact(values) => role_values_spec_contains(values@, value),
        }
    }

    /// Selects every role already present in the containing boundary.
    #[must_use]
    pub const fn any_within_parent() -> (selector: Self)
        ensures selector.spec_exact_values().is_none(),
    {
        Self(SelectorValues::AnyWithinParent)
    }

    /// Returns whether this selector uses its containing boundary.
    #[must_use]
    pub const fn is_any_within_parent(&self) -> bool {
        matches!(self.0, SelectorValues::AnyWithinParent)
    }

    /// Borrows exact values, or returns `None` for `AnyWithinParent`.
    #[must_use]
    pub const fn exact_values(&self) -> (values: Option<&[ActorRole]>)
        ensures match values {
            Some(values) => self.spec_exact_values() == Some(values@),
            None => self.spec_exact_values().is_none(),
        },
    {
        match &self.0 {
            SelectorValues::AnyWithinParent => None,
            SelectorValues::Exact(values) => Some(values.as_slice()),
        }
    }

    pub(crate) const fn exact_values_checked(&self) -> (values: Option<&[ActorRole]>)
        ensures match values {
            Some(values) => self.spec_exact_values_internal() == Some(values@),
            None => self.spec_exact_values_internal().is_none(),
        },
    {
        match &self.0 {
            SelectorValues::AnyWithinParent => None,
            SelectorValues::Exact(values) => Some(values.as_slice()),
        }
    }

    pub(crate) fn contains(&self, value: ActorRole) -> (result: bool)
        ensures result == self.spec_contains(value),
    {
        match &self.0 {
            SelectorValues::AnyWithinParent => true,
            SelectorValues::Exact(values) => role_values_contain(values.as_slice(), value),
        }
    }

    pub(crate) fn duplicate(&self) -> (duplicate: Self)
        ensures duplicate.spec_same_as(self),
    {
        reveal(RoleSelector::spec_same_as);
        reveal(RoleSelector::spec_exact_values);
        match &self.0 {
            SelectorValues::AnyWithinParent => {
                assert(self.0 is AnyWithinParent);
                assert(self.spec_exact_values().is_none());
                let duplicate = Self::any_within_parent();
                assert(duplicate.spec_same_as(self));
                duplicate
            }
            SelectorValues::Exact(values) => {
                let mut copied = Vec::new();
                let mut index = 0;
                while index < values.len()
                    invariant
                        0 <= index <= values.len(),
                        copied@ == values@.subrange(0, index as int),
                    decreases values.len() - index,
                {
                    copied.push(values[index]);
                    index += 1;
                }
                assert(copied@ == values@);
                let duplicate = Self(SelectorValues::Exact(copied));
                assert(duplicate.spec_same_as(self));
                duplicate
            }
        }
    }
}

/// Environment selector whose wildcard is bounded by its containing authority ceiling.
#[derive(Debug, Eq, PartialEq)]
pub struct EnvironmentSelector(pub(crate) SelectorValues<EnvironmentId>);

impl EnvironmentSelector {
    /// Returns the exact environment selector values, or `None` for the parent-bounded wildcard.
    pub closed spec fn spec_exact_values(&self) -> Option<Seq<EnvironmentId>> {
        self.spec_exact_values_internal()
    }

    pub(crate) open spec fn spec_exact_values_internal(&self) -> Option<Seq<EnvironmentId>> {
        match self.0 {
            SelectorValues::AnyWithinParent => None,
            SelectorValues::Exact(values) => Some(values@),
        }
    }

    pub(crate) closed spec fn spec_same_as(&self, other: &Self) -> bool {
        self.spec_exact_values() == other.spec_exact_values()
    }
    /// Returns exact finite environment membership used by policy specifications.
    pub closed spec fn spec_contains(&self, value: EnvironmentId) -> bool {
        match self.0 {
            SelectorValues::AnyWithinParent => true,
            SelectorValues::Exact(values) => environment_values_spec_contains(values@, value),
        }
    }

    /// Selects every environment already present in the containing boundary.
    #[must_use]
    pub const fn any_within_parent() -> (selector: Self)
        ensures selector.spec_exact_values().is_none(),
    {
        Self(SelectorValues::AnyWithinParent)
    }

    /// Returns whether this selector uses its containing boundary.
    #[must_use]
    pub const fn is_any_within_parent(&self) -> bool {
        matches!(self.0, SelectorValues::AnyWithinParent)
    }

    /// Borrows exact values, or returns `None` for `AnyWithinParent`.
    #[must_use]
    pub const fn exact_values(&self) -> (values: Option<&[EnvironmentId]>)
        ensures match values {
            Some(values) => self.spec_exact_values() == Some(values@),
            None => self.spec_exact_values().is_none(),
        },
    {
        match &self.0 {
            SelectorValues::AnyWithinParent => None,
            SelectorValues::Exact(values) => Some(values.as_slice()),
        }
    }

    pub(crate) const fn exact_values_checked(&self) -> (values: Option<&[EnvironmentId]>)
        ensures match values {
            Some(values) => self.spec_exact_values_internal() == Some(values@),
            None => self.spec_exact_values_internal().is_none(),
        },
    {
        match &self.0 {
            SelectorValues::AnyWithinParent => None,
            SelectorValues::Exact(values) => Some(values.as_slice()),
        }
    }

    pub(crate) fn contains(&self, value: EnvironmentId) -> (result: bool)
        ensures result == self.spec_contains(value),
    {
        match &self.0 {
            SelectorValues::AnyWithinParent => true,
            SelectorValues::Exact(values) => {
                environment_values_contain(values.as_slice(), value)
            }
        }
    }

    pub(crate) fn duplicate(&self) -> (duplicate: Self)
        ensures duplicate.spec_same_as(self),
    {
        reveal(EnvironmentSelector::spec_same_as);
        reveal(EnvironmentSelector::spec_exact_values);
        match &self.0 {
            SelectorValues::AnyWithinParent => {
                assert(self.0 is AnyWithinParent);
                assert(self.spec_exact_values().is_none());
                let duplicate = Self::any_within_parent();
                assert(duplicate.spec_same_as(self));
                duplicate
            }
            SelectorValues::Exact(values) => {
                let mut copied = Vec::new();
                let mut index = 0;
                while index < values.len()
                    invariant
                        0 <= index <= values.len(),
                        copied@ == values@.subrange(0, index as int),
                    decreases values.len() - index,
                {
                    copied.push(values[index]);
                    index += 1;
                }
                assert(copied@ == values@);
                let duplicate = Self(SelectorValues::Exact(copied));
                assert(duplicate.spec_same_as(self));
                duplicate
            }
        }
    }
}

} // verus!
