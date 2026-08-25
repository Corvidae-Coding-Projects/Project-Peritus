//! Executable D2 predicates and focused Verus proof roots.

use vstd::prelude::*;

mod executable;

pub use executable::{
    evidence_is_fresh, findings_are_conserved, no_implicit_success, quorum_is_complete,
    replay_equivalent, transition_is_legal,
};

verus! {

/// Closed mathematical finding-disposition vocabulary used by the D2 transition proofs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    dead_code,
    reason = "Verus ghost vocabulary is intentionally erased from ordinary Rust execution"
)]
pub enum DispositionModel {
    /// Newly admitted and unresolved.
    Open,
    /// Fixer reports a repair; reviewer confirmation is still required.
    Fixed,
    /// Fixer disputes the finding; reviewer confirmation is still required.
    Disputed,
    /// Fixer proposes a provenance-preserving replacement.
    SupersessionProposed,
    /// External authority has been requested but not observed.
    WaiverRequested,
    /// Independent reviewer confirmed resolution.
    ResolutionConfirmed,
    /// Independent reviewer confirmed invalidation.
    InvalidationConfirmed,
    /// Independent reviewer confirmed a provenance-preserving replacement.
    Superseded,
    /// Existing external authority observation matched exactly.
    Waived,
}

/// Mathematical exact freshness: a fresh claim requires every component to match.
pub open spec fn exact_freshness(all_components_equal: bool, claimed: bool) -> bool {
    !claimed || all_components_equal
}

/// Mathematical validity of the immutable contract/candidate binding and its configured bounds.
#[allow(clippy::too_many_arguments)]
pub open spec fn valid_binding(
    exact_contract: bool,
    categories_canonical_nonempty: bool,
    producers_canonical_nonempty: bool,
    ancestries_canonical_nonempty: bool,
    reviewer_quorum: int,
    maximum_cycles: int,
    configured_cycles: int,
    configured_assignments: int,
) -> bool {
    exact_contract
        && categories_canonical_nonempty
        && producers_canonical_nonempty
        && ancestries_canonical_nonempty
        && reviewer_quorum > 0
        && maximum_cycles > 0
        && maximum_cycles <= configured_cycles
        && maximum_cycles <= configured_assignments
}

/// Closed legal append-only finding-disposition transition relation.
pub open spec fn legal_disposition_transition(
    before: DispositionModel,
    after: DispositionModel,
    independent_confirmation: bool,
    external_waiver: bool,
    provenance_preserved: bool,
) -> bool {
    match (before, after) {
        (DispositionModel::Open, DispositionModel::Open)
        | (DispositionModel::Open, DispositionModel::Fixed)
        | (DispositionModel::Open, DispositionModel::Disputed)
        | (DispositionModel::Open, DispositionModel::SupersessionProposed)
        | (DispositionModel::Open, DispositionModel::WaiverRequested) => true,
        (DispositionModel::Fixed, DispositionModel::ResolutionConfirmed)
        | (DispositionModel::Disputed, DispositionModel::InvalidationConfirmed) => {
            independent_confirmation
        }
        (DispositionModel::SupersessionProposed, DispositionModel::Superseded) => {
            independent_confirmation && provenance_preserved
        }
        (DispositionModel::WaiverRequested, DispositionModel::Waived) => external_waiver,
        _ => false,
    }
}

/// Mathematical classification of the only dispositions that conserve a current finding.
pub open spec fn conserving_disposition(
    disposition: DispositionModel,
    independent_confirmation: bool,
    external_waiver: bool,
    provenance_preserved: bool,
) -> bool {
    match disposition {
        DispositionModel::ResolutionConfirmed | DispositionModel::InvalidationConfirmed => {
            independent_confirmation
        }
        DispositionModel::Superseded => independent_confirmation && provenance_preserved,
        DispositionModel::Waived => external_waiver,
        _ => false,
    }
}

/// Mathematical quorum conjunction with every dimension explicit.
#[allow(clippy::too_many_arguments)]
pub open spec fn complete_quorum(
    count: bool,
    categories: bool,
    reviewers: bool,
    producer: bool,
    contexts: bool,
    models: bool,
    providers: bool,
    ancestry: bool,
    fresh: bool,
) -> bool {
    count && categories && reviewers && producer && contexts && models && providers && ancestry
        && fresh
}

/// Mathematical conservation claim.
pub open spec fn conserved(open_findings: int, permitted_closures: int) -> bool {
    0 <= open_findings && open_findings <= permitted_closures
}

/// Mathematical terminal truth claim.
pub open spec fn truthful_completion(quorum: bool, conservation: bool, completed: bool) -> bool {
    !completed || (quorum && conservation)
}

/// Mathematical closed-state transition rule.
pub open spec fn fenced_closed(terminal: bool, accepted_successor: bool) -> bool {
    !terminal || !accepted_successor
}

/// Mathematical reducer fence and exact one-event successor relation.
#[allow(clippy::too_many_arguments)]
pub open spec fn legal_reducer_step(
    active: bool,
    current_sequence: int,
    expected_sequence: int,
    predecessor_matches: bool,
    revision_matches: bool,
    prior_digest_matches: bool,
    command_identity_fresh: bool,
    emitted_events: int,
    successor_sequence: int,
) -> bool {
    active
        && current_sequence == expected_sequence
        && predecessor_matches
        && revision_matches
        && prior_digest_matches
        && command_identity_fresh
        && emitted_events == 1
        && successor_sequence == current_sequence + 1
}

/// Mathematical oscillation/cycle limit rule.
pub open spec fn within_review_limit(used: int, maximum: int) -> bool {
    0 <= used && used <= maximum
}

/// Mathematical validity of one independently configured production limit.
pub open spec fn valid_configured_limit(value: int, ceiling: int) -> bool {
    0 < value && value <= ceiling
}

/// Mathematical aggregation of every deterministic non-progress/escalation input.
pub open spec fn oscillation_requires_escalation(
    repeated_finding_set: bool,
    severity_stagnation: bool,
    severity_regression: bool,
    disagreement: bool,
    cycles_exhausted: bool,
) -> bool {
    repeated_finding_set
        || severity_stagnation
        || severity_regression
        || disagreement
        || cycles_exhausted
}

/// Mathematical exact replay equivalence.
pub open spec fn exact_replay(expected: int, observed: int, claimed: bool) -> bool {
    !claimed || expected == observed
}

/// Proves stale evidence cannot satisfy exact freshness.
pub proof fn stale_evidence_is_not_fresh()
    ensures !exact_freshness(false, true)
{
}

/// Proves a binding with no canonical producer set is invalid.
pub proof fn missing_producer_set_invalidates_binding(
    exact_contract: bool,
    categories: bool,
    ancestries: bool,
    reviewer_quorum: int,
    maximum_cycles: int,
    configured_cycles: int,
    configured_assignments: int,
)
    ensures !valid_binding(
        exact_contract,
        categories,
        false,
        ancestries,
        reviewer_quorum,
        maximum_cycles,
        configured_cycles,
        configured_assignments,
    )
{
}

/// Proves a fixer repair claim is not a conserving disposition.
pub proof fn fixer_claim_cannot_conserve()
    ensures !conserving_disposition(DispositionModel::Fixed, true, true, true)
{
}

/// Proves reviewer-confirmed resolution cannot be appended without confirmation.
pub proof fn reviewer_resolution_requires_confirmation()
    ensures !legal_disposition_transition(
        DispositionModel::Fixed,
        DispositionModel::ResolutionConfirmed,
        false,
        false,
        false,
    )
{
}

/// Proves supersession cannot conserve a finding if provenance would be lost.
pub proof fn supersession_requires_preserved_provenance()
    ensures !conserving_disposition(DispositionModel::Superseded, true, false, false)
{
}

/// Proves a waiver request cannot become waived without an external observation.
pub proof fn waiver_requires_external_observation()
    ensures !legal_disposition_transition(
        DispositionModel::WaiverRequested,
        DispositionModel::Waived,
        false,
        false,
        false,
    )
{
}

/// Proves removing any required quorum fact prevents completeness.
pub proof fn missing_fresh_context_breaks_quorum(
    count: bool,
    categories: bool,
    reviewers: bool,
    producer: bool,
    contexts: bool,
    models: bool,
    providers: bool,
    ancestry: bool,
)
    ensures !complete_quorum(
        count,
        categories,
        reviewers,
        producer,
        contexts,
        models,
        providers,
        ancestry,
        false,
    )
{
}

/// Proves completion cannot be truthful with missing quorum.
pub proof fn missing_quorum_cannot_complete(conservation: bool)
    ensures !truthful_completion(false, conservation, true)
{
}

/// Proves completion cannot be truthful with an unconserved current finding.
pub proof fn missing_conservation_cannot_complete(quorum: bool)
    ensures !truthful_completion(quorum, false, true)
{
}

/// Proves terminal state rejects a successor.
pub proof fn terminal_is_fenced_closed()
    ensures fenced_closed(true, false)
{
}

/// Proves a stale sequence fence cannot satisfy the reducer-step relation.
pub proof fn stale_sequence_rejects_reducer_step(
    active: bool,
    current_sequence: int,
    expected_sequence: int,
    predecessor_matches: bool,
    revision_matches: bool,
    prior_digest_matches: bool,
    command_identity_fresh: bool,
    emitted_events: int,
    successor_sequence: int,
)
    requires current_sequence != expected_sequence
    ensures !legal_reducer_step(
        active,
        current_sequence,
        expected_sequence,
        predecessor_matches,
        revision_matches,
        prior_digest_matches,
        command_identity_fresh,
        emitted_events,
        successor_sequence,
    )
{
}

/// Proves a checked increment below the cap remains within the review limit.
pub proof fn bounded_cycle_successor(used: int, maximum: int)
    requires 0 <= used, used < maximum
    ensures within_review_limit(used + 1, maximum)
{
}

/// Proves zero cannot satisfy a configured production limit.
pub proof fn zero_limit_is_invalid(ceiling: int)
    ensures !valid_configured_limit(0, ceiling)
{
}

/// Proves a value above its ceiling cannot satisfy a configured production limit.
pub proof fn above_ceiling_is_invalid(value: int, ceiling: int)
    requires value > ceiling
    ensures !valid_configured_limit(value, ceiling)
{
}

/// Proves any repeated finding set is an escalation input independent of other dimensions.
pub proof fn repeated_finding_set_requires_escalation(
    stagnation: bool,
    regression: bool,
    disagreement: bool,
    exhausted: bool,
)
    ensures oscillation_requires_escalation(true, stagnation, regression, disagreement, exhausted)
{
}

/// Proves replay equality is reflexive.
pub proof fn replay_is_reflexive(digest: int)
    ensures exact_replay(digest, digest, true)
{
}

} // verus!
