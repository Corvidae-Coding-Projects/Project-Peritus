//! Ordered exact parent-boundary containment for complete selectors.

use super::ScopeSelector;
use crate::{ScopeDimension, identity::revision_values_equal};

impl ScopeSelector {
    pub(crate) fn first_boundary_mismatch(
        &self,
        boundary: &crate::AuthorityBoundary,
    ) -> Option<ScopeDimension> {
        if !revision_values_equal(self.revision, *boundary.revision()) {
            return Some(ScopeDimension::Revision);
        }
        if actors_outside(self, boundary) {
            return Some(ScopeDimension::Actor);
        }
        if roles_outside(self, boundary) {
            return Some(ScopeDimension::Role);
        }
        if environments_outside(self, boundary) {
            return Some(ScopeDimension::Environment);
        }
        if permissions_outside(self, boundary) {
            return Some(ScopeDimension::Permissions);
        }
        None
    }
}
fn actors_outside(selector: &ScopeSelector, boundary: &crate::AuthorityBoundary) -> bool {
    let Some(values) = selector.actors.exact_values_checked() else {
        return false;
    };
    let mut index = 0;
    while index < values.len() {
        if !boundary.contains_actor(values[index]) {
            return true;
        }
        index += 1;
    }
    false
}
fn roles_outside(selector: &ScopeSelector, boundary: &crate::AuthorityBoundary) -> bool {
    let Some(values) = selector.roles.exact_values_checked() else {
        return false;
    };
    let mut index = 0;
    while index < values.len() {
        if !boundary.contains_role(values[index]) {
            return true;
        }
        index += 1;
    }
    false
}
fn environments_outside(selector: &ScopeSelector, boundary: &crate::AuthorityBoundary) -> bool {
    let Some(values) = selector.environments.exact_values_checked() else {
        return false;
    };
    let mut index = 0;
    while index < values.len() {
        if !boundary.contains_environment(values[index]) {
            return true;
        }
        index += 1;
    }
    false
}
fn permissions_outside(selector: &ScopeSelector, boundary: &crate::AuthorityBoundary) -> bool {
    let Some(values) = selector.permissions.exact_values_internal() else {
        return false;
    };
    let values = values.as_slice();
    let mut index = 0;
    while index < values.len() {
        if !boundary.contains_permission(&values[index]) {
            return true;
        }
        index += 1;
    }
    false
}
