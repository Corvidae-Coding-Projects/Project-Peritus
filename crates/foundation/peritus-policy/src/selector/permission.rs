//! Parent-bounded exact permission selector.

use crate::{Permission, PermissionSet};

#[doc = " Permission selector whose wildcard is bounded by its containing authority ceiling."]
#[derive(Debug, Eq, PartialEq)]
pub struct PermissionSelector {
    pub(crate) exact: Option<PermissionSet>,
}
impl PermissionSelector {
    #[doc = " Selects every exact pair already present in the containing boundary."]
    #[must_use]
    pub const fn any_within_parent() -> Self {
        Self { exact: None }
    }
    #[doc = " Selects one checked canonical permission set."]
    #[must_use]
    pub const fn exact(values: PermissionSet) -> Self {
        Self { exact: Some(values) }
    }
    #[doc = " Returns whether this selector uses its containing boundary."]
    #[must_use]
    pub const fn is_any_within_parent(&self) -> bool {
        self.exact.is_none()
    }
    #[doc = " Returns the checked exact permission set, or `None` for `AnyWithinParent`."]
    #[must_use]
    pub const fn exact_values(&self) -> Option<&PermissionSet> {
        self.exact.as_ref()
    }
    pub(crate) const fn exact_values_internal(&self) -> Option<&PermissionSet> {
        self.exact.as_ref()
    }
    pub(crate) fn contains(&self, permission: &Permission) -> bool {
        let Some(values) = &self.exact else {
            return true;
        };
        values.contains(permission)
    }
    pub(crate) fn duplicate(&self) -> Self {
        Self { exact: self.exact.as_ref().map(PermissionSet::duplicate) }
    }
}
