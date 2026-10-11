//! Complete immutable policy-definition validation.

use crate::{AuthorityCeiling, OperationRegistry, RestrictionLayer};
use peritus_types::PolicyId;

pub mod construction;

#[doc = " Complete immutable policy definition evaluated by this crate."]
#[derive(Debug, Eq, PartialEq)]
pub struct PolicyDefinition {
    policy_id: PolicyId,
    ceiling: AuthorityCeiling,
    operations: OperationRegistry,
    layers: Vec<RestrictionLayer>,
}
impl PolicyDefinition {
    pub(crate) fn has_immutable_deny(&self, scope: &crate::CapabilityScope) -> bool {
        crate::evaluation_predicates::ceiling_has_immutable_deny(&self.ceiling, scope)
    }
    pub(crate) fn boundary_contains(&self, scope: &crate::CapabilityScope) -> bool {
        self.ceiling.contains_scope(scope)
    }
    pub(crate) const fn matches_policy_id(&self, scope: &crate::CapabilityScope) -> bool {
        crate::identity::identifier_values_equal(
            *self.policy_id.as_bytes(),
            *scope.policy_id().as_bytes(),
        )
    }
    pub(crate) fn has_full_coverage(&self, scope: &crate::CapabilityScope) -> bool {
        crate::evaluation_predicates::ceiling_has_full_coverage(&self.ceiling, scope)
    }
    pub(crate) fn has_restriction_deny(&self, scope: &crate::CapabilityScope) -> bool {
        crate::evaluation_predicates::has_restriction_deny(self, scope)
    }
    pub(crate) fn first_operation_denial(
        &self,
        scope: &crate::CapabilityScope,
    ) -> Option<crate::AuthorizationDenialReason> {
        self.operations.first_denial(scope)
    }
    pub(crate) fn mandatory_risks_for_scope(
        &self,
        scope: &crate::CapabilityScope,
    ) -> crate::RiskSet {
        self.operations.risks_for_scope(scope)
    }
    #[doc = " Returns this immutable policy identity."]
    #[must_use]
    pub const fn policy_id(&self) -> PolicyId {
        self.policy_id
    }
    #[doc = " Returns the protected authority ceiling."]
    #[must_use]
    pub const fn ceiling(&self) -> &AuthorityCeiling {
        &self.ceiling
    }
    #[doc = " Returns the immutable authenticated operation registry bound to this policy."]
    #[must_use]
    pub const fn operations(&self) -> &OperationRegistry {
        &self.operations
    }
    #[doc = " Borrows lower restriction layers in authority order."]
    #[must_use]
    pub const fn layers(&self) -> &[RestrictionLayer] {
        self.layers.as_slice()
    }
}
