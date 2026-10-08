//! Executable buffering, replay, and non-authority obligations proved with Verus.

use vstd::prelude::*;

verus! {

/// Exact physical-page, spill, and explicit-loss accounting transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BufferFacts {
    /// Resident page bytes before enqueue.
    pub resident_bytes_before: u64,
    /// Configured positive physical page size.
    pub page_bytes: u64,
    /// Configured positive physical page count.
    pub page_count: u64,
    /// Resident page bytes after enqueue.
    pub resident_bytes_after: u64,
    /// Drop counter before enqueue.
    pub drops_before: u64,
    /// Drop counter after enqueue.
    pub drops_after: u64,
    /// Durable-spill counter before enqueue.
    pub spilled_before: u64,
    /// Durable-spill counter after enqueue.
    pub spilled_after: u64,
    /// Whether the arriving canonical record fits resident memory under the current prefix pin.
    pub resident_fit: bool,
    /// Whether the caller selected durable lossless spill for overflow.
    pub lossless_spill: bool,
}

/// Mathematical byte-residency and explicit overflow-disposition predicate.
pub open spec fn bounded_accounting_spec(facts: BufferFacts) -> bool {
    facts.page_bytes > 0
        && facts.page_count > 0
        && facts.page_count <= u64::MAX / facts.page_bytes
        && facts.resident_bytes_before <= facts.page_bytes * facts.page_count
        && facts.resident_bytes_after <= facts.page_bytes * facts.page_count
        && if facts.resident_fit {
            facts.drops_after == facts.drops_before
                && facts.spilled_after == facts.spilled_before
        } else if facts.lossless_spill {
            facts.drops_after == facts.drops_before
                && facts.spilled_before < u64::MAX
                && facts.spilled_after == facts.spilled_before + 1
        } else {
            facts.drops_before < u64::MAX
                && facts.drops_after == facts.drops_before + 1
                && facts.spilled_after == facts.spilled_before
        }
}

/// Checks physical-page bounds and exact caller-selected spill or loss accounting.
#[must_use]
pub const fn bounded_accounting(facts: BufferFacts) -> (valid: bool)
    ensures valid == bounded_accounting_spec(facts),
{
    facts.page_bytes > 0
        && facts.page_count > 0
        && facts.page_count <= u64::MAX / facts.page_bytes
        && facts.resident_bytes_before <= facts.page_bytes * facts.page_count
        && facts.resident_bytes_after <= facts.page_bytes * facts.page_count
        && if facts.resident_fit {
            facts.drops_after == facts.drops_before
                && facts.spilled_after == facts.spilled_before
        } else if facts.lossless_spill {
            facts.drops_after == facts.drops_before
                && facts.spilled_before < u64::MAX
                && facts.spilled_after == facts.spilled_before + 1
        } else {
            facts.drops_before < u64::MAX
                && facts.drops_after == facts.drops_before + 1
                && facts.spilled_after == facts.spilled_before
        }
}

/// Checks monotonic submitted, accepted, dropped, and exported accounting.
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
) -> (valid: bool)
    ensures valid == (
        submitted_after >= submitted_before
            && accepted_after >= accepted_before
            && dropped_after >= dropped_before
            && exported_after >= exported_before
            && accepted_after <= submitted_after
            && dropped_after <= submitted_after
            && exported_after <= accepted_after
    ),
{
    submitted_after >= submitted_before
        && accepted_after >= accepted_before
        && dropped_after >= dropped_before
        && exported_after >= exported_before
        && accepted_after <= submitted_after
        && dropped_after <= submitted_after
        && exported_after <= accepted_after
}

/// Checks exact whole-batch acknowledgement; partial success is never accepted.
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
) -> (valid: bool)
    ensures valid == (
        stream_matches && batch_matches && first_matches && last_matches && count_matches
    ),
{
    stream_matches && batch_matches && first_matches && last_matches && count_matches
}

/// Checks checkpoint replay equivalence through an exact prefix.
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
) -> (valid: bool)
    ensures valid == (
        stream_matches && sequence_not_future && prefix_matches && counters_valid
    ),
{
    stream_matches && sequence_not_future && prefix_matches && counters_valid
}

/// Checks exact, overflow-free accounting for a contiguous final-disposition prefix.
#[must_use]
pub const fn disposition_prefix_legal(
    submitted: u64,
    dropped: u64,
    exported: u64,
) -> (valid: bool)
    ensures valid == (exported <= submitted && dropped == submitted - exported),
{
    exported <= submitted && dropped == submitted - exported
}

/// Scalar proof that export changes no authority, execution, or budget measure.
#[must_use]
pub const fn export_preserves_authority(
    authority_before: u64,
    authority_after: u64,
    execution_before: u64,
    execution_after: u64,
    budget_before: u64,
    budget_after: u64,
) -> (valid: bool)
    ensures valid == (
        authority_before == authority_after
            && execution_before == execution_after
            && budget_before == budget_after
    ),
{
    authority_before == authority_after
        && execution_before == execution_after
        && budget_before == budget_after
}

} // verus!
