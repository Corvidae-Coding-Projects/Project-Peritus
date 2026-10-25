//! Parent-relative selectors with exact finite wildcard semantics.

use crate::{
    ActorRole,
    identity::{actor_values_contain, environment_values_contain, role_values_contain},
};

use peritus_types::{ActorId, EnvironmentId};

mod permission;
pub use permission::PermissionSelector;
mod construction;

#[derive(Debug, Eq, PartialEq)]
pub enum SelectorValues<T> {
    AnyWithinParent,
    Exact(Vec<T>),
}
#[doc = " Actor selector whose wildcard is bounded by its containing authority ceiling."]
#[derive(Debug, Eq, PartialEq)]
pub struct ActorSelector(pub(crate) SelectorValues<ActorId>);
impl ActorSelector {
    #[doc = " Selects every actor already present in the containing boundary."]
    #[must_use]
    pub const fn any_within_parent() -> Self {
        Self(SelectorValues::AnyWithinParent)
    }
    #[doc = " Returns whether this selector uses its containing boundary."]
    #[must_use]
    pub const fn is_any_within_parent(&self) -> bool {
        matches!(self.0, SelectorValues::AnyWithinParent)
    }
    #[doc = " Borrows exact values, or returns `None` for `AnyWithinParent`."]
    #[must_use]
    pub const fn exact_values(&self) -> Option<&[ActorId]> {
        match &self.0 {
            SelectorValues::AnyWithinParent => None,
            SelectorValues::Exact(values) => Some(values.as_slice()),
        }
    }
    pub(crate) const fn exact_values_checked(&self) -> Option<&[ActorId]> {
        match &self.0 {
            SelectorValues::AnyWithinParent => None,
            SelectorValues::Exact(values) => Some(values.as_slice()),
        }
    }
    pub(crate) fn contains(&self, value: ActorId) -> bool {
        match &self.0 {
            SelectorValues::AnyWithinParent => true,
            SelectorValues::Exact(values) => actor_values_contain(values.as_slice(), value),
        }
    }
    pub(crate) fn duplicate(&self) -> Self {
        match &self.0 {
            SelectorValues::AnyWithinParent => Self::any_within_parent(),
            SelectorValues::Exact(values) => {
                let mut copied = Vec::new();
                let mut index = 0;
                while index < values.len() {
                    copied.push(values[index]);
                    index += 1;
                }

                Self(SelectorValues::Exact(copied))
            }
        }
    }
}
#[doc = " Role selector whose wildcard is bounded by its containing authority ceiling."]
#[derive(Debug, Eq, PartialEq)]
pub struct RoleSelector(pub(crate) SelectorValues<ActorRole>);
impl RoleSelector {
    #[doc = " Selects every role already present in the containing boundary."]
    #[must_use]
    pub const fn any_within_parent() -> Self {
        Self(SelectorValues::AnyWithinParent)
    }
    #[doc = " Returns whether this selector uses its containing boundary."]
    #[must_use]
    pub const fn is_any_within_parent(&self) -> bool {
        matches!(self.0, SelectorValues::AnyWithinParent)
    }
    #[doc = " Borrows exact values, or returns `None` for `AnyWithinParent`."]
    #[must_use]
    pub const fn exact_values(&self) -> Option<&[ActorRole]> {
        match &self.0 {
            SelectorValues::AnyWithinParent => None,
            SelectorValues::Exact(values) => Some(values.as_slice()),
        }
    }
    pub(crate) const fn exact_values_checked(&self) -> Option<&[ActorRole]> {
        match &self.0 {
            SelectorValues::AnyWithinParent => None,
            SelectorValues::Exact(values) => Some(values.as_slice()),
        }
    }
    pub(crate) fn contains(&self, value: ActorRole) -> bool {
        match &self.0 {
            SelectorValues::AnyWithinParent => true,
            SelectorValues::Exact(values) => role_values_contain(values.as_slice(), value),
        }
    }
    pub(crate) fn duplicate(&self) -> Self {
        match &self.0 {
            SelectorValues::AnyWithinParent => Self::any_within_parent(),
            SelectorValues::Exact(values) => {
                let mut copied = Vec::new();
                let mut index = 0;
                while index < values.len() {
                    copied.push(values[index]);
                    index += 1;
                }

                Self(SelectorValues::Exact(copied))
            }
        }
    }
}
#[doc = " Environment selector whose wildcard is bounded by its containing authority ceiling."]
#[derive(Debug, Eq, PartialEq)]
pub struct EnvironmentSelector(pub(crate) SelectorValues<EnvironmentId>);
impl EnvironmentSelector {
    #[doc = " Selects every environment already present in the containing boundary."]
    #[must_use]
    pub const fn any_within_parent() -> Self {
        Self(SelectorValues::AnyWithinParent)
    }
    #[doc = " Returns whether this selector uses its containing boundary."]
    #[must_use]
    pub const fn is_any_within_parent(&self) -> bool {
        matches!(self.0, SelectorValues::AnyWithinParent)
    }
    #[doc = " Borrows exact values, or returns `None` for `AnyWithinParent`."]
    #[must_use]
    pub const fn exact_values(&self) -> Option<&[EnvironmentId]> {
        match &self.0 {
            SelectorValues::AnyWithinParent => None,
            SelectorValues::Exact(values) => Some(values.as_slice()),
        }
    }
    pub(crate) const fn exact_values_checked(&self) -> Option<&[EnvironmentId]> {
        match &self.0 {
            SelectorValues::AnyWithinParent => None,
            SelectorValues::Exact(values) => Some(values.as_slice()),
        }
    }
    pub(crate) fn contains(&self, value: EnvironmentId) -> bool {
        match &self.0 {
            SelectorValues::AnyWithinParent => true,
            SelectorValues::Exact(values) => environment_values_contain(values.as_slice(), value),
        }
    }
    pub(crate) fn duplicate(&self) -> Self {
        match &self.0 {
            SelectorValues::AnyWithinParent => Self::any_within_parent(),
            SelectorValues::Exact(values) => {
                let mut copied = Vec::new();
                let mut index = 0;
                while index < values.len() {
                    copied.push(values[index]);
                    index += 1;
                }

                Self(SelectorValues::Exact(copied))
            }
        }
    }
}
