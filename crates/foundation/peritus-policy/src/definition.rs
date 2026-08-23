//! Complete immutable policy-definition validation.

use crate::{AuthorityCeiling, OperationRegistry, RestrictionLayer};
use peritus_types::PolicyId;
use vstd::prelude::*;

pub mod construction;

verus! {

/// Complete immutable policy definition evaluated by this crate.
#[derive(Debug, Eq, PartialEq)]
pub struct PolicyDefinition {
    policy_id: PolicyId,
    ceiling: AuthorityCeiling,
    operations: OperationRegistry,
    layers: Vec<RestrictionLayer>,
}

impl PolicyDefinition {
    /// Returns the exact immutable policy identifier value used by specifications.
    pub closed spec fn spec_policy_id_value(&self) -> PolicyId { self.policy_id }

    /// Returns the exact immutable policy identifier bytes used by specifications.
    pub closed spec fn spec_policy_id(&self) -> [u8; 16] { self.policy_id.spec_bytes() }

    /// Returns exact request-to-policy identity binding.
    pub open spec fn spec_matches_policy_id(&self, scope: &crate::CapabilityScope) -> bool {
        crate::model::same_identifier(self.spec_policy_id(), scope.spec_policy_id())
    }
    /// Returns the exact ceiling-grant sequence used by evaluation specifications.
    pub closed spec fn spec_grants(&self) -> Seq<crate::CeilingGrant> {
        self.ceiling.spec_grants()
    }

    /// Returns the exact protected ceiling value used by amendment specifications.
    pub closed spec fn spec_ceiling_value(&self) -> AuthorityCeiling { self.ceiling }

    /// Returns the exact bound operation registry value used by amendment specifications.
    pub closed spec fn spec_operation_registry_value(&self) -> OperationRegistry {
        self.operations
    }

    /// Returns the exact parent validity bound used by constraint specifications.
    pub closed spec fn spec_boundary_validity(&self) -> crate::ValidityWindow {
        self.ceiling.spec_boundary_validity()
    }

    /// Returns the exact protected boundary revision used by amendment specifications.
    pub closed spec fn spec_boundary_revision(&self) -> peritus_types::RevisionTuple {
        self.ceiling.spec_boundary_revision()
    }

    /// Returns whether this policy preserves the base ceiling under an exact revision rebind.
    pub closed spec fn spec_ceiling_is_revision_rebind_of(
        &self,
        base: &Self,
        revision: peritus_types::RevisionTuple,
    ) -> bool {
        self.ceiling.spec_is_revision_rebind_of(&base.ceiling, revision)
    }

    /// Returns whether this policy preserves the exact authenticated operation registry.
    pub closed spec fn spec_operations_same_as(&self, base: &Self) -> bool {
        self.operations.spec_same_as(&base.operations)
    }

    pub(crate) proof fn establish_amendment_component_views(
        &self,
        base: &Self,
        revision: peritus_types::RevisionTuple,
    )
        requires
            self.spec_ceiling_value().spec_is_revision_rebind_of(
                &base.spec_ceiling_value(),
                revision,
            ),
            self.spec_operation_registry_value().spec_same_as(
                &base.spec_operation_registry_value(),
            ),
        ensures
            self.spec_boundary_revision() == revision,
            self.spec_ceiling_is_revision_rebind_of(base, revision),
            self.spec_operations_same_as(base),
    {
        reveal(PolicyDefinition::spec_ceiling_value);
        reveal(PolicyDefinition::spec_operation_registry_value);
        reveal(PolicyDefinition::spec_boundary_revision);
        reveal(PolicyDefinition::spec_ceiling_is_revision_rebind_of);
        reveal(PolicyDefinition::spec_operations_same_as);
        self.ceiling.revision_rebind_has_exact_revision(&base.ceiling, revision);
    }

    /// Returns the exact parent logical-use bound used by constraint specifications.
    pub closed spec fn spec_boundary_use_limit(&self) -> crate::UseLimit {
        self.ceiling.spec_boundary_use_limit()
    }

    /// Returns the exact protected immutable-denial sequence.
    pub closed spec fn spec_immutable_denies(&self) -> Seq<crate::RestrictionRule> {
        self.ceiling.spec_immutable_denies()
    }

    /// Returns the exact authenticated operation descriptor sequence.
    pub closed spec fn spec_operations(&self) -> Seq<crate::OperationDescriptor> {
        self.operations.spec_descriptors()
    }

    /// Returns the exact policy-owned risk union for the requested permissions.
    pub closed spec fn spec_mandatory_risks_for_scope(
        &self,
        scope: &crate::CapabilityScope,
    ) -> Seq<crate::RiskClass> {
        self.operations.spec_risks_for_permissions(scope.spec_permissions())
    }

    /// Returns the exact first operation-registry or role-separation denial.
    pub closed spec fn spec_first_operation_denial(
        &self,
        scope: &crate::CapabilityScope,
    ) -> Option<crate::AuthorizationDenialReason> {
        self.operations.spec_first_denial(scope)
    }

    /// Returns the exact lower-layer sequence used by evaluation specifications.
    pub closed spec fn spec_layers(&self) -> Seq<RestrictionLayer> { self.layers@ }

    /// Returns exact before/after identity for appending one ordinary restriction layer.
    pub closed spec fn spec_is_one_layer_extension_of(
        &self,
        base: &Self,
        added: &RestrictionLayer,
    ) -> bool {
        self.policy_id == base.policy_id
            && self.ceiling == base.ceiling
            && self.operations == base.operations
            && self.layers@ == base.layers@.push(*added)
    }

    pub(crate) proof fn one_layer_extension_preserves_ceiling_query(
        &self,
        base: &Self,
        added: &RestrictionLayer,
        scope: &crate::CapabilityScope,
    )
        requires self.spec_is_one_layer_extension_of(base, added),
        ensures
            crate::monotonicity_model::ceiling_query_rank(self, scope)
                == crate::monotonicity_model::ceiling_query_rank(base, scope),
            self.spec_layers() == base.spec_layers().push(*added),
    {
        reveal(PolicyDefinition::spec_is_one_layer_extension_of);
        reveal(PolicyDefinition::spec_policy_id);
        reveal(PolicyDefinition::spec_boundary_contains);
        reveal(PolicyDefinition::spec_has_immutable_deny);
        reveal(PolicyDefinition::spec_has_full_coverage);
        reveal(PolicyDefinition::spec_first_operation_denial);
    }

    /// Returns exact complete-boundary containment for one request.
    pub closed spec fn spec_boundary_contains(&self, scope: &crate::CapabilityScope) -> bool {
        self.ceiling.spec_contains_scope(scope)
    }

    /// Returns whether any protected immutable denial matches the request.
    pub closed spec fn spec_has_immutable_deny(&self, scope: &crate::CapabilityScope) -> bool {
        self.ceiling.spec_has_immutable_deny(scope)
    }

    /// Returns exact whole-request ceiling-grant coverage.
    pub closed spec fn spec_has_full_coverage(&self, scope: &crate::CapabilityScope) -> bool {
        self.ceiling.spec_has_full_coverage(scope)
    }

    /// Returns whether any lower restriction-layer denial matches the request.
    pub open spec fn spec_has_restriction_deny(&self, scope: &crate::CapabilityScope) -> bool {
        crate::model::restriction_deny_matches_from(self.spec_layers(), scope, 0)
    }

    pub(crate) fn has_immutable_deny(&self, scope: &crate::CapabilityScope) -> (result: bool)
        ensures result == self.spec_has_immutable_deny(scope),
    {
        crate::evaluation_predicates::ceiling_has_immutable_deny(&self.ceiling, scope)
    }

    pub(crate) fn boundary_contains(&self, scope: &crate::CapabilityScope) -> (result: bool)
        ensures result == self.spec_boundary_contains(scope),
    {
        self.ceiling.contains_scope(scope)
    }

    pub(crate) const fn matches_policy_id(&self, scope: &crate::CapabilityScope) -> (result: bool)
        ensures result == self.spec_matches_policy_id(scope),
    {
        crate::identity::identifier_values_equal(
            *self.policy_id.as_bytes(),
            *scope.policy_id().as_bytes(),
        )
    }

    pub(crate) fn has_full_coverage(&self, scope: &crate::CapabilityScope) -> (result: bool)
        ensures result == self.spec_has_full_coverage(scope),
    {
        crate::evaluation_predicates::ceiling_has_full_coverage(&self.ceiling, scope)
    }

    pub(crate) fn has_restriction_deny(&self, scope: &crate::CapabilityScope) -> (result: bool)
        ensures result == self.spec_has_restriction_deny(scope),
    {
        crate::evaluation_predicates::has_restriction_deny(self, scope)
    }

    pub(crate) fn first_operation_denial(
        &self,
        scope: &crate::CapabilityScope,
    ) -> (result: Option<crate::AuthorizationDenialReason>)
        ensures result == self.spec_first_operation_denial(scope),
    {
        self.operations.first_denial(scope)
    }

    pub(crate) fn mandatory_risks_for_scope(
        &self,
        scope: &crate::CapabilityScope,
    ) -> (risks: crate::RiskSet)
        requires self.spec_first_operation_denial(scope).is_none(),
        ensures risks.spec_values() == self.spec_mandatory_risks_for_scope(scope),
    {
        self.operations.risks_for_scope(scope)
    }

    pub(crate) proof fn mandatory_risks_depend_only_on_permissions(
        &self,
        left: &crate::CapabilityScope,
        right: &crate::CapabilityScope,
    )
        requires left.spec_permissions() == right.spec_permissions(),
        ensures
            self.spec_mandatory_risks_for_scope(left)
                == self.spec_mandatory_risks_for_scope(right),
    {
        reveal(PolicyDefinition::spec_mandatory_risks_for_scope);
    }

    pub(crate) proof fn operation_denial_depends_only_on_role_permissions(
        &self,
        left: &crate::CapabilityScope,
        right: &crate::CapabilityScope,
    )
        requires
            left.spec_role() == right.spec_role(),
            left.spec_permissions() == right.spec_permissions(),
        ensures self.spec_first_operation_denial(left) == self.spec_first_operation_denial(right),
    {
        reveal(PolicyDefinition::spec_first_operation_denial);
    }
    /// Returns this immutable policy identity.
    #[must_use]
    pub const fn policy_id(&self) -> (policy_id: PolicyId)
        ensures
            policy_id == self.spec_policy_id_value(),
            policy_id.spec_bytes() == self.spec_policy_id(),
    { self.policy_id }

    /// Returns the protected authority ceiling.
    #[must_use]
    pub const fn ceiling(&self) -> (ceiling: &AuthorityCeiling)
        ensures
            *ceiling == self.spec_ceiling_value(),
            ceiling.spec_boundary_revision() == self.spec_boundary_revision(),
            ceiling.spec_grants() == self.spec_grants(),
            ceiling.spec_immutable_denies() == self.spec_immutable_denies(),
            ceiling.spec_boundary_validity() == self.spec_boundary_validity(),
            ceiling.spec_boundary_use_limit() == self.spec_boundary_use_limit(),
    { &self.ceiling }

    /// Returns the immutable authenticated operation registry bound to this policy.
    #[must_use]
    pub const fn operations(&self) -> (operations: &OperationRegistry)
        ensures
            *operations == self.spec_operation_registry_value(),
            operations.spec_descriptors() == self.spec_operations(),
    { &self.operations }

    /// Borrows lower restriction layers in authority order.
    #[must_use]
    pub const fn layers(&self) -> (layers: &[RestrictionLayer])
        ensures layers@ == self.spec_layers(),
    { self.layers.as_slice() }
}

} // verus!
