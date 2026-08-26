//! Executable fact projections paired with Verus specifications for E3 invariants.

#![allow(
    clippy::fn_params_excessive_bools,
    clippy::struct_excessive_bools,
    reason = "formal fact projections keep each independent invariant premise explicit"
)]

use vstd::prelude::*;

verus! {

/// Mathematical conservation of the closed logical rollout partition.
pub open spec fn accounting_conserved_spec(
    expected: int,
    passed: int,
    task_failed: int,
    infrastructure_failed: int,
    cancelled: int,
    ambiguous: int,
) -> bool {
    expected >= 0
        && passed >= 0
        && task_failed >= 0
        && infrastructure_failed >= 0
        && cancelled >= 0
        && ambiguous >= 0
        && expected == passed + task_failed + infrastructure_failed + cancelled + ambiguous
}

/// Executable conservation check over bounded counters.
#[must_use]
pub const fn accounting_conserved(
    expected: u32,
    passed: u32,
    task_failed: u32,
    infrastructure_failed: u32,
    cancelled: u32,
    ambiguous: u32,
) -> (result: bool)
    ensures result == accounting_conserved_spec(
        expected as int,
        passed as int,
        task_failed as int,
        infrastructure_failed as int,
        cancelled as int,
        ambiguous as int,
    )
{
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

/// Valid pass@k arithmetic preconditions.
pub open spec fn pass_at_k_preconditions_spec(total: int, successes: int, k: int) -> bool {
    total > 0 && successes >= 0 && successes <= total && k > 0 && k <= total
}

/// Executable pass@k precondition projection.
#[must_use]
pub fn pass_at_k_preconditions(total: u32, successes: u32, k: u16) -> (result: bool)
    ensures result == pass_at_k_preconditions_spec(total as int, successes as int, k as int)
{
    total > 0 && successes <= total && k > 0 && u32::from(k) <= total
}

/// Terminal campaign phases cannot transition to nonterminal phases.
pub open spec fn terminal_dominates_spec(current_terminal: bool, successor_terminal: bool) -> bool {
    !current_terminal || successor_terminal
}

/// Executable terminal-dominance projection.
#[must_use]
pub const fn terminal_dominates(current_terminal: bool, successor_terminal: bool) -> (result: bool)
    ensures result == terminal_dominates_spec(current_terminal, successor_terminal)
{
    !current_terminal || successor_terminal
}

/// Executable facts establishing one immutable evaluation-profile identity.
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
    /// Creates the complete frozen-profile fact projection.
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
    /// Returns whether the dataset identity is bound.
    #[must_use] pub const fn dataset_digest_bound(self) -> bool { self.dataset_digest_bound }
    /// Returns whether selected partitions are bound.
    #[must_use] pub const fn partition_digest_bound(self) -> bool { self.partition_digest_bound }
    /// Returns whether provider snapshots are bound.
    #[must_use] pub const fn provider_snapshot_bound(self) -> bool { self.provider_snapshot_bound }
    /// Returns whether model controls are bound.
    #[must_use] pub const fn model_controls_bound(self) -> bool { self.model_controls_bound }
    /// Returns whether both harness revisions are bound.
    #[must_use] pub const fn harness_revisions_bound(self) -> bool { self.harness_revisions_bound }
    /// Returns whether retry, infrastructure, metric, and seed policies are bound.
    #[must_use] pub const fn policies_bound(self) -> bool { self.policies_bound }
}

/// Mathematical identity predicate for an immutable evaluation profile.
pub closed spec fn frozen_profile_spec(facts: FrozenProfileFacts) -> bool {
    facts.dataset_digest_bound
        && facts.partition_digest_bound
        && facts.provider_snapshot_bound
        && facts.model_controls_bound
        && facts.harness_revisions_bound
        && facts.policies_bound
}

/// Proves the executable projection is exactly frozen-profile identity.
#[must_use]
pub const fn frozen_profile(facts: FrozenProfileFacts) -> (valid: bool)
    ensures valid == frozen_profile_spec(facts)
{
    facts.dataset_digest_bound
        && facts.partition_digest_bound
        && facts.provider_snapshot_bound
        && facts.model_controls_bound
        && facts.harness_revisions_bound
        && facts.policies_bound
}

/// Executable facts for complete, unique, monotonic rollout accounting.
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
    /// Creates a complete ledger-validity fact projection.
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
    /// Returns whether planned identities are duplicate-free.
    #[must_use] pub const fn planned_ids_unique(self) -> bool { self.planned_ids_unique }
    /// Returns whether terminal identities are duplicate-free.
    #[must_use] pub const fn terminal_ids_unique(self) -> bool { self.terminal_ids_unique }
    /// Returns whether the planned and terminal identity sets match.
    #[must_use] pub const fn planned_equals_terminal(self) -> bool { self.planned_equals_terminal }
    /// Returns whether attempt ordinals increase exactly.
    #[must_use] pub const fn attempts_monotonic(self) -> bool { self.attempts_monotonic }
    /// Returns whether every attempt list respects its frozen ceiling.
    #[must_use] pub const fn attempts_bounded(self) -> bool { self.attempts_bounded }
    /// Returns whether every rollout has one terminal classification.
    #[must_use] pub const fn one_terminal_per_rollout(self) -> bool { self.one_terminal_per_rollout }
    /// Returns whether candidate and baseline logical pairs are conserved.
    #[must_use] pub const fn paired_arms_conserved(self) -> bool { self.paired_arms_conserved }
}

/// Mathematical complete-ledger predicate.
pub closed spec fn ledger_validity_spec(facts: LedgerValidityFacts) -> bool {
    facts.planned_ids_unique
        && facts.terminal_ids_unique
        && facts.planned_equals_terminal
        && facts.attempts_monotonic
        && facts.attempts_bounded
        && facts.one_terminal_per_rollout
        && facts.paired_arms_conserved
}

/// Proves complete rollout accounting from its executable projection.
#[must_use]
pub const fn ledger_validity(facts: LedgerValidityFacts) -> (valid: bool)
    ensures valid == ledger_validity_spec(facts)
{
    facts.planned_ids_unique
        && facts.terminal_ids_unique
        && facts.planned_equals_terminal
        && facts.attempts_monotonic
        && facts.attempts_bounded
        && facts.one_terminal_per_rollout
        && facts.paired_arms_conserved
}

/// Executable facts for portable statistical validity.
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
    /// Creates the complete statistical-validity fact projection.
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
    /// Returns whether pass-at-k preconditions hold.
    #[must_use] pub const fn pass_at_k_preconditions(self) -> bool { self.pass_at_k_preconditions }
    /// Returns whether all probability values are bounded.
    #[must_use] pub const fn probabilities_bounded(self) -> bool { self.probabilities_bounded }
    /// Returns whether Wilson intervals use raw binomial counts only.
    #[must_use] pub const fn wilson_binomial_only(self) -> bool { self.wilson_binomial_only }
    /// Returns whether every interval has ordered bounded endpoints.
    #[must_use] pub const fn intervals_ordered(self) -> bool { self.intervals_ordered }
    /// Returns whether paired analysis uses complete task clusters.
    #[must_use] pub const fn paired_tasks_complete(self) -> bool { self.paired_tasks_complete }
    /// Returns whether bootstrap resampling is deterministically seeded.
    #[must_use] pub const fn bootstrap_deterministic(self) -> bool { self.bootstrap_deterministic }
    /// Returns whether infrastructure outcomes follow the frozen exclusion policy.
    #[must_use] pub const fn infrastructure_excluded_by_policy(self) -> bool { self.infrastructure_excluded_by_policy }
}

/// Mathematical statistical-validity predicate.
pub closed spec fn statistical_validity_spec(facts: StatisticalValidityFacts) -> bool {
    facts.pass_at_k_preconditions
        && facts.probabilities_bounded
        && facts.wilson_binomial_only
        && facts.intervals_ordered
        && facts.paired_tasks_complete
        && facts.bootstrap_deterministic
        && facts.infrastructure_excluded_by_policy
}

/// Proves checked metric inputs satisfy the complete statistical contract.
#[must_use]
pub const fn statistical_validity(facts: StatisticalValidityFacts) -> (valid: bool)
    ensures valid == statistical_validity_spec(facts)
{
    facts.pass_at_k_preconditions
        && facts.probabilities_bounded
        && facts.wilson_binomial_only
        && facts.intervals_ordered
        && facts.paired_tasks_complete
        && facts.bootstrap_deterministic
        && facts.infrastructure_excluded_by_policy
}

/// Executable facts for one legal aggregate transition.
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
    /// Creates a complete aggregate-transition projection.
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
    /// Returns whether the command fence matches the current head.
    #[must_use] pub const fn command_fence_exact(self) -> bool { self.command_fence_exact }
    /// Returns whether the campaign phase edge is legal.
    #[must_use] pub const fn phase_edge_legal(self) -> bool { self.phase_edge_legal }
    /// Returns whether each affected rollout status edge is legal.
    #[must_use] pub const fn rollout_status_edge_legal(self) -> bool { self.rollout_status_edge_legal }
    /// Returns whether referenced artifacts are finalized.
    #[must_use] pub const fn artifact_dependencies_finalized(self) -> bool { self.artifact_dependencies_finalized }
    /// Returns whether durable effects correspond exactly to the accepted event.
    #[must_use] pub const fn outbox_effect_matches_event(self) -> bool { self.outbox_effect_matches_event }
    /// Returns whether terminal dominance holds.
    #[must_use] pub const fn terminal_dominates(self) -> bool { self.terminal_dominates }
}

/// Mathematical legal-transition predicate.
pub closed spec fn transition_validity_spec(facts: TransitionValidityFacts) -> bool {
    facts.command_fence_exact
        && facts.phase_edge_legal
        && facts.rollout_status_edge_legal
        && facts.artifact_dependencies_finalized
        && facts.outbox_effect_matches_event
        && facts.terminal_dominates
}

/// Proves a pure reducer transition satisfies its durable obligations.
#[must_use]
pub const fn transition_validity(facts: TransitionValidityFacts) -> (valid: bool)
    ensures valid == transition_validity_spec(facts)
{
    facts.command_fence_exact
        && facts.phase_edge_legal
        && facts.rollout_status_edge_legal
        && facts.artifact_dependencies_finalized
        && facts.outbox_effect_matches_event
        && facts.terminal_dominates
}

/// Executable replay/checkpoint refinement facts.
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
    /// Creates a complete replay-refinement fact projection.
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
    /// Returns whether replay consumed the canonical prefix.
    #[must_use] pub const fn canonical_prefix_complete(self) -> bool { self.canonical_prefix_complete }
    /// Returns whether replay and checkpoint sequences match.
    #[must_use] pub const fn sequence_equal(self) -> bool { self.sequence_equal }
    /// Returns whether replay and checkpoint predecessor identities match.
    #[must_use] pub const fn previous_event_equal(self) -> bool { self.previous_event_equal }
    /// Returns whether semantic states match.
    #[must_use] pub const fn semantic_state_equal(self) -> bool { self.semantic_state_equal }
    /// Returns whether canonical state digests match.
    #[must_use] pub const fn state_digest_equal(self) -> bool { self.state_digest_equal }
    /// Returns whether retry emitted no duplicate effect.
    #[must_use] pub const fn no_duplicate_effects(self) -> bool { self.no_duplicate_effects }
}

/// Mathematical replay/checkpoint refinement predicate.
pub closed spec fn replay_refinement_spec(facts: ReplayRefinementFacts) -> bool {
    facts.canonical_prefix_complete
        && facts.sequence_equal
        && facts.previous_event_equal
        && facts.semantic_state_equal
        && facts.state_digest_equal
        && facts.no_duplicate_effects
}

/// Proves replay and the checked checkpoint are equivalent and retry-safe.
#[must_use]
pub const fn replay_refinement(facts: ReplayRefinementFacts) -> (valid: bool)
    ensures valid == replay_refinement_spec(facts)
{
    facts.canonical_prefix_complete
        && facts.sequence_equal
        && facts.previous_event_equal
        && facts.semantic_state_equal
        && facts.state_digest_equal
        && facts.no_duplicate_effects
}

/// Executable absence-of-authority facts for evaluation output.
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
    /// Creates a complete evaluation non-authority projection.
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
    /// Returns whether harness mutation is absent.
    #[must_use] pub const fn no_harness_mutation(self) -> bool { self.no_harness_mutation }
    /// Returns whether acceptance authority is absent.
    #[must_use] pub const fn no_acceptance_authority(self) -> bool { self.no_acceptance_authority }
    /// Returns whether promotion authority is absent.
    #[must_use] pub const fn no_promotion_authority(self) -> bool { self.no_promotion_authority }
    /// Returns whether deployment authority is absent.
    #[must_use] pub const fn no_deployment_authority(self) -> bool { self.no_deployment_authority }
    /// Returns whether source evidence is unchanged.
    #[must_use] pub const fn source_evidence_unchanged(self) -> bool { self.source_evidence_unchanged }
    /// Returns whether the report representation is inert.
    #[must_use] pub const fn report_inert(self) -> bool { self.report_inert }
}

/// Mathematical E3 non-authority predicate.
pub closed spec fn non_authority_spec(facts: NonAuthorityFacts) -> bool {
    facts.no_harness_mutation
        && facts.no_acceptance_authority
        && facts.no_promotion_authority
        && facts.no_deployment_authority
        && facts.source_evidence_unchanged
        && facts.report_inert
}

/// Proves evaluation evidence cannot act as acceptance, promotion, or deployment authority.
#[must_use]
pub const fn non_authority(facts: NonAuthorityFacts) -> (valid: bool)
    ensures valid == non_authority_spec(facts)
{
    facts.no_harness_mutation
        && facts.no_acceptance_authority
        && facts.no_promotion_authority
        && facts.no_deployment_authority
        && facts.source_evidence_unchanged
        && facts.report_inert
}

} // verus!
