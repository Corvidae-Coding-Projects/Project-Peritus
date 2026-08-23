//! Pointwise allowed-query-set model for ordinary restriction composition.

#![cfg(verus_only)]

use crate::{
    CapabilityScope, PolicyDecision, PolicyDecisionKind, PolicyDefinition, RestrictionLayer,
    RestrictionRule,
};
use vstd::prelude::*;

verus! {

/// Numeric authority order used for every complete-query outcome.
pub open spec fn decision_rank(kind: PolicyDecisionKind) -> int {
    match kind {
        PolicyDecisionKind::Denied => 0,
        PolicyDecisionKind::ApprovalRequired => 1,
        PolicyDecisionKind::Authorized => 2,
    }
}

/// Rank of one query before ordinary restriction layers are applied.
pub open spec fn ceiling_query_rank(
    policy: &PolicyDefinition,
    scope: &CapabilityScope,
) -> int {
    if policy.spec_matches_policy_id(scope)
        && policy.spec_boundary_contains(scope)
        && policy.spec_first_operation_denial(scope).is_none()
        && !policy.spec_has_immutable_deny(scope)
        && policy.spec_has_full_coverage(scope)
    {
        2
    } else {
        0
    }
}

/// Aggregate rank contributed by the policy's exact ordinary restriction layers.
pub open spec fn ordinary_restriction_rank(
    policy: &PolicyDefinition,
    scope: &CapabilityScope,
) -> int {
    if policy.spec_has_restriction_deny(scope) {
        0
    } else if crate::approval_fold_model::policy_approval_values(policy, scope).required {
        1
    } else {
        2
    }
}

/// Pointwise rank after composing the ceiling and every ordinary restriction.
pub open spec fn composed_query_rank(
    policy: &PolicyDefinition,
    scope: &CapabilityScope,
) -> int {
    crate::model::decision_meet(
        ceiling_query_rank(policy, scope),
        ordinary_restriction_rank(policy, scope),
    )
}

/// Membership in the allowed query set at a requested authority threshold.
pub open spec fn query_is_allowed(rank: int, minimum_rank: int) -> bool {
    rank >= minimum_rank
}

/// Rank contributed by one exact ordinary rule for one complete query.
pub open spec fn rule_query_rank(rule: &RestrictionRule, scope: &CapabilityScope) -> int {
    if !rule.spec_matches_scope(scope) {
        2
    } else if rule.spec_is_deny() {
        0
    } else {
        1
    }
}

/// Most-restrictive rank of a suffix of one exact ordinary layer.
pub open spec fn layer_query_rank_from(
    rules: Seq<RestrictionRule>,
    scope: &CapabilityScope,
    index: nat,
) -> int
    decreases rules.len() - index,
{
    if index >= rules.len() {
        2
    } else {
        crate::model::decision_meet(
            rule_query_rank(&rules[index as int], scope),
            layer_query_rank_from(rules, scope, index + 1),
        )
    }
}

/// Whether a suffix contains a matching approval restriction.
pub open spec fn matching_approval_rule_from(
    rules: Seq<RestrictionRule>,
    scope: &CapabilityScope,
    index: nat,
) -> bool
    decreases rules.len() - index,
{
    if index >= rules.len() {
        false
    } else {
        (rules[index as int].spec_matches_scope(scope)
            && rules[index as int].spec_approval_requirement().is_some())
            || matching_approval_rule_from(rules, scope, index + 1)
    }
}

/// Whether a layer suffix contains a matching approval restriction.
pub open spec fn matching_approval_layer_from(
    layers: Seq<RestrictionLayer>,
    scope: &CapabilityScope,
    index: nat,
) -> bool
    decreases layers.len() - index,
{
    if index >= layers.len() {
        false
    } else {
        matching_approval_rule_from(layers[index as int].spec_rules(), scope, 0)
            || matching_approval_layer_from(layers, scope, index + 1)
    }
}

/// Whether the exact two-dimensional layer/rule suffix contains a matching approval.
pub open spec fn matching_approval_from(
    layers: Seq<RestrictionLayer>,
    scope: &CapabilityScope,
    layer_index: nat,
    rule_index: nat,
) -> bool
    decreases
        layers.len() - layer_index,
        if layer_index < layers.len() {
            layers[layer_index as int].spec_rules().len() - rule_index
        } else {
            0
        },
{
    if layer_index >= layers.len() {
        false
    } else if rule_index >= layers[layer_index as int].spec_rules().len() {
        matching_approval_from(layers, scope, layer_index + 1, 0)
    } else {
        let rule = layers[layer_index as int].spec_rules()[rule_index as int];
        (rule.spec_matches_scope(scope) && rule.spec_approval_requirement().is_some())
            || matching_approval_from(layers, scope, layer_index, rule_index + 1)
    }
}

/// Rank contributed by one complete ordinary restriction layer.
pub open spec fn layer_query_rank(layer: &RestrictionLayer, scope: &CapabilityScope) -> int {
    layer_query_rank_from(layer.spec_rules(), scope, 0)
}

/// Most-restrictive rank of a suffix of exact ordinary restriction layers.
pub open spec fn layers_query_rank_from(
    layers: Seq<RestrictionLayer>,
    scope: &CapabilityScope,
    index: nat,
) -> int
    decreases layers.len() - index,
{
    if index >= layers.len() {
        2
    } else {
        crate::model::decision_meet(
            layer_query_rank(&layers[index as int], scope),
            layers_query_rank_from(layers, scope, index + 1),
        )
    }
}

/// Structural pointwise query rank of one complete policy definition.
pub open spec fn structural_policy_query_rank(
    policy: &PolicyDefinition,
    scope: &CapabilityScope,
) -> int {
    crate::model::decision_meet(
        ceiling_query_rank(policy, scope),
        layers_query_rank_from(policy.spec_layers(), scope, 0),
    )
}

/// The exact evaluator result is a member only of sets permitted by ordinary composition.
pub open spec fn decision_respects_allowed_query_set(
    policy: &PolicyDefinition,
    scope: &CapabilityScope,
    decision: &PolicyDecision,
) -> bool {
    decision_rank(decision.spec_kind()) <= composed_query_rank(policy, scope)
        && composed_query_rank(policy, scope) == structural_policy_query_rank(policy, scope)
        && forall |minimum_rank: int| 0 <= minimum_rank <= 2
            && #[trigger] query_is_allowed(
                decision_rank(decision.spec_kind()),
                minimum_rank,
            ) ==> query_is_allowed(
                ceiling_query_rank(policy, scope),
                minimum_rank,
            )
}

} // verus!
