//! Executable fact projections paired with Verus specifications for E3 invariants.

#![allow(
    clippy::fn_params_excessive_bools,
    clippy::struct_excessive_bools,
    reason = "formal fact projections keep each independent invariant premise explicit"
)]

#[doc = " Executable conservation check over bounded counters."]
#[must_use]
pub const fn accounting_conserved(
    expected: u32,
    passed: u32,
    task_failed: u32,
    infrastructure_failed: u32,
    cancelled: u32,
    ambiguous: u32,
) -> bool {
    if passed > expected {
        false
    } else {
        let after_passed = expected - passed;
        if task_failed > after_passed {
            false
        } else {
            let after_task_failed = after_passed - task_failed;
            if infrastructure_failed > after_task_failed {
                false
            } else {
                let after_infrastructure = after_task_failed - infrastructure_failed;
                if cancelled > after_infrastructure {
                    false
                } else {
                    ambiguous == after_infrastructure - cancelled
                }
            }
        }
    }
}
#[doc = " Executable pass@k precondition projection."]
#[must_use]
pub fn pass_at_k_preconditions(total: u32, successes: u32, k: u16) -> bool {
    total > 0 && successes <= total && k > 0 && u32::from(k) <= total
}
#[doc = " Executable terminal-dominance projection."]
#[must_use]
pub const fn terminal_dominates(current_terminal: bool, successor_terminal: bool) -> bool {
    !current_terminal || successor_terminal
}
#[doc = " Executable facts establishing one immutable evaluation-profile identity."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrozenProfileFacts {
    dataset_digest_bound: bool,
    partition_digest_bound: bool,
    provider_snapshot_bound: bool,
    model_controls_bound: bool,
    harness_revisions_bound: bool,
    policies_bound: bool,
}
impl FrozenProfileFacts {
    #[doc = " Creates the complete frozen-profile fact projection."]
    #[must_use]
    pub const fn new(
        dataset_digest_bound: bool,
        partition_digest_bound: bool,
        provider_snapshot_bound: bool,
        model_controls_bound: bool,
        harness_revisions_bound: bool,
        policies_bound: bool,
    ) -> Self {
        Self {
            dataset_digest_bound,
            partition_digest_bound,
            provider_snapshot_bound,
            model_controls_bound,
            harness_revisions_bound,
            policies_bound,
        }
    }
    #[doc = " Returns whether the dataset identity is bound."]
    #[must_use]
    pub const fn dataset_digest_bound(self) -> bool {
        self.dataset_digest_bound
    }
    #[doc = " Returns whether selected partitions are bound."]
    #[must_use]
    pub const fn partition_digest_bound(self) -> bool {
        self.partition_digest_bound
    }
    #[doc = " Returns whether provider snapshots are bound."]
    #[must_use]
    pub const fn provider_snapshot_bound(self) -> bool {
        self.provider_snapshot_bound
    }
    #[doc = " Returns whether model controls are bound."]
    #[must_use]
    pub const fn model_controls_bound(self) -> bool {
        self.model_controls_bound
    }
    #[doc = " Returns whether both harness revisions are bound."]
    #[must_use]
    pub const fn harness_revisions_bound(self) -> bool {
        self.harness_revisions_bound
    }
    #[doc = " Returns whether retry, infrastructure, metric, and seed policies are bound."]
    #[must_use]
    pub const fn policies_bound(self) -> bool {
        self.policies_bound
    }
}
#[doc = " Proves the executable projection is exactly frozen-profile identity."]
#[must_use]
pub const fn frozen_profile(facts: FrozenProfileFacts) -> bool {
    facts.dataset_digest_bound
        && facts.partition_digest_bound
        && facts.provider_snapshot_bound
        && facts.model_controls_bound
        && facts.harness_revisions_bound
        && facts.policies_bound
}
#[doc = " Executable facts for complete, unique, monotonic rollout accounting."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LedgerValidityFacts {
    planned_ids_unique: bool,
    terminal_ids_unique: bool,
    planned_equals_terminal: bool,
    attempts_monotonic: bool,
    attempts_bounded: bool,
    one_terminal_per_rollout: bool,
    paired_arms_conserved: bool,
}
impl LedgerValidityFacts {
    #[doc = " Creates a complete ledger-validity fact projection."]
    #[must_use]
    pub const fn new(
        planned_ids_unique: bool,
        terminal_ids_unique: bool,
        planned_equals_terminal: bool,
        attempts_monotonic: bool,
        attempts_bounded: bool,
        one_terminal_per_rollout: bool,
        paired_arms_conserved: bool,
    ) -> Self {
        Self {
            planned_ids_unique,
            terminal_ids_unique,
            planned_equals_terminal,
            attempts_monotonic,
            attempts_bounded,
            one_terminal_per_rollout,
            paired_arms_conserved,
        }
    }
    #[doc = " Returns whether planned identities are duplicate-free."]
    #[must_use]
    pub const fn planned_ids_unique(self) -> bool {
        self.planned_ids_unique
    }
    #[doc = " Returns whether terminal identities are duplicate-free."]
    #[must_use]
    pub const fn terminal_ids_unique(self) -> bool {
        self.terminal_ids_unique
    }
    #[doc = " Returns whether the planned and terminal identity sets match."]
    #[must_use]
    pub const fn planned_equals_terminal(self) -> bool {
        self.planned_equals_terminal
    }
    #[doc = " Returns whether attempt ordinals increase exactly."]
    #[must_use]
    pub const fn attempts_monotonic(self) -> bool {
        self.attempts_monotonic
    }
    #[doc = " Returns whether every attempt list respects its frozen ceiling."]
    #[must_use]
    pub const fn attempts_bounded(self) -> bool {
        self.attempts_bounded
    }
    #[doc = " Returns whether every rollout has one terminal classification."]
    #[must_use]
    pub const fn one_terminal_per_rollout(self) -> bool {
        self.one_terminal_per_rollout
    }
    #[doc = " Returns whether candidate and baseline logical pairs are conserved."]
    #[must_use]
    pub const fn paired_arms_conserved(self) -> bool {
        self.paired_arms_conserved
    }
}
#[doc = " Proves complete rollout accounting from its executable projection."]
#[must_use]
pub const fn ledger_validity(facts: LedgerValidityFacts) -> bool {
    facts.planned_ids_unique
        && facts.terminal_ids_unique
        && facts.planned_equals_terminal
        && facts.attempts_monotonic
        && facts.attempts_bounded
        && facts.one_terminal_per_rollout
        && facts.paired_arms_conserved
}
#[doc = " Executable facts for portable statistical validity."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StatisticalValidityFacts {
    pass_at_k_preconditions: bool,
    probabilities_bounded: bool,
    wilson_binomial_only: bool,
    intervals_ordered: bool,
    paired_tasks_complete: bool,
    bootstrap_deterministic: bool,
    infrastructure_excluded_by_policy: bool,
}
impl StatisticalValidityFacts {
    #[doc = " Creates the complete statistical-validity fact projection."]
    #[must_use]
    pub const fn new(
        pass_at_k_preconditions: bool,
        probabilities_bounded: bool,
        wilson_binomial_only: bool,
        intervals_ordered: bool,
        paired_tasks_complete: bool,
        bootstrap_deterministic: bool,
        infrastructure_excluded_by_policy: bool,
    ) -> Self {
        Self {
            pass_at_k_preconditions,
            probabilities_bounded,
            wilson_binomial_only,
            intervals_ordered,
            paired_tasks_complete,
            bootstrap_deterministic,
            infrastructure_excluded_by_policy,
        }
    }
    #[doc = " Returns whether pass-at-k preconditions hold."]
    #[must_use]
    pub const fn pass_at_k_preconditions(self) -> bool {
        self.pass_at_k_preconditions
    }
    #[doc = " Returns whether all probability values are bounded."]
    #[must_use]
    pub const fn probabilities_bounded(self) -> bool {
        self.probabilities_bounded
    }
    #[doc = " Returns whether Wilson intervals use raw binomial counts only."]
    #[must_use]
    pub const fn wilson_binomial_only(self) -> bool {
        self.wilson_binomial_only
    }
    #[doc = " Returns whether every interval has ordered bounded endpoints."]
    #[must_use]
    pub const fn intervals_ordered(self) -> bool {
        self.intervals_ordered
    }
    #[doc = " Returns whether paired analysis uses complete task clusters."]
    #[must_use]
    pub const fn paired_tasks_complete(self) -> bool {
        self.paired_tasks_complete
    }
    #[doc = " Returns whether bootstrap resampling is deterministically seeded."]
    #[must_use]
    pub const fn bootstrap_deterministic(self) -> bool {
        self.bootstrap_deterministic
    }
    #[doc = " Returns whether infrastructure outcomes follow the frozen exclusion policy."]
    #[must_use]
    pub const fn infrastructure_excluded_by_policy(self) -> bool {
        self.infrastructure_excluded_by_policy
    }
}
#[doc = " Proves checked metric inputs satisfy the complete statistical contract."]
#[must_use]
pub const fn statistical_validity(facts: StatisticalValidityFacts) -> bool {
    facts.pass_at_k_preconditions
        && facts.probabilities_bounded
        && facts.wilson_binomial_only
        && facts.intervals_ordered
        && facts.paired_tasks_complete
        && facts.bootstrap_deterministic
        && facts.infrastructure_excluded_by_policy
}
#[doc = " Executable facts for one legal aggregate transition."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransitionValidityFacts {
    command_fence_exact: bool,
    phase_edge_legal: bool,
    rollout_status_edge_legal: bool,
    artifact_dependencies_finalized: bool,
    outbox_effect_matches_event: bool,
    terminal_dominates: bool,
}
impl TransitionValidityFacts {
    #[doc = " Creates a complete aggregate-transition projection."]
    #[must_use]
    pub const fn new(
        command_fence_exact: bool,
        phase_edge_legal: bool,
        rollout_status_edge_legal: bool,
        artifact_dependencies_finalized: bool,
        outbox_effect_matches_event: bool,
        terminal_dominates: bool,
    ) -> Self {
        Self {
            command_fence_exact,
            phase_edge_legal,
            rollout_status_edge_legal,
            artifact_dependencies_finalized,
            outbox_effect_matches_event,
            terminal_dominates,
        }
    }
    #[doc = " Returns whether the command fence matches the current head."]
    #[must_use]
    pub const fn command_fence_exact(self) -> bool {
        self.command_fence_exact
    }
    #[doc = " Returns whether the campaign phase edge is legal."]
    #[must_use]
    pub const fn phase_edge_legal(self) -> bool {
        self.phase_edge_legal
    }
    #[doc = " Returns whether each affected rollout status edge is legal."]
    #[must_use]
    pub const fn rollout_status_edge_legal(self) -> bool {
        self.rollout_status_edge_legal
    }
    #[doc = " Returns whether referenced artifacts are finalized."]
    #[must_use]
    pub const fn artifact_dependencies_finalized(self) -> bool {
        self.artifact_dependencies_finalized
    }
    #[doc = " Returns whether durable effects correspond exactly to the accepted event."]
    #[must_use]
    pub const fn outbox_effect_matches_event(self) -> bool {
        self.outbox_effect_matches_event
    }
    #[doc = " Returns whether terminal dominance holds."]
    #[must_use]
    pub const fn terminal_dominates(self) -> bool {
        self.terminal_dominates
    }
}
#[doc = " Proves a pure reducer transition satisfies its durable obligations."]
#[must_use]
pub const fn transition_validity(facts: TransitionValidityFacts) -> bool {
    facts.command_fence_exact
        && facts.phase_edge_legal
        && facts.rollout_status_edge_legal
        && facts.artifact_dependencies_finalized
        && facts.outbox_effect_matches_event
        && facts.terminal_dominates
}
#[doc = " Executable replay/checkpoint refinement facts."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReplayRefinementFacts {
    canonical_prefix_complete: bool,
    sequence_equal: bool,
    previous_event_equal: bool,
    semantic_state_equal: bool,
    state_digest_equal: bool,
    no_duplicate_effects: bool,
}
impl ReplayRefinementFacts {
    #[doc = " Creates a complete replay-refinement fact projection."]
    #[must_use]
    pub const fn new(
        canonical_prefix_complete: bool,
        sequence_equal: bool,
        previous_event_equal: bool,
        semantic_state_equal: bool,
        state_digest_equal: bool,
        no_duplicate_effects: bool,
    ) -> Self {
        Self {
            canonical_prefix_complete,
            sequence_equal,
            previous_event_equal,
            semantic_state_equal,
            state_digest_equal,
            no_duplicate_effects,
        }
    }
    #[doc = " Returns whether replay consumed the canonical prefix."]
    #[must_use]
    pub const fn canonical_prefix_complete(self) -> bool {
        self.canonical_prefix_complete
    }
    #[doc = " Returns whether replay and checkpoint sequences match."]
    #[must_use]
    pub const fn sequence_equal(self) -> bool {
        self.sequence_equal
    }
    #[doc = " Returns whether replay and checkpoint predecessor identities match."]
    #[must_use]
    pub const fn previous_event_equal(self) -> bool {
        self.previous_event_equal
    }
    #[doc = " Returns whether semantic states match."]
    #[must_use]
    pub const fn semantic_state_equal(self) -> bool {
        self.semantic_state_equal
    }
    #[doc = " Returns whether canonical state digests match."]
    #[must_use]
    pub const fn state_digest_equal(self) -> bool {
        self.state_digest_equal
    }
    #[doc = " Returns whether retry emitted no duplicate effect."]
    #[must_use]
    pub const fn no_duplicate_effects(self) -> bool {
        self.no_duplicate_effects
    }
}
#[doc = " Proves replay and the checked checkpoint are equivalent and retry-safe."]
#[must_use]
pub const fn replay_refinement(facts: ReplayRefinementFacts) -> bool {
    facts.canonical_prefix_complete
        && facts.sequence_equal
        && facts.previous_event_equal
        && facts.semantic_state_equal
        && facts.state_digest_equal
        && facts.no_duplicate_effects
}
#[doc = " Executable absence-of-authority facts for evaluation output."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NonAuthorityFacts {
    no_harness_mutation: bool,
    no_acceptance_authority: bool,
    no_promotion_authority: bool,
    no_deployment_authority: bool,
    source_evidence_unchanged: bool,
    report_inert: bool,
}
impl NonAuthorityFacts {
    #[doc = " Creates a complete evaluation non-authority projection."]
    #[must_use]
    pub const fn new(
        no_harness_mutation: bool,
        no_acceptance_authority: bool,
        no_promotion_authority: bool,
        no_deployment_authority: bool,
        source_evidence_unchanged: bool,
        report_inert: bool,
    ) -> Self {
        Self {
            no_harness_mutation,
            no_acceptance_authority,
            no_promotion_authority,
            no_deployment_authority,
            source_evidence_unchanged,
            report_inert,
        }
    }
    #[doc = " Returns whether harness mutation is absent."]
    #[must_use]
    pub const fn no_harness_mutation(self) -> bool {
        self.no_harness_mutation
    }
    #[doc = " Returns whether acceptance authority is absent."]
    #[must_use]
    pub const fn no_acceptance_authority(self) -> bool {
        self.no_acceptance_authority
    }
    #[doc = " Returns whether promotion authority is absent."]
    #[must_use]
    pub const fn no_promotion_authority(self) -> bool {
        self.no_promotion_authority
    }
    #[doc = " Returns whether deployment authority is absent."]
    #[must_use]
    pub const fn no_deployment_authority(self) -> bool {
        self.no_deployment_authority
    }
    #[doc = " Returns whether source evidence is unchanged."]
    #[must_use]
    pub const fn source_evidence_unchanged(self) -> bool {
        self.source_evidence_unchanged
    }
    #[doc = " Returns whether the report representation is inert."]
    #[must_use]
    pub const fn report_inert(self) -> bool {
        self.report_inert
    }
}
#[doc = " Proves evaluation evidence cannot act as acceptance, promotion, or deployment authority."]
#[must_use]
pub const fn non_authority(facts: NonAuthorityFacts) -> bool {
    facts.no_harness_mutation
        && facts.no_acceptance_authority
        && facts.no_promotion_authority
        && facts.no_deployment_authority
        && facts.source_evidence_unchanged
        && facts.report_inert
}
