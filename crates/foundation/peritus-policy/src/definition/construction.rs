//! Exact ordered policy-definition admission.

use super::PolicyDefinition;
use crate::{
    AuthorityBoundary, AuthorityCeiling, OperationRegistry, PolicyError, RestrictionLayer,
    RestrictionRule,
};

use peritus_types::PolicyId;

fn validate_rule_boundaries(
    rules: &[RestrictionRule],
    boundary: &AuthorityBoundary,
) -> Result<(), PolicyError> {
    let mut index = 0;
    while index < rules.len() {
        if let Some(dimension) = rules[index].selector().first_boundary_mismatch(boundary) {
            return Err(PolicyError::selector_outside_boundary(dimension));
        }
        index += 1;
    }
    Ok(())
}
fn validate_layer_boundaries(
    layers: &[RestrictionLayer],
    boundary: &AuthorityBoundary,
) -> Result<(), PolicyError> {
    let mut index = 0;
    while index < layers.len() {
        match validate_rule_boundaries(layers[index].rules(), boundary) {
            Err(error) => return Err(error),
            Ok(()) => index += 1,
        }
    }
    Ok(())
}
impl PolicyDefinition {
    #[doc = " Creates a policy with restriction layers in strict authority-tier order."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns the exact first identity, tier-order, or selector-containment failure."]
    pub fn new(
        policy_id: PolicyId,
        ceiling: AuthorityCeiling,
        operations: OperationRegistry,
        layers: Vec<RestrictionLayer>,
    ) -> Result<Self, PolicyError> {
        if !crate::identity::identifier_values_equal(
            *ceiling.boundary().revision().policy_id().as_bytes(),
            *policy_id.as_bytes(),
        ) {
            return Err(PolicyError::policy_revision_mismatch());
        }
        let mut index = 1;
        while index < layers.len() {
            if layers[index - 1].tier().rank() >= layers[index].tier().rank() {
                return Err(PolicyError::invalid_policy_tier());
            }
            index += 1;
        }
        if let Err(error) = validate_layer_boundaries(layers.as_slice(), ceiling.boundary()) {
            return Err(error);
        }
        let policy = Self { policy_id, ceiling, operations, layers };
        Ok(policy)
    }
}
