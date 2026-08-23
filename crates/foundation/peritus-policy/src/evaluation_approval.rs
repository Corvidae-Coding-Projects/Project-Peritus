//! Executable exact conjunction of all matching approval restrictions.

use crate::{
    ApprovalRequirement, AuthorizationDenialReason, CapabilityScope, PolicyDefinition,
    PolicyError,
};
use vstd::prelude::*;

verus! {

/// Total result of conjoining every matching approval restriction.
pub enum RestrictionResult {
    /// Exact optional effective approval requirement.
    Accepted(Option<ApprovalRequirement>),
    /// Stable semantic approval conflict denial.
    Denied(AuthorizationDenialReason),
}

enum AccumulationResult {
    Accumulated(Option<ApprovalRequirement>),
    Conflict,
}

proof fn approval_conflict_is_terminal(
    layers: Seq<crate::RestrictionLayer>,
    requested: &CapabilityScope,
    layer_index: nat,
    rule_index: nat,
    previous: crate::approval_fold_model::ApprovalValues,
    requirement: &ApprovalRequirement,
)
    requires
        layer_index < layers.len(),
        rule_index < layers[layer_index as int].spec_rules().len(),
        layers[layer_index as int].spec_rules()[rule_index as int]
            .spec_matches_scope(requested),
        layers[layer_index as int].spec_rules()[rule_index as int]
            .spec_approval_requirement() == Some(*requirement),
        crate::approval_fold_model::combine_approval_values(previous, requirement).conflict,
    ensures
        crate::approval_fold_model::approval_values_from_layers(
            layers,
            requested,
            layer_index,
            rule_index,
            previous,
        ).conflict,
{
    let conflicted = crate::approval_fold_model::combine_approval_values(
        previous,
        requirement,
    );
    assert(crate::approval_fold_model::approval_values_from_layers(
        layers,
        requested,
        layer_index,
        rule_index,
        previous,
    ) == crate::approval_fold_model::approval_values_from_layers(
        layers,
        requested,
        layer_index,
        rule_index + 1,
        conflicted,
    ));
    crate::approval_fold_model::approval_fold_preserves_conflict(
        layers,
        requested,
        layer_index,
        rule_index + 1,
        conflicted,
    );
}

fn accumulate_requirement(
    approval: Option<ApprovalRequirement>,
    requirement: &ApprovalRequirement,
) -> (result: Result<AccumulationResult, PolicyError>)
    ensures
        result.is_ok(),
        match result {
            Ok(AccumulationResult::Accumulated(next)) => {
                crate::approval_fold_model::approval_accumulator_values(&next)
                    == crate::approval_fold_model::combine_approval_values(
                        crate::approval_fold_model::approval_accumulator_values(&approval),
                        requirement,
                    )
            }
            Ok(AccumulationResult::Conflict) => {
                crate::approval_fold_model::combine_approval_values(
                    crate::approval_fold_model::approval_accumulator_values(&approval),
                    requirement,
                ).conflict
            }
            Err(_) => true,
        },
{
    match approval {
        None => Ok(AccumulationResult::Accumulated(Some(requirement.duplicate()))),
        Some(accumulated) => {
            let conjunction = accumulated.conjunction(requirement)?;
            let Some(value) = conjunction else {
                assert(crate::approval_fold_model::combine_approval_values(
                    crate::approval_fold_model::approval_values_from_requirement(&accumulated),
                    requirement,
                ).conflict);
                return Ok(AccumulationResult::Conflict);
            };
            assert(crate::approval_fold_model::approval_accumulator_values(&Some(value))
                == crate::approval_fold_model::combine_approval_values(
                    crate::approval_fold_model::approval_values_from_requirement(&accumulated),
                    requirement,
                ));
            Ok(AccumulationResult::Accumulated(Some(value)))
        }
    }
}

/// Conjoins every matching approval restriction in canonical policy order.
pub fn approval_conjunction(
    policy: &PolicyDefinition,
    requested: &CapabilityScope,
) -> (result: Result<RestrictionResult, PolicyError>)
    ensures
        result.is_ok(),
        match result {
            Ok(RestrictionResult::Accepted(approval)) => {
                crate::approval_fold_model::approval_accumulator_values(&approval)
                    == crate::approval_fold_model::policy_approval_values(policy, requested)
                    && !crate::approval_fold_model::policy_approval_values(
                        policy,
                        requested,
                    ).conflict
            }
            Ok(RestrictionResult::Denied(reason)) => {
                reason == AuthorizationDenialReason::ApprovalConstraintConflict
                    && crate::approval_fold_model::policy_approval_values(
                        policy,
                        requested,
                    ).conflict
            }
            Err(_) => true,
        },
{
    let mut approval: Option<ApprovalRequirement> = None;
    let layers = policy.layers();
    let mut layer_index = 0;
    while layer_index < layers.len()
        invariant
            0 <= layer_index <= layers.len(),
            layers@ == policy.spec_layers(),
            !crate::approval_fold_model::approval_accumulator_values(&approval).conflict,
            crate::approval_fold_model::approval_values_from_layers(
                layers@,
                requested,
                layer_index as nat,
                0,
                crate::approval_fold_model::approval_accumulator_values(&approval),
            ) == crate::approval_fold_model::policy_approval_values(policy, requested),
        decreases layers.len() - layer_index,
    {
        let rules = layers[layer_index].rules();
        let mut rule_index = 0;
        while rule_index < rules.len()
            invariant
                0 <= layer_index < layers.len(),
                0 <= rule_index <= rules.len(),
                rules@ == layers@[layer_index as int].spec_rules(),
                !crate::approval_fold_model::approval_accumulator_values(&approval).conflict,
                crate::approval_fold_model::approval_values_from_layers(
                    layers@,
                    requested,
                    layer_index as nat,
                    rule_index as nat,
                    crate::approval_fold_model::approval_accumulator_values(&approval),
                ) == crate::approval_fold_model::policy_approval_values(policy, requested),
            decreases rules.len() - rule_index,
        {
            let rule = &rules[rule_index];
            let ghost previous = crate::approval_fold_model::approval_accumulator_values(&approval);
            let requirement = rule.matching_approval_requirement(requested);
            if let Some(requirement) = requirement {
                match accumulate_requirement(approval, requirement)? {
                    AccumulationResult::Accumulated(next) => approval = next,
                    AccumulationResult::Conflict => {
                        proof {
                            approval_conflict_is_terminal(
                                layers@,
                                requested,
                                layer_index as nat,
                                rule_index as nat,
                                previous,
                                requirement,
                            );
                        }
                        assert(crate::approval_fold_model::approval_values_from_layers(
                            layers@,
                            requested,
                            layer_index as nat,
                            rule_index as nat,
                            previous,
                        ).conflict);
                        return Ok(RestrictionResult::Denied(
                            AuthorizationDenialReason::ApprovalConstraintConflict,
                        ));
                    }
                }
            }
            assert(crate::approval_fold_model::approval_values_from_layers(
                layers@,
                requested,
                layer_index as nat,
                rule_index as nat,
                previous,
            ) == crate::approval_fold_model::approval_values_from_layers(
                layers@,
                requested,
                layer_index as nat,
                (rule_index + 1) as nat,
                crate::approval_fold_model::approval_accumulator_values(&approval),
            ));
            rule_index += 1;
        }
        assert(crate::approval_fold_model::approval_values_from_layers(
            layers@,
            requested,
            layer_index as nat,
            rule_index as nat,
            crate::approval_fold_model::approval_accumulator_values(&approval),
        ) == crate::approval_fold_model::approval_values_from_layers(
            layers@,
            requested,
            (layer_index + 1) as nat,
            0,
            crate::approval_fold_model::approval_accumulator_values(&approval),
        ));
        layer_index += 1;
    }
    Ok(RestrictionResult::Accepted(approval))
}

} // verus!
