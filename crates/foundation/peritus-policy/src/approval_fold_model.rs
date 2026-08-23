//! Exact fold of matching approval restrictions into one effective requirement.

#![cfg(verus_only)]

use crate::{
    ActorRole, ApprovalRequirement, AuthorityTier, CapabilityScope, IndependenceRequirement,
    PolicyDecision, PolicyDefinition, RestrictionLayer, ValidityWindow,
};
use vstd::prelude::*;

verus! {

/// Exact accumulated values of every matching approval restriction.
pub struct ApprovalValues {
    /// Whether at least one approval restriction matched.
    pub required: bool,
    /// Whether exact role or validity conjunction became empty.
    pub conflict: bool,
    /// Greatest required authority tier.
    pub minimum_tier: AuthorityTier,
    /// Canonical intersection of allowed approver roles.
    pub approver_roles: Seq<ActorRole>,
    /// Canonical union of independence requirements.
    pub independence: Seq<IndependenceRequirement>,
    /// Inclusive approval-validity epoch.
    pub not_before_epoch: int,
    /// Inclusive approval-validity tick.
    pub not_before_tick: int,
    /// Exclusive approval-validity epoch.
    pub expires_epoch: int,
    /// Exclusive approval-validity tick.
    pub expires_tick: int,
}

/// Returns the neutral approval accumulator.
pub open spec fn empty_approval_values() -> ApprovalValues {
    ApprovalValues {
        required: false,
        conflict: false,
        minimum_tier: AuthorityTier::Project,
        approver_roles: Seq::empty(),
        independence: Seq::empty(),
        not_before_epoch: 0,
        not_before_tick: 0,
        expires_epoch: 0,
        expires_tick: 0,
    }
}

/// Projects one exact requirement into accumulator values.
pub open spec fn approval_values_from_requirement(
    requirement: &ApprovalRequirement,
) -> ApprovalValues {
    ApprovalValues {
        required: true,
        conflict: false,
        minimum_tier: requirement.spec_minimum_tier(),
        approver_roles: requirement.spec_approver_roles(),
        independence: requirement.spec_independence(),
        not_before_epoch: requirement.spec_validity().spec_not_before().spec_epoch(),
        not_before_tick: requirement.spec_validity().spec_not_before().spec_tick_millis(),
        expires_epoch: requirement.spec_validity().spec_expires_at().spec_epoch(),
        expires_tick: requirement.spec_validity().spec_expires_at().spec_tick_millis(),
    }
}

/// Projects the evaluator's optional concrete accumulator into exact model values.
pub open spec fn approval_accumulator_values(
    requirement: &Option<ApprovalRequirement>,
) -> ApprovalValues {
    match requirement {
        Some(value) => approval_values_from_requirement(value),
        None => empty_approval_values(),
    }
}

/// Returns whether one additional requirement conflicts with an accumulated conjunction.
pub open spec fn approval_values_conflict_with(
    accumulated: ApprovalValues,
    requirement: &ApprovalRequirement,
) -> bool {
    let roles = crate::approval_model::role_intersection_from(
        accumulated.approver_roles,
        requirement.spec_approver_roles(),
        0,
        0,
    );
    let next_not_before_epoch = if accumulated.not_before_tick
        >= requirement.spec_validity().spec_not_before().spec_tick_millis()
    {
        accumulated.not_before_epoch
    } else {
        requirement.spec_validity().spec_not_before().spec_epoch()
    };
    let next_expires_epoch = if accumulated.expires_tick
        <= requirement.spec_validity().spec_expires_at().spec_tick_millis()
    {
        accumulated.expires_epoch
    } else {
        requirement.spec_validity().spec_expires_at().spec_epoch()
    };
    roles.len() == 0
        || accumulated.not_before_epoch
            != requirement.spec_validity().spec_not_before().spec_epoch()
        || next_not_before_epoch != next_expires_epoch
        || crate::model::maximum_int(
            accumulated.not_before_tick,
            requirement.spec_validity().spec_not_before().spec_tick_millis(),
        ) >= crate::model::minimum_int(
            accumulated.expires_tick,
            requirement.spec_validity().spec_expires_at().spec_tick_millis(),
        )
}

/// Adds one matching requirement to the exact accumulator.
pub open spec fn combine_approval_values(
    accumulated: ApprovalValues,
    requirement: &ApprovalRequirement,
) -> ApprovalValues {
    if accumulated.conflict {
        accumulated
    } else if !accumulated.required {
        approval_values_from_requirement(requirement)
    } else if approval_values_conflict_with(accumulated, requirement) {
        ApprovalValues { conflict: true, ..accumulated }
    } else {
        ApprovalValues {
            required: true,
            conflict: false,
            minimum_tier: crate::approval_model::maximum_authority_tier(
                accumulated.minimum_tier,
                requirement.spec_minimum_tier(),
            ),
            approver_roles: crate::approval_model::role_intersection_from(
                accumulated.approver_roles,
                requirement.spec_approver_roles(),
                0,
                0,
            ),
            independence: crate::approval_model::independence_union_from(
                accumulated.independence,
                requirement.spec_independence(),
                0,
                0,
            ),
            not_before_epoch: if accumulated.not_before_tick
                >= requirement.spec_validity().spec_not_before().spec_tick_millis()
            {
                accumulated.not_before_epoch
            } else {
                requirement.spec_validity().spec_not_before().spec_epoch()
            },
            not_before_tick: crate::model::maximum_int(
                accumulated.not_before_tick,
                requirement.spec_validity().spec_not_before().spec_tick_millis(),
            ),
            expires_epoch: if accumulated.expires_tick
                <= requirement.spec_validity().spec_expires_at().spec_tick_millis()
            {
                accumulated.expires_epoch
            } else {
                requirement.spec_validity().spec_expires_at().spec_epoch()
            },
            expires_tick: crate::model::minimum_int(
                accumulated.expires_tick,
                requirement.spec_validity().spec_expires_at().spec_tick_millis(),
            ),
        }
    }
}

/// Constructively folds matching approval rules across canonical layers and rules.
pub open spec fn approval_values_from_layers(
    layers: Seq<RestrictionLayer>,
    scope: &CapabilityScope,
    layer_index: nat,
    rule_index: nat,
    accumulated: ApprovalValues,
) -> ApprovalValues
    decreases
        layers.len() - layer_index,
        if layer_index < layers.len() {
            layers[layer_index as int].spec_rules().len() - rule_index
        } else {
            0
        },
{
    if layer_index >= layers.len() {
        accumulated
    } else if rule_index >= layers[layer_index as int].spec_rules().len() {
        approval_values_from_layers(layers, scope, layer_index + 1, 0, accumulated)
    } else {
        let rule = layers[layer_index as int].spec_rules()[rule_index as int];
        let next = match rule.spec_approval_requirement() {
            Some(requirement) if rule.spec_matches_scope(scope) => {
                combine_approval_values(accumulated, &requirement)
            }
            _ => accumulated,
        };
        approval_values_from_layers(layers, scope, layer_index, rule_index + 1, next)
    }
}

/// Returns the exact conjunction of every matching approval restriction.
pub open spec fn policy_approval_values(
    policy: &PolicyDefinition,
    scope: &CapabilityScope,
) -> ApprovalValues {
    approval_values_from_layers(
        policy.spec_layers(),
        scope,
        0,
        0,
        empty_approval_values(),
    )
}

/// A terminal approval conflict cannot be removed by any lower restriction.
pub proof fn approval_fold_preserves_conflict(
    layers: Seq<RestrictionLayer>,
    scope: &CapabilityScope,
    layer_index: nat,
    rule_index: nat,
    accumulated: ApprovalValues,
)
    requires accumulated.conflict,
    ensures approval_values_from_layers(
        layers,
        scope,
        layer_index,
        rule_index,
        accumulated,
    ).conflict,
    decreases
        layers.len() - layer_index,
        if layer_index < layers.len() {
            layers[layer_index as int].spec_rules().len() - rule_index
        } else {
            0
        },
{
    if layer_index < layers.len() {
        if rule_index >= layers[layer_index as int].spec_rules().len() {
            approval_fold_preserves_conflict(layers, scope, layer_index + 1, 0, accumulated);
        } else {
            approval_fold_preserves_conflict(
                layers,
                scope,
                layer_index,
                rule_index + 1,
                accumulated,
            );
        }
    }
}

/// Returns whether constraining approval validity to the effective scope becomes empty.
pub open spec fn effective_approval_conflict(
    values: ApprovalValues,
    effective: ValidityWindow,
) -> bool {
    let next_not_before_epoch = if values.not_before_tick
        >= effective.spec_not_before().spec_tick_millis()
    {
        values.not_before_epoch
    } else {
        effective.spec_not_before().spec_epoch()
    };
    let next_expires_epoch = if values.expires_tick
        <= effective.spec_expires_at().spec_tick_millis()
    {
        values.expires_epoch
    } else {
        effective.spec_expires_at().spec_epoch()
    };
    values.not_before_epoch != effective.spec_not_before().spec_epoch()
        || next_not_before_epoch != next_expires_epoch
        || crate::model::maximum_int(
            values.not_before_tick,
            effective.spec_not_before().spec_tick_millis(),
        ) >= crate::model::minimum_int(
            values.expires_tick,
            effective.spec_expires_at().spec_tick_millis(),
        )
}

/// An exact failed validity refinement is the aggregate form of the same conflict.
pub proof fn constrained_none_implies_effective_conflict(
    original: &ApprovalRequirement,
    effective: ValidityWindow,
    values: ApprovalValues,
)
    requires
        values == approval_values_from_requirement(original),
        crate::approval_model::constrained_approval_result(original, effective, &None),
    ensures effective_approval_conflict(values, effective),
{
}

/// Relates the exact approval fold to the evaluator's exhaustive final decision.
pub open spec fn decision_has_exact_approval(
    policy: &PolicyDefinition,
    scope: &CapabilityScope,
    decision: &PolicyDecision,
) -> bool {
    let values = policy_approval_values(policy, scope);
    let effective = decision.spec_scope_validity();
    let constrained_conflict = effective_approval_conflict(values, effective);
    if values.conflict || (values.required && constrained_conflict) {
        matches!(decision, PolicyDecision::Denied(_))
    } else if !values.required {
        matches!(decision, PolicyDecision::Authorized(_))
    } else {
        match decision {
            PolicyDecision::ApprovalRequired(challenge) => {
                challenge.spec_risks()
                    == policy.spec_mandatory_risks_for_scope(scope)
                    && challenge.spec_requirement_minimum_tier() == values.minimum_tier
                    && challenge.spec_requirement_approver_roles() == values.approver_roles
                    && challenge.spec_requirement_independence() == values.independence
                    && challenge.spec_requirement_validity().spec_not_before().spec_epoch()
                        == if values.not_before_tick
                            >= effective.spec_not_before().spec_tick_millis()
                        {
                            values.not_before_epoch
                        } else {
                            effective.spec_not_before().spec_epoch()
                        }
                    && challenge.spec_requirement_validity().spec_not_before().spec_tick_millis()
                        == crate::model::maximum_int(
                            values.not_before_tick,
                            effective.spec_not_before().spec_tick_millis(),
                        )
                    && challenge.spec_requirement_validity().spec_expires_at().spec_epoch()
                        == if values.expires_tick
                            <= effective.spec_expires_at().spec_tick_millis()
                        {
                            values.expires_epoch
                        } else {
                            effective.spec_expires_at().spec_epoch()
                        }
                    && challenge.spec_requirement_validity().spec_expires_at().spec_tick_millis()
                        == crate::model::minimum_int(
                            values.expires_tick,
                            effective.spec_expires_at().spec_tick_millis(),
                        )
            }
            _ => false,
        }
    }
}

} // verus!
