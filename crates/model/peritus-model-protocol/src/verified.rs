//! Verus-checked scalar projection of C5 protocol state and legality predicates.

#![allow(
    clippy::struct_excessive_bools,
    reason = "formal projections keep every independent obligation visible"
)]

#[doc = " Independent state facts required before one normalized reducer transition may be accepted."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReducerTransitionFacts {
    #[doc = " Local/provider ordering is valid."]
    pub ordering_valid: bool,
    #[doc = " Event and aggregate byte/count limits hold."]
    pub bounds_valid: bool,
    #[doc = " The target response/item/call phase permits the event."]
    pub phase_valid: bool,
    #[doc = " Identity uniqueness or exact deduplication holds."]
    pub identity_valid: bool,
    #[doc = " No terminal outcome was already established."]
    pub terminal_open: bool,
}
#[doc = " Checks the complete scalar transition projection."]
#[must_use]
pub const fn reducer_transition_legal(facts: ReducerTransitionFacts) -> bool {
    facts.ordering_valid
        && facts.bounds_valid
        && facts.phase_valid
        && facts.identity_valid
        && facts.terminal_open
}
#[doc = " Exact provider-event deduplication facts."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeduplicationFacts {
    #[doc = " Provider identity matches the earlier event."]
    pub identity_matches: bool,
    #[doc = " Exact raw-event digest matches."]
    pub digest_matches: bool,
    #[doc = " Provider ordering identity/sequence matches."]
    pub provider_sequence_matches: bool,
    #[doc = " Local sequence is the repeated original or next observed envelope."]
    pub local_sequence_compatible: bool,
}
#[doc = " Checks that duplicate suppression cannot hide changed provider bytes or ordering."]
#[must_use]
pub const fn deduplication_legal(facts: DeduplicationFacts) -> bool {
    facts.identity_matches
        && facts.digest_matches
        && facts.provider_sequence_matches
        && facts.local_sequence_compatible
}
#[doc = " Completion facts for one fragmented semantic item."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FragmentCompletionFacts {
    #[doc = " Fragment bytes fit their item and response bounds."]
    pub bytes_bounded: bool,
    #[doc = " Text boundary is complete UTF-8 when text is required."]
    pub utf8_complete: bool,
    #[doc = " JSON is syntactically complete when JSON is required."]
    pub json_complete: bool,
    #[doc = " Tool/item closing event was observed."]
    pub explicitly_closed: bool,
}
#[doc = " Checks that fragmented text/JSON cannot become complete output early."]
#[must_use]
pub const fn fragment_completion_legal(facts: FragmentCompletionFacts) -> bool {
    facts.bytes_bounded && facts.utf8_complete && facts.json_complete && facts.explicitly_closed
}
#[doc = " Independent retry-legality facts selected by the retry table."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetryLegalityFacts {
    #[doc = " Attempt and elapsed bounds permit more work."]
    pub bounds_allow: bool,
    #[doc = " Cancellation has not won."]
    pub not_cancelled: bool,
    #[doc = " Prior terminal completion has not occurred."]
    pub not_terminal: bool,
    #[doc = " A fresh retry is definitely safe or documented-deduplicated."]
    pub fresh_retry_safe: bool,
    #[doc = " Exact resumption is documented when partial output exists."]
    pub partial_has_exact_resume: bool,
    #[doc = " The chosen action matches the cause/phase table."]
    pub action_matches_cause: bool,
}
#[doc = " Checks every independent retry authorization fact."]
#[must_use]
pub const fn retry_legality_complete(facts: RetryLegalityFacts) -> bool {
    facts.bounds_allow
        && facts.not_cancelled
        && facts.not_terminal
        && facts.fresh_retry_safe
        && facts.partial_has_exact_resume
        && facts.action_matches_cause
}
#[doc = " Provider observations before/after one reducer transition."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthorityObservation {
    #[doc = " Authoritative capability/policy measure before observation."]
    pub authority_before: u64,
    #[doc = " Authoritative capability/policy measure after observation."]
    pub authority_after: u64,
    #[doc = " Authoritative B1 budget measure before observation."]
    pub budget_before: u64,
    #[doc = " Authoritative B1 budget measure after observation."]
    pub budget_after: u64,
}
#[doc = " Checks that provider observations neither grant authority nor mint/refund B1 budget."]
#[must_use]
pub const fn provider_observation_preserves_authority(facts: AuthorityObservation) -> bool {
    facts.authority_after == facts.authority_before && facts.budget_after == facts.budget_before
}
#[doc = " Checks that every required bit is proven supported."]
#[must_use]
pub const fn capability_mask_legal(required: u64, supported: u64) -> bool {
    required & !supported == 0
}
#[doc = " Checks exact monotonic local sequence progression."]
#[must_use]
pub const fn next_sequence_legal(previous: u64, next: u64) -> bool {
    previous < u64::MAX && next == previous + 1
}
#[doc = " Checks that an observed cumulative counter cannot regress; missing remains unknown."]
#[must_use]
pub const fn usage_counter_monotonic(
    previous_present: bool,
    previous: u64,
    next_present: bool,
    next: u64,
) -> bool {
    !previous_present || !next_present || next >= previous
}
