//! Executable buffering, replay, and non-authority obligations proved with Verus.

#[doc = " Exact bounded queue accounting transition."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BufferFacts {
    #[doc = " Queue length before enqueue."]
    pub length_before: u64,
    #[doc = " Configured nonzero capacity."]
    pub capacity: u64,
    #[doc = " Queue length after enqueue."]
    pub length_after: u64,
    #[doc = " Drop counter before enqueue."]
    pub drops_before: u64,
    #[doc = " Drop counter after enqueue."]
    pub drops_after: u64,
    #[doc = " Whether the queue was full before enqueue."]
    pub was_full: bool,
}
#[doc = " Checks queue bounds and exact drop accounting."]
#[must_use]
pub const fn bounded_accounting(facts: BufferFacts) -> bool {
    facts.capacity > 0
        && facts.length_before <= facts.capacity
        && facts.length_after <= facts.capacity
        && facts.was_full == (facts.length_before == facts.capacity)
        && if facts.was_full {
            facts.drops_before < u64::MAX && facts.drops_after == facts.drops_before + 1
        } else {
            facts.drops_after == facts.drops_before
        }
}
#[doc = " Checks monotonic submitted, accepted, dropped, and exported accounting."]
#[must_use]
#[allow(
    clippy::too_many_arguments,
    reason = "the proof exposes four independent before-and-after counter pairs"
)]
pub const fn counters_monotonic(
    submitted_before: u64,
    submitted_after: u64,
    accepted_before: u64,
    accepted_after: u64,
    dropped_before: u64,
    dropped_after: u64,
    exported_before: u64,
    exported_after: u64,
) -> bool {
    submitted_after >= submitted_before
        && accepted_after >= accepted_before
        && dropped_after >= dropped_before
        && exported_after >= exported_before
        && accepted_after <= submitted_after
        && dropped_after <= submitted_after
        && exported_after <= accepted_after
}
#[doc = " Checks exact whole-batch acknowledgement; partial success is never accepted."]
#[must_use]
#[allow(
    clippy::fn_params_excessive_bools,
    reason = "the proof keeps every independently checked acknowledgement field explicit"
)]
pub const fn acknowledgement_legal(
    stream_matches: bool,
    batch_matches: bool,
    first_matches: bool,
    last_matches: bool,
    count_matches: bool,
) -> bool {
    stream_matches && batch_matches && first_matches && last_matches && count_matches
}
#[doc = " Checks checkpoint replay equivalence through an exact prefix."]
#[must_use]
#[allow(
    clippy::fn_params_excessive_bools,
    reason = "the proof keeps the stream, position, prefix, and counter facts independent"
)]
pub const fn recovery_prefix_legal(
    stream_matches: bool,
    sequence_not_future: bool,
    prefix_matches: bool,
    counters_valid: bool,
) -> bool {
    stream_matches && sequence_not_future && prefix_matches && counters_valid
}
#[doc = " Checks exact, overflow-free accounting for a contiguous final-disposition prefix."]
#[must_use]
pub const fn disposition_prefix_legal(submitted: u64, dropped: u64, exported: u64) -> bool {
    exported <= submitted && dropped == submitted - exported
}
#[doc = " Scalar proof that export changes no authority, execution, or budget measure."]
#[must_use]
pub const fn export_preserves_authority(
    authority_before: u64,
    authority_after: u64,
    execution_before: u64,
    execution_after: u64,
    budget_before: u64,
    budget_after: u64,
) -> bool {
    authority_before == authority_after
        && execution_before == execution_after
        && budget_before == budget_after
}
