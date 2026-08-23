//! Exact ordered policy-definition admission.

use super::PolicyDefinition;
use crate::{
    AuthorityBoundary, AuthorityCeiling, OperationRegistry, PolicyError, RestrictionLayer,
    RestrictionRule,
};
#[cfg(verus_only)]
use crate::{PolicyErrorKind, ScopeDimension};
use peritus_types::PolicyId;
use vstd::prelude::*;

verus! {

#[cfg(verus_only)]
pub(crate) type DefinitionValidationError = (PolicyErrorKind, Option<ScopeDimension>);

pub(crate) open spec fn first_tier_order_error(
    layers: Seq<RestrictionLayer>,
    index: nat,
) -> bool
    decreases layers.len() - index,
{
    if index >= layers.len() {
        false
    } else if layers[index as int - 1].spec_tier().spec_rank()
        >= layers[index as int].spec_tier().spec_rank()
    {
        true
    } else {
        first_tier_order_error(layers, index + 1)
    }
}

pub(crate) open spec fn first_rule_boundary_mismatch(
    rules: Seq<RestrictionRule>,
    boundary: &AuthorityBoundary,
    index: nat,
) -> Option<ScopeDimension>
    decreases rules.len() - index,
{
    if index >= rules.len() {
        None
    } else if rules[index as int]
        .spec_selector()
        .spec_first_boundary_mismatch(boundary) is Some
    {
        rules[index as int]
            .spec_selector()
            .spec_first_boundary_mismatch(boundary)
    } else {
        first_rule_boundary_mismatch(rules, boundary, index + 1)
    }
}

pub(crate) open spec fn first_layer_boundary_mismatch(
    layers: Seq<RestrictionLayer>,
    boundary: &AuthorityBoundary,
    index: nat,
) -> Option<ScopeDimension>
    decreases layers.len() - index,
{
    if index >= layers.len() {
        None
    } else if first_rule_boundary_mismatch(
        layers[index as int].spec_rules(),
        boundary,
        0,
    ) is Some
    {
        first_rule_boundary_mismatch(layers[index as int].spec_rules(), boundary, 0)
    } else {
        first_layer_boundary_mismatch(layers, boundary, index + 1)
    }
}

/// Returns the exact first policy-definition validation failure.
pub closed spec fn policy_definition_validation_error(
    policy_id: PolicyId,
    ceiling: &AuthorityCeiling,
    layers: Seq<RestrictionLayer>,
) -> Option<DefinitionValidationError> {
    policy_definition_validation_error_internal(policy_id, ceiling, layers)
}

pub(crate) open spec fn policy_definition_validation_error_internal(
    policy_id: PolicyId,
    ceiling: &AuthorityCeiling,
    layers: Seq<RestrictionLayer>,
) -> Option<DefinitionValidationError> {
    if !crate::model::same_identifier(
        ceiling.spec_boundary_revision().spec_policy_id().spec_bytes(),
        policy_id.spec_bytes(),
    ) {
        Some((PolicyErrorKind::PolicyRevisionMismatch, None))
    } else if first_tier_order_error(layers, 1) {
        Some((PolicyErrorKind::InvalidPolicyTier, None))
    } else if first_layer_boundary_mismatch(layers, &ceiling.spec_boundary_value(), 0) is Some {
        Some((
            PolicyErrorKind::SelectorOutsideBoundary,
            first_layer_boundary_mismatch(layers, &ceiling.spec_boundary_value(), 0),
        ))
    } else {
        None
    }
}

fn validate_rule_boundaries(
    rules: &[RestrictionRule],
    boundary: &AuthorityBoundary,
) -> (result: Result<(), PolicyError>)
    ensures
        match result {
            Ok(()) => first_rule_boundary_mismatch(rules@, boundary, 0).is_none(),
            Err(error) => {
                first_rule_boundary_mismatch(rules@, boundary, 0) == error.spec_dimension()
                    && error.spec_kind() == PolicyErrorKind::SelectorOutsideBoundary
                    && error.spec_collection().is_none()
                    && error.spec_dimension().is_some()
            }
        },
{
    let mut index = 0;
    while index < rules.len()
        invariant
            0 <= index <= rules.len(),
            first_rule_boundary_mismatch(rules@, boundary, 0)
                == first_rule_boundary_mismatch(rules@, boundary, index as nat),
        decreases rules.len() - index,
    {
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
) -> (result: Result<(), PolicyError>)
    ensures
        match result {
            Ok(()) => first_layer_boundary_mismatch(layers@, boundary, 0).is_none(),
            Err(error) => {
                first_layer_boundary_mismatch(layers@, boundary, 0) == error.spec_dimension()
                    && error.spec_kind() == PolicyErrorKind::SelectorOutsideBoundary
                    && error.spec_collection().is_none()
                    && error.spec_dimension().is_some()
            }
        },
{
    let mut index = 0;
    while index < layers.len()
        invariant
            0 <= index <= layers.len(),
            first_layer_boundary_mismatch(layers@, boundary, 0)
                == first_layer_boundary_mismatch(layers@, boundary, index as nat),
        decreases layers.len() - index,
    {
        match validate_rule_boundaries(layers[index].rules(), boundary) {
            Err(error) => return Err(error),
            Ok(()) => index += 1,
        }
    }
    Ok(())
}

impl PolicyDefinition {
    /// Creates a policy with restriction layers in strict authority-tier order.
    ///
    /// # Errors
    ///
    /// Returns the exact first identity, tier-order, or selector-containment failure.
    pub fn new(
        policy_id: PolicyId,
        ceiling: AuthorityCeiling,
        operations: OperationRegistry,
        layers: Vec<RestrictionLayer>,
    ) -> (result: Result<Self, PolicyError>)
        ensures
            match result {
                Ok(policy) => {
                    policy_definition_validation_error(policy_id, &ceiling, layers@).is_none()
                        && policy.spec_policy_id() == policy_id.spec_bytes()
                        && policy.spec_ceiling_value() == ceiling
                        && policy.spec_operation_registry_value() == operations
                        && policy.spec_layers() == layers@
                }
                Err(error) => {
                    policy_definition_validation_error(policy_id, &ceiling, layers@)
                        == Some((error.spec_kind(), error.spec_dimension()))
                        && error.spec_collection().is_none()
                }
            },
    {
        reveal(policy_definition_validation_error);
        if !crate::identity::identifier_values_equal(
            *ceiling.boundary().revision().policy_id().as_bytes(),
            *policy_id.as_bytes(),
        ) {
            return Err(PolicyError::policy_revision_mismatch());
        }
        assert(crate::model::same_identifier(
            ceiling.spec_boundary_revision().spec_policy_id().spec_bytes(),
            policy_id.spec_bytes(),
        ));
        let mut index = 1;
        while index < layers.len()
            invariant
                (layers.len() == 0 && index == 1) || 1 <= index <= layers.len(),
                crate::model::same_identifier(
                    ceiling.spec_boundary_revision().spec_policy_id().spec_bytes(),
                    policy_id.spec_bytes(),
                ),
                first_tier_order_error(layers@, 1)
                    == first_tier_order_error(layers@, index as nat),
            decreases layers.len() - index,
        {
            if layers[index - 1].tier().rank() >= layers[index].tier().rank() {
                assert(first_tier_order_error(layers@, index as nat));
                assert(first_tier_order_error(layers@, 1));
                assert(policy_definition_validation_error_internal(
                    policy_id,
                    &ceiling,
                    layers@,
                ) == Some((PolicyErrorKind::InvalidPolicyTier, None)));
                return Err(PolicyError::invalid_policy_tier());
            }
            index += 1;
        }
        if let Err(error) = validate_layer_boundaries(layers.as_slice(), ceiling.boundary()) {
            assert(policy_definition_validation_error_internal(
                policy_id,
                &ceiling,
                layers@,
            ) == Some((error.spec_kind(), error.spec_dimension())));
            return Err(error);
        }
        let policy = Self { policy_id, ceiling, operations, layers };
        reveal(PolicyDefinition::spec_policy_id);
        reveal(PolicyDefinition::spec_ceiling_value);
        reveal(PolicyDefinition::spec_operation_registry_value);
        reveal(PolicyDefinition::spec_layers);
        Ok(policy)
    }
}

} // verus!
