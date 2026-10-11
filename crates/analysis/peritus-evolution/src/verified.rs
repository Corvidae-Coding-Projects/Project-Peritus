//! Executable fact projections paired with Verus specifications for F0 invariants.

#![allow(
    clippy::fn_params_excessive_bools,
    clippy::struct_excessive_bools,
    reason = "formal fact projections keep independent safety premises explicit"
)]

#[doc = " Executable terminal-dominance projection."]
#[must_use]
pub const fn terminal_dominance(current_terminal: bool, publication_only: bool) -> bool {
    !current_terminal || publication_only
}
#[doc = " Executable deny-wins eligibility check."]
#[must_use]
pub const fn deny_wins(criteria_complete: bool, failed: u32, unavailable: u32) -> bool {
    criteria_complete && failed == 0 && unavailable == 0
}
#[doc = " Checked facts establishing `INV-018 EvaluatorIsolation`."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvaluatorIsolationFacts {
    exact_evaluation_arms: bool,
    frozen_dataset_profile: bool,
    sealed_evaluator_preserved: bool,
    promotion_policy_preserved: bool,
    protected_assets_preserved: bool,
    declared_changes_complete: bool,
}
impl EvaluatorIsolationFacts {
    #[doc = " Creates the complete executable isolation projection."]
    #[must_use]
    pub const fn new(
        exact_evaluation_arms: bool,
        frozen_dataset_profile: bool,
        sealed_evaluator_preserved: bool,
        promotion_policy_preserved: bool,
        protected_assets_preserved: bool,
        declared_changes_complete: bool,
    ) -> Self {
        Self {
            exact_evaluation_arms,
            frozen_dataset_profile,
            sealed_evaluator_preserved,
            promotion_policy_preserved,
            protected_assets_preserved,
            declared_changes_complete,
        }
    }
    #[doc = " Exact E1 arms are bound."]
    #[must_use]
    pub const fn exact_evaluation_arms(self) -> bool {
        self.exact_evaluation_arms
    }
    #[doc = " Dataset and evaluator profile are frozen."]
    #[must_use]
    pub const fn frozen_dataset_profile(self) -> bool {
        self.frozen_dataset_profile
    }
    #[doc = " Sealed evaluator material is unchanged."]
    #[must_use]
    pub const fn sealed_evaluator_preserved(self) -> bool {
        self.sealed_evaluator_preserved
    }
    #[doc = " Protected promotion policy is unchanged."]
    #[must_use]
    pub const fn promotion_policy_preserved(self) -> bool {
        self.promotion_policy_preserved
    }
    #[doc = " All other protected assets are unchanged."]
    #[must_use]
    pub const fn protected_assets_preserved(self) -> bool {
        self.protected_assets_preserved
    }
    #[doc = " Manifest union equals the actual E1 delta."]
    #[must_use]
    pub const fn declared_changes_complete(self) -> bool {
        self.declared_changes_complete
    }
}
#[doc = " Executable `INV-018` refinement root."]
#[must_use]
pub const fn evaluator_isolation(facts: EvaluatorIsolationFacts) -> bool {
    facts.exact_evaluation_arms
        && facts.frozen_dataset_profile
        && facts.sealed_evaluator_preserved
        && facts.promotion_policy_preserved
        && facts.protected_assets_preserved
        && facts.declared_changes_complete
}
#[doc = " Checked facts establishing `INV-019 PromotionSafety`."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PromotionSafetyFacts {
    immutable_inputs: bool,
    baseline_current: bool,
    attribution_complete: bool,
    selection_eligible: bool,
    evaluator_isolated: bool,
    schema_compatible: bool,
    required_review_complete: bool,
    action_exact: bool,
    authority_exact: bool,
    approval_fresh: bool,
}
impl PromotionSafetyFacts {
    #[doc = " Creates the complete promotion-safety projection."]
    #[must_use]
    #[allow(clippy::too_many_arguments, reason = "independent promotion premises remain visible")]
    pub const fn new(
        immutable_inputs: bool,
        baseline_current: bool,
        attribution_complete: bool,
        selection_eligible: bool,
        evaluator_isolated: bool,
        schema_compatible: bool,
        required_review_complete: bool,
        action_exact: bool,
        authority_exact: bool,
        approval_fresh: bool,
    ) -> Self {
        Self {
            immutable_inputs,
            baseline_current,
            attribution_complete,
            selection_eligible,
            evaluator_isolated,
            schema_compatible,
            required_review_complete,
            action_exact,
            authority_exact,
            approval_fresh,
        }
    }
}
#[doc = " Executable `INV-019` refinement root."]
#[must_use]
pub const fn promotion_safety(facts: PromotionSafetyFacts) -> bool {
    facts.immutable_inputs
        && facts.baseline_current
        && facts.attribution_complete
        && facts.selection_eligible
        && facts.evaluator_isolated
        && facts.schema_compatible
        && facts.required_review_complete
        && facts.action_exact
        && facts.authority_exact
        && facts.approval_fresh
}
#[doc = " Executable rollback-legality refinement root."]
#[must_use]
pub const fn rollback_legality(
    target_retained: bool,
    target_distinct: bool,
    compatible: bool,
    policy_exact: bool,
    authority_exact: bool,
    approval_fresh: bool,
) -> bool {
    target_retained
        && target_distinct
        && compatible
        && policy_exact
        && authority_exact
        && approval_fresh
}
#[doc = " Executable pointer-conservation projection."]
#[must_use]
pub const fn pointer_conservation(
    generation_increments: bool,
    predecessor_exact: bool,
    successor_exact: bool,
    history_appended: bool,
) -> bool {
    generation_increments && predecessor_exact && successor_exact && history_appended
}
#[doc = " Executable replay-equivalence projection."]
#[must_use]
pub const fn replay_refinement(
    sequence_contiguous: bool,
    head_exact: bool,
    prior_digest_exact: bool,
    immutable_binding_exact: bool,
    successor_digest_exact: bool,
) -> bool {
    sequence_contiguous
        && head_exact
        && prior_digest_exact
        && immutable_binding_exact
        && successor_digest_exact
}
