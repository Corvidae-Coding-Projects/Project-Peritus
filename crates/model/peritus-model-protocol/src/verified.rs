//! Verus-checked scalar projection of C5 protocol state and legality predicates.

#![allow(
    clippy::struct_excessive_bools,
    reason = "formal projections keep every independent obligation visible"
)]

use vstd::prelude::*;

verus! {

/// Independent state facts required before one normalized reducer transition may be accepted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReducerTransitionFacts {
    /// Local/provider ordering is valid.
    pub ordering_valid: bool,
    /// Event and aggregate byte/count limits hold.
    pub bounds_valid: bool,
    /// The target response/item/call phase permits the event.
    pub phase_valid: bool,
    /// Identity uniqueness or exact deduplication holds.
    pub identity_valid: bool,
    /// No terminal outcome was already established.
    pub terminal_open: bool,
}

/// Mathematical complete transition predicate.
pub open spec fn reducer_transition_legal_spec(facts: ReducerTransitionFacts) -> bool {
    facts.ordering_valid
        && facts.bounds_valid
        && facts.phase_valid
        && facts.identity_valid
        && facts.terminal_open
}

/// Checks the complete scalar transition projection.
#[must_use]
pub const fn reducer_transition_legal(facts: ReducerTransitionFacts) -> (result: bool)
    ensures result == reducer_transition_legal_spec(facts),
{
    facts.ordering_valid
        && facts.bounds_valid
        && facts.phase_valid
        && facts.identity_valid
        && facts.terminal_open
}

/// Exact provider-event deduplication facts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeduplicationFacts {
    /// Provider identity matches the earlier event.
    pub identity_matches: bool,
    /// Exact raw-event digest matches.
    pub digest_matches: bool,
    /// Provider ordering identity/sequence matches.
    pub provider_sequence_matches: bool,
    /// Local sequence is the repeated original or next observed envelope.
    pub local_sequence_compatible: bool,
}

/// Mathematical exact-deduplication predicate.
pub open spec fn deduplication_legal_spec(facts: DeduplicationFacts) -> bool {
    facts.identity_matches
        && facts.digest_matches
        && facts.provider_sequence_matches
        && facts.local_sequence_compatible
}

/// Checks that duplicate suppression cannot hide changed provider bytes or ordering.
#[must_use]
pub const fn deduplication_legal(facts: DeduplicationFacts) -> (result: bool)
    ensures result == deduplication_legal_spec(facts),
{
    facts.identity_matches
        && facts.digest_matches
        && facts.provider_sequence_matches
        && facts.local_sequence_compatible
}

/// Completion facts for one fragmented semantic item.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FragmentCompletionFacts {
    /// Fragment bytes fit their item and response bounds.
    pub bytes_bounded: bool,
    /// Text boundary is complete UTF-8 when text is required.
    pub utf8_complete: bool,
    /// JSON is syntactically complete when JSON is required.
    pub json_complete: bool,
    /// Tool/item closing event was observed.
    pub explicitly_closed: bool,
}

/// Mathematical fragment-completion predicate.
pub open spec fn fragment_completion_legal_spec(facts: FragmentCompletionFacts) -> bool {
    facts.bytes_bounded
        && facts.utf8_complete
        && facts.json_complete
        && facts.explicitly_closed
}

/// Checks that fragmented text/JSON cannot become complete output early.
#[must_use]
pub const fn fragment_completion_legal(facts: FragmentCompletionFacts) -> (result: bool)
    ensures result == fragment_completion_legal_spec(facts),
{
    facts.bytes_bounded
        && facts.utf8_complete
        && facts.json_complete
        && facts.explicitly_closed
}

/// Independent retry-legality facts selected by the retry table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetryLegalityFacts {
    /// Attempt and elapsed bounds permit more work.
    pub bounds_allow: bool,
    /// Cancellation has not won.
    pub not_cancelled: bool,
    /// Prior terminal completion has not occurred.
    pub not_terminal: bool,
    /// A fresh retry is definitely safe or documented-deduplicated.
    pub fresh_retry_safe: bool,
    /// Exact resumption is documented when partial output exists.
    pub partial_has_exact_resume: bool,
    /// The chosen action matches the cause/phase table.
    pub action_matches_cause: bool,
}

/// Mathematical complete retry predicate.
pub open spec fn retry_legality_complete_spec(facts: RetryLegalityFacts) -> bool {
    facts.bounds_allow
        && facts.not_cancelled
        && facts.not_terminal
        && facts.fresh_retry_safe
        && facts.partial_has_exact_resume
        && facts.action_matches_cause
}

/// Checks every independent retry authorization fact.
#[must_use]
pub const fn retry_legality_complete(facts: RetryLegalityFacts) -> (result: bool)
    ensures result == retry_legality_complete_spec(facts),
{
    facts.bounds_allow
        && facts.not_cancelled
        && facts.not_terminal
        && facts.fresh_retry_safe
        && facts.partial_has_exact_resume
        && facts.action_matches_cause
}

/// Provider observations before/after one reducer transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthorityObservation {
    /// Authoritative capability/policy measure before observation.
    pub authority_before: u64,
    /// Authoritative capability/policy measure after observation.
    pub authority_after: u64,
    /// Authoritative B1 budget measure before observation.
    pub budget_before: u64,
    /// Authoritative B1 budget measure after observation.
    pub budget_after: u64,
}

/// Mathematical non-authority rule for provider observations.
pub open spec fn provider_observation_preserves_authority_spec(
    facts: AuthorityObservation,
) -> bool {
    facts.authority_after == facts.authority_before
        && facts.budget_after == facts.budget_before
}

/// Checks that provider observations neither grant authority nor mint/refund B1 budget.
#[must_use]
pub const fn provider_observation_preserves_authority(
    facts: AuthorityObservation,
) -> (result: bool)
    ensures result == provider_observation_preserves_authority_spec(facts),
{
    facts.authority_after == facts.authority_before
        && facts.budget_after == facts.budget_before
}

/// Mathematical required-capability subset predicate.
pub open spec fn capability_mask_legal_spec(required: u64, supported: u64) -> bool {
    required & !supported == 0
}

/// Checks that every required bit is proven supported.
#[must_use]
pub const fn capability_mask_legal(required: u64, supported: u64) -> (result: bool)
    ensures result == capability_mask_legal_spec(required, supported),
{
    required & !supported == 0
}

/// Mathematical exact next-sequence predicate without wraparound.
pub open spec fn next_sequence_legal_spec(previous: u64, next: u64) -> bool {
    previous < u64::MAX && next == previous + 1
}

/// Checks exact monotonic local sequence progression.
#[must_use]
pub const fn next_sequence_legal(previous: u64, next: u64) -> (result: bool)
    ensures result == next_sequence_legal_spec(previous, next),
{
    previous < u64::MAX && next == previous + 1
}

/// Mathematical optional-counter monotonicity predicate.
pub open spec fn usage_counter_monotonic_spec(
    previous_present: bool,
    previous: u64,
    next_present: bool,
    next: u64,
) -> bool {
    !previous_present || !next_present || next >= previous
}

/// Checks that an observed cumulative counter cannot regress; missing remains unknown.
#[must_use]
pub const fn usage_counter_monotonic(
    previous_present: bool,
    previous: u64,
    next_present: bool,
    next: u64,
) -> (result: bool)
    ensures result == usage_counter_monotonic_spec(
        previous_present,
        previous,
        next_present,
        next,
    ),
{
    !previous_present || !next_present || next >= previous
}

} // verus!

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_transition_fact_is_required() {
        let complete = ReducerTransitionFacts {
            ordering_valid: true,
            bounds_valid: true,
            phase_valid: true,
            identity_valid: true,
            terminal_open: true,
        };
        assert!(reducer_transition_legal(complete));
        assert!(!reducer_transition_legal(ReducerTransitionFacts {
            ordering_valid: false,
            ..complete
        }));
        assert!(!reducer_transition_legal(ReducerTransitionFacts {
            bounds_valid: false,
            ..complete
        }));
        assert!(!reducer_transition_legal(ReducerTransitionFacts {
            phase_valid: false,
            ..complete
        }));
        assert!(!reducer_transition_legal(ReducerTransitionFacts {
            identity_valid: false,
            ..complete
        }));
        assert!(!reducer_transition_legal(ReducerTransitionFacts {
            terminal_open: false,
            ..complete
        }));
    }

    #[test]
    fn provider_observations_preserve_authoritative_state() {
        assert!(provider_observation_preserves_authority(AuthorityObservation {
            authority_before: 9,
            authority_after: 9,
            budget_before: 21,
            budget_after: 21,
        }));
        assert!(!provider_observation_preserves_authority(AuthorityObservation {
            authority_before: 9,
            authority_after: 10,
            budget_before: 21,
            budget_after: 21,
        }));
        assert!(!provider_observation_preserves_authority(AuthorityObservation {
            authority_before: 9,
            authority_after: 9,
            budget_before: 21,
            budget_after: 22,
        }));
    }
}
