//! Executable C7 trace obligations proved with Verus.

use vstd::prelude::*;

verus! {

/// Independent facts required for one causal observation transition.
#[allow(
    clippy::struct_excessive_bools,
    reason = "the proof exposes independent causal predicates without packing or aliasing them"
)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CausalFacts {
    /// The structural parent exists and remains open when required.
    pub parent_valid: bool,
    /// Every event predecessor exists in the same trace.
    pub predecessors_valid: bool,
    /// Span sequence is the unique successor.
    pub sequence_valid: bool,
    /// Entity binding is unchanged or a legal refinement.
    pub binding_valid: bool,
    /// Caller-observed time did not regress.
    pub time_monotonic: bool,
}

/// Mathematical complete causal integrity predicate.
pub open spec fn causal_integrity_spec(facts: CausalFacts) -> bool {
    facts.parent_valid
        && facts.predecessors_valid
        && facts.sequence_valid
        && facts.binding_valid
        && facts.time_monotonic
}

/// Checks every independent causal transition fact.
#[must_use]
pub const fn causal_integrity(facts: CausalFacts) -> (result: bool)
    ensures result == causal_integrity_spec(facts),
{
    facts.parent_valid
        && facts.predecessors_valid
        && facts.sequence_valid
        && facts.binding_valid
        && facts.time_monotonic
}

/// Checks the exact non-wrapping successor relation.
#[must_use]
pub const fn next_sequence(previous: u64, observed: u64) -> (valid: bool)
    ensures valid == (previous < u64::MAX && observed == previous + 1),
{
    previous < u64::MAX && observed == previous + 1
}

/// Checks exact duplicate suppression: identity and canonical digest must both match.
#[must_use]
pub const fn exact_duplicate(identity_matches: bool, digest_matches: bool) -> (valid: bool)
    ensures valid == (identity_matches && digest_matches),
{
    identity_matches && digest_matches
}

/// Scalar projection proving an observation cannot mutate authority or budget state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NonAuthorityFacts {
    /// Authority measure before observation.
    pub authority_before: u64,
    /// Authority measure after observation.
    pub authority_after: u64,
    /// Budget measure before observation.
    pub budget_before: u64,
    /// Budget measure after observation.
    pub budget_after: u64,
    /// Execution-state measure before observation.
    pub execution_before: u64,
    /// Execution-state measure after observation.
    pub execution_after: u64,
}

/// Mathematical C7 non-authority predicate.
pub open spec fn preserves_authority_spec(facts: NonAuthorityFacts) -> bool {
    facts.authority_before == facts.authority_after
        && facts.budget_before == facts.budget_after
        && facts.execution_before == facts.execution_after
}

/// Checks that tracing changes no authoritative scalar projection.
#[must_use]
pub const fn preserves_authority(facts: NonAuthorityFacts) -> (valid: bool)
    ensures valid == preserves_authority_spec(facts),
{
    facts.authority_before == facts.authority_after
        && facts.budget_before == facts.budget_after
        && facts.execution_before == facts.execution_after
}

/// Checks the redaction decision: raw content is absent or exact encrypted vault facts all hold.
#[must_use]
#[allow(
    clippy::fn_params_excessive_bools,
    reason = "the proof keeps each independent redaction fact explicit"
)]
pub const fn redaction_decision(
    raw_exposed: bool,
    vault_present: bool,
    finalized: bool,
    encrypted: bool,
    digest_matches: bool,
) -> (valid: bool)
    ensures valid == (!raw_exposed && (!vault_present || (finalized && encrypted && digest_matches))),
{
    !raw_exposed && (!vault_present || (finalized && encrypted && digest_matches))
}

/// Checks scalar replay equivalence for identical genesis, input count, and fold digest.
#[must_use]
pub const fn replay_equivalent(
    genesis_matches: bool,
    input_count_matches: bool,
    fold_digest_matches: bool,
) -> (valid: bool)
    ensures valid == (genesis_matches && input_count_matches && fold_digest_matches),
{
    genesis_matches && input_count_matches && fold_digest_matches
}

} // verus!
