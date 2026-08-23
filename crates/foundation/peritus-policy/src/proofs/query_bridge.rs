//! Bridge between structural layer ranks and the executable evaluator aggregate.

#![cfg(verus_only)]

use crate::{
    approval_fold_model as approvals, model, monotonicity_model as queries, CapabilityScope,
    PolicyDefinition, RestrictionLayer, RestrictionRule,
};
use vstd::prelude::*;

verus! {

proof fn rule_suffix_rank_is_exact(
    rules: Seq<RestrictionRule>,
    scope: &CapabilityScope,
    index: nat,
)
    requires index <= rules.len(),
    ensures
        queries::layer_query_rank_from(rules, scope, index) == if model::deny_rule_matches_from(
            rules,
            scope,
            index,
        ) {
            0int
        } else if queries::matching_approval_rule_from(rules, scope, index) {
            1int
        } else {
            2int
        },
    decreases rules.len() - index,
{
    if index < rules.len() {
        rules[index as int].kind_is_total();
        rule_suffix_rank_is_exact(rules, scope, index + 1);
    }
}

proof fn two_dimensional_match_at_layer(
    layers: Seq<RestrictionLayer>,
    scope: &CapabilityScope,
    layer_index: nat,
    rule_index: nat,
)
    requires
        layer_index < layers.len(),
        rule_index <= layers[layer_index as int].spec_rules().len(),
    ensures
        queries::matching_approval_from(
            layers,
            scope,
            layer_index,
            rule_index,
        ) == (
            queries::matching_approval_rule_from(
                layers[layer_index as int].spec_rules(),
                scope,
                rule_index,
            ) || queries::matching_approval_from(
                layers,
                scope,
                layer_index + 1,
                0,
            )
        ),
    decreases layers[layer_index as int].spec_rules().len() - rule_index,
{
    if rule_index < layers[layer_index as int].spec_rules().len() {
        two_dimensional_match_at_layer(layers, scope, layer_index, rule_index + 1);
    }
}

proof fn layer_suffix_rank_is_exact(
    layers: Seq<RestrictionLayer>,
    scope: &CapabilityScope,
    index: nat,
)
    requires index <= layers.len(),
    ensures
        queries::layers_query_rank_from(layers, scope, index)
            == if model::restriction_deny_matches_from(layers, scope, index) {
                0int
            } else if queries::matching_approval_from(layers, scope, index, 0) {
                1int
            } else {
                2int
            },
    decreases layers.len() - index,
{
    if index < layers.len() {
        rule_suffix_rank_is_exact(layers[index as int].spec_rules(), scope, 0);
        two_dimensional_match_at_layer(layers, scope, index, 0);
        layer_suffix_rank_is_exact(layers, scope, index + 1);
    }
}

/// The evaluator aggregate and structural layer fold assign the same pointwise rank.
pub(crate) proof fn structural_and_composed_query_ranks_agree(
    policy: &PolicyDefinition,
    scope: &CapabilityScope,
)
    ensures
        queries::composed_query_rank(policy, scope)
            == queries::structural_policy_query_rank(policy, scope),
{
    let layers = policy.spec_layers();
    let empty = approvals::empty_approval_values();
    super::monotonicity::approval_required_matches(layers, scope, 0, 0, empty);
    layer_suffix_rank_is_exact(layers, scope, 0);
}

} // verus!
