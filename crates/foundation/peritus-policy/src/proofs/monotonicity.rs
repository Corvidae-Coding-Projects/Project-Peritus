//! Proofs that ordinary composition only shrinks pointwise allowed query sets (`INV-022`).

#[cfg(verus_only)]
use crate::{
    approval_fold_model as approvals, model, monotonicity_model as queries,
    AuthorizationRequest, CapabilityScope, PolicyDecision, PolicyDecisionKind, PolicyDefinition,
    RestrictionLayer,
};
use vstd::prelude::*;

verus! {

proof fn composition_is_no_broader(left: int, right: int)
    ensures
        model::decision_meet(left, right) <= left,
        model::decision_meet(left, right) <= right,
{}

pub(crate) proof fn deny_wins(other: int)
    requires other >= 0,
    ensures model::decision_meet(0, other) == 0,
{}

proof fn authorized_identity_is_neutral(other: int)
    requires 0 <= other <= 2,
    ensures model::decision_meet(2, other) == other,
{}

proof fn composition_is_associative(first: int, second: int, third: int)
    ensures
        model::decision_meet(first, model::decision_meet(second, third))
            == model::decision_meet(model::decision_meet(first, second), third),
{}

proof fn adding_restriction_cannot_increase(
    base: int,
    restriction: int,
    minimum_rank: int,
)
    requires
        0 <= minimum_rank <= 2,
    ensures
        model::decision_meet(base, restriction) <= base,
        queries::query_is_allowed(
            model::decision_meet(base, restriction),
            minimum_rank,
        ) ==> queries::query_is_allowed(base, minimum_rank),
{
    composition_is_no_broader(base, restriction);
}

proof fn layer_rank_is_bounded(layer: &RestrictionLayer, scope: &CapabilityScope)
    ensures 0 <= queries::layer_query_rank(layer, scope) <= 2,
{
    proof fn suffix(rules: Seq<crate::RestrictionRule>, scope: &CapabilityScope, index: nat)
        requires index <= rules.len(),
        ensures 0 <= queries::layer_query_rank_from(rules, scope, index) <= 2,
        decreases rules.len() - index,
    {
        if index < rules.len() {
            suffix(rules, scope, index + 1);
        }
    }
    suffix(layer.spec_rules(), scope, 0);
}

proof fn appended_layers_rank(
    base: Seq<RestrictionLayer>,
    added: &RestrictionLayer,
    scope: &CapabilityScope,
    index: nat,
)
    requires index <= base.len(),
    ensures
        queries::layers_query_rank_from(base.push(*added), scope, index)
            == model::decision_meet(
                queries::layers_query_rank_from(base, scope, index),
                queries::layer_query_rank(added, scope),
            ),
    decreases base.len() - index,
{
    layer_rank_is_bounded(added, scope);
    assert(base.push(*added).len() == base.len() + 1);
    if index < base.len() {
        assert(base.push(*added)[index as int] == base[index as int]);
        assert(queries::layers_query_rank_from(base.push(*added), scope, index)
            == model::decision_meet(
                queries::layer_query_rank(&base[index as int], scope),
                queries::layers_query_rank_from(base.push(*added), scope, index + 1),
            ));
        assert(queries::layers_query_rank_from(base, scope, index)
            == model::decision_meet(
                queries::layer_query_rank(&base[index as int], scope),
                queries::layers_query_rank_from(base, scope, index + 1),
            ));
        appended_layers_rank(base, added, scope, index + 1);
        composition_is_associative(
            queries::layer_query_rank(&base[index as int], scope),
            queries::layers_query_rank_from(base, scope, index + 1),
            queries::layer_query_rank(added, scope),
        );
    } else {
        assert(index == base.len());
        assert(base.push(*added)[index as int] == *added);
        assert(queries::layers_query_rank_from(base, scope, index) == 2);
        assert(queries::layers_query_rank_from(base.push(*added), scope, index + 1) == 2);
        assert(queries::layers_query_rank_from(base.push(*added), scope, index)
            == model::decision_meet(queries::layer_query_rank(added, scope), 2));
        authorized_identity_is_neutral(queries::layer_query_rank(added, scope));
    }
}

/// Appending one exact ordinary layer preserves pointwise allowed-query-set inclusion.
pub(crate) proof fn appending_one_layer_cannot_increase_allowed_query_set(
    before: &PolicyDefinition,
    after: &PolicyDefinition,
    added: &RestrictionLayer,
    scope: &CapabilityScope,
    minimum_rank: int,
)
    requires
        after.spec_is_one_layer_extension_of(before, added),
        0 <= minimum_rank <= 2,
    ensures
        queries::structural_policy_query_rank(after, scope)
            <= queries::structural_policy_query_rank(before, scope),
        queries::query_is_allowed(
            queries::structural_policy_query_rank(after, scope),
            minimum_rank,
        ) ==> queries::query_is_allowed(
            queries::structural_policy_query_rank(before, scope),
            minimum_rank,
        ),
{
    after.one_layer_extension_preserves_ceiling_query(before, added, scope);
    appended_layers_rank(before.spec_layers(), added, scope, 0);
    composition_is_no_broader(
        queries::structural_policy_query_rank(before, scope),
        queries::layer_query_rank(added, scope),
    );
}

proof fn approval_combine_requires(
    accumulated: approvals::ApprovalValues,
    requirement: &crate::ApprovalRequirement,
)
    requires accumulated.conflict ==> accumulated.required,
    ensures
        approvals::combine_approval_values(accumulated, requirement).required,
        approvals::combine_approval_values(accumulated, requirement).conflict
            ==> approvals::combine_approval_values(accumulated, requirement).required,
{
}

pub(super) proof fn approval_required_matches(
    layers: Seq<RestrictionLayer>,
    scope: &CapabilityScope,
    layer_index: nat,
    rule_index: nat,
    accumulated: approvals::ApprovalValues,
)
    requires
        layer_index <= layers.len(),
        layer_index < layers.len()
            ==> rule_index <= layers[layer_index as int].spec_rules().len(),
        accumulated.conflict ==> accumulated.required,
    ensures
        approvals::approval_values_from_layers(
            layers,
            scope,
            layer_index,
            rule_index,
            accumulated,
        ).conflict ==> approvals::approval_values_from_layers(
            layers,
            scope,
            layer_index,
            rule_index,
            accumulated,
        ).required,
        approvals::approval_values_from_layers(
            layers,
            scope,
            layer_index,
            rule_index,
            accumulated,
        ).required == (
            accumulated.required
                || queries::matching_approval_from(
                    layers,
                    scope,
                    layer_index,
                    rule_index,
                )
        ),
    decreases
        layers.len() - layer_index,
        if layer_index < layers.len() {
            layers[layer_index as int].spec_rules().len() - rule_index
        } else {
            0
        },
{
    if layer_index < layers.len() {
        let rules = layers[layer_index as int].spec_rules();
        if rule_index >= rules.len() {
            approval_required_matches(layers, scope, layer_index + 1, 0, accumulated);
        } else {
            let rule = rules[rule_index as int];
            match rule.spec_approval_requirement() {
                Some(requirement) if rule.spec_matches_scope(scope) => {
                    let next = approvals::combine_approval_values(accumulated, &requirement);
                    approval_combine_requires(accumulated, &requirement);
                    approval_required_matches(
                        layers,
                        scope,
                        layer_index,
                        rule_index + 1,
                        next,
                    );
                }
                _ => {
                    approval_required_matches(
                        layers,
                        scope,
                        layer_index,
                        rule_index + 1,
                        accumulated,
                    );
                }
            }
        }
    }
}

/// Connects the executable evaluator's exact decision to pointwise set inclusion.
pub(crate) proof fn evaluator_cannot_broaden_allowed_queries(
    policy: &PolicyDefinition,
    request: &AuthorizationRequest,
    decision: &PolicyDecision,
)
    requires
        policy.spec_matches_policy_id(&request.spec_scope_value()),
        policy.spec_boundary_contains(&request.spec_scope_value()),
        policy.spec_first_operation_denial(&request.spec_scope_value()).is_none(),
        !policy.spec_has_immutable_deny(&request.spec_scope_value()),
        !policy.spec_has_restriction_deny(&request.spec_scope_value()),
        policy.spec_has_full_coverage(&request.spec_scope_value()),
        crate::constraint_model::decision_has_exact_constraints(
            policy,
            &request.spec_scope_value(),
            decision,
        ),
        approvals::decision_has_exact_approval(
            policy,
            &request.spec_scope_value(),
            decision,
        ),
    ensures
        queries::decision_respects_allowed_query_set(
            policy,
            &request.spec_scope_value(),
            decision,
        ),
{
    let scope = request.spec_scope_value();
    let base = queries::ceiling_query_rank(policy, &scope);
    let restriction = queries::ordinary_restriction_rank(policy, &scope);
    super::query_bridge::structural_and_composed_query_ranks_agree(policy, &scope);
    decision.variant_agrees_with_spec_kind();
    assert(base == 2);
    assert(restriction >= 1);
    assert(queries::decision_rank(decision.spec_kind()) <= restriction) by {
        if decision.spec_kind() == PolicyDecisionKind::Authorized {
            assert(!approvals::policy_approval_values(policy, &scope).required);
        }
    };
    assert(queries::decision_rank(decision.spec_kind())
        <= model::decision_meet(base, restriction));
    assert forall |minimum_rank: int| 0 <= minimum_rank <= 2
        && #[trigger] queries::query_is_allowed(
            queries::decision_rank(decision.spec_kind()),
            minimum_rank,
        ) implies queries::query_is_allowed(base, minimum_rank) by {
        adding_restriction_cannot_increase(base, restriction, minimum_rank);
    };
}

} // verus!
