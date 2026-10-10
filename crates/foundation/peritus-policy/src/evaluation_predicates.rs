//! Constructive executable searches refining whole-request deny and coverage predicates.

use crate::{
    AuthorityCeiling, CapabilityScope, CeilingGrant, PolicyDefinition, RestrictionLayer,
    RestrictionRule,
};

fn grant_covers_permission_from(
    grants: &[CeilingGrant],
    requested: &CapabilityScope,
    permission: &crate::Permission,
    index: usize,
) -> bool {
    if index == grants.len() {
        false
    } else if grants[index].matches_identity(requested)
        && grants[index].contains_permission(permission)
    {
        true
    } else {
        grant_covers_permission_from(grants, requested, permission, index + 1)
    }
}
fn full_grant_coverage_from(
    permissions: &[crate::Permission],
    grants: &[CeilingGrant],
    requested: &CapabilityScope,
    index: usize,
) -> bool {
    if index == permissions.len() {
        true
    } else if !grant_covers_permission_from(grants, requested, &permissions[index], 0) {
        false
    } else {
        full_grant_coverage_from(permissions, grants, requested, index + 1)
    }
}
pub fn ceiling_has_full_coverage(ceiling: &AuthorityCeiling, requested: &CapabilityScope) -> bool {
    let permissions = requested.permissions();
    let values = permissions.as_slice();
    let grants = ceiling.grants();
    full_grant_coverage_from(values, grants, requested, 0)
}
fn deny_rule_matches_from(
    rules: &[RestrictionRule],
    requested: &CapabilityScope,
    index: usize,
) -> bool {
    if index == rules.len() {
        false
    } else if rules[index].is_deny() && rules[index].matches_scope(requested) {
        true
    } else {
        deny_rule_matches_from(rules, requested, index + 1)
    }
}
pub fn ceiling_has_immutable_deny(ceiling: &AuthorityCeiling, requested: &CapabilityScope) -> bool {
    deny_rule_matches_from(ceiling.immutable_denies(), requested, 0)
}
fn restriction_deny_matches_from(
    layers: &[RestrictionLayer],
    requested: &CapabilityScope,
    index: usize,
) -> bool {
    if index == layers.len() {
        false
    } else if deny_rule_matches_from(layers[index].rules(), requested, 0) {
        true
    } else {
        restriction_deny_matches_from(layers, requested, index + 1)
    }
}
pub fn has_restriction_deny(policy: &PolicyDefinition, requested: &CapabilityScope) -> bool {
    restriction_deny_matches_from(policy.layers(), requested, 0)
}
