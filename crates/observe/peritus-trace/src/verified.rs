//! Executable C7 trace obligations proved with Verus.

#[doc = " Independent facts required for one causal observation transition."]
#[allow(
    clippy::struct_excessive_bools,
    reason = "the proof exposes independent causal predicates without packing or aliasing them"
)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CausalFacts {
    #[doc = " The structural parent exists and remains open when required."]
    pub parent_valid: bool,
    #[doc = " Every event predecessor exists in the same trace."]
    pub predecessors_valid: bool,
    #[doc = " Span sequence is the unique successor."]
    pub sequence_valid: bool,
    #[doc = " Entity binding is unchanged or a legal refinement."]
    pub binding_valid: bool,
    #[doc = " Caller-observed time did not regress."]
    pub time_monotonic: bool,
}
#[doc = " Checks every independent causal transition fact."]
#[must_use]
pub const fn causal_integrity(facts: CausalFacts) -> bool {
    facts.parent_valid
        && facts.predecessors_valid
        && facts.sequence_valid
        && facts.binding_valid
        && facts.time_monotonic
}
#[doc = " Checks the exact non-wrapping successor relation."]
#[must_use]
pub const fn next_sequence(previous: u64, observed: u64) -> bool {
    previous < u64::MAX && observed == previous + 1
}
#[doc = " Checks exact duplicate suppression: identity and canonical digest must both match."]
#[must_use]
pub const fn exact_duplicate(identity_matches: bool, digest_matches: bool) -> bool {
    identity_matches && digest_matches
}
#[doc = " Scalar projection proving an observation cannot mutate authority or budget state."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NonAuthorityFacts {
    #[doc = " Authority measure before observation."]
    pub authority_before: u64,
    #[doc = " Authority measure after observation."]
    pub authority_after: u64,
    #[doc = " Budget measure before observation."]
    pub budget_before: u64,
    #[doc = " Budget measure after observation."]
    pub budget_after: u64,
    #[doc = " Execution-state measure before observation."]
    pub execution_before: u64,
    #[doc = " Execution-state measure after observation."]
    pub execution_after: u64,
}
#[doc = " Checks that tracing changes no authoritative scalar projection."]
#[must_use]
pub const fn preserves_authority(facts: NonAuthorityFacts) -> bool {
    facts.authority_before == facts.authority_after
        && facts.budget_before == facts.budget_after
        && facts.execution_before == facts.execution_after
}
#[doc = " Checks the redaction decision: raw content is absent or exact encrypted vault facts all hold."]
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
) -> bool {
    !raw_exposed && (!vault_present || (finalized && encrypted && digest_matches))
}
#[doc = " Checks scalar replay equivalence for identical genesis, input count, and fold digest."]
#[must_use]
pub const fn replay_equivalent(
    genesis_matches: bool,
    input_count_matches: bool,
    fold_digest_matches: bool,
) -> bool {
    genesis_matches && input_count_matches && fold_digest_matches
}
