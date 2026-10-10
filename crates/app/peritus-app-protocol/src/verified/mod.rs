//! Executable A3 safety predicates paired with Verus specifications.

#[doc = " Checks one candidate against one supported inclusive range."]
#[must_use]
pub const fn version_supported(
    major: u16,
    minor: u16,
    supported_major: u16,
    minimum_minor: u16,
    maximum_minor: u16,
) -> bool {
    major > 0
        && major == supported_major
        && minimum_minor <= maximum_minor
        && minimum_minor <= minor
        && minor <= maximum_minor
}
#[doc = " Checks all premises required for a successful negotiated session."]
#[allow(
    clippy::fn_params_excessive_bools,
    reason = "the executable refinement predicate exposes each independent proof premise"
)]
#[must_use]
pub const fn negotiation_safe(
    mutually_supported: bool,
    required_features_present: bool,
    limits_nonzero: bool,
    selected_session: bool,
) -> bool {
    mutually_supported && required_features_present && limits_nonzero && selected_session
}
#[doc = " Returns the unique successor cursor without numeric wraparound."]
#[must_use]
pub const fn next_cursor(last: u64) -> Option<u64> {
    last.checked_add(1)
}
#[doc = " Checks that one distinct delivery strictly advances the scanned source position."]
#[must_use]
pub const fn delivery_advances(scanned: u64, observed: u64) -> bool {
    scanned < observed
}
#[doc = " Checks that a cumulative acknowledgement closes an actually delivered prefix."]
#[must_use]
pub const fn ack_legal(
    last_acknowledged: u64,
    last_delivered: u64,
    acknowledged: u64,
    gap_open: bool,
    delivered_member: bool,
) -> bool {
    !gap_open
        && last_acknowledged <= acknowledged
        && acknowledged <= last_delivered
        && (acknowledged == last_acknowledged || delivered_member)
}
#[doc = " Checks the exact at-least-once redelivery identity relation."]
#[allow(
    clippy::fn_params_excessive_bools,
    reason = "the executable refinement predicate exposes each stable identity premise"
)]
#[must_use]
pub const fn redelivery_identity(
    same_cursor: bool,
    same_event: bool,
    same_frame_digest: bool,
    same_subscription: bool,
    attempt_advances: bool,
) -> bool {
    same_cursor && same_event && same_frame_digest && same_subscription && attempt_advances
}
#[doc = " Checks one chunk without permitting overlap, gaps, limit excess, or total-size overflow."]
#[must_use]
pub const fn chunk_accepted(
    accepted_bytes: u64,
    offset: u64,
    chunk_bytes: u64,
    total_bytes: u64,
    chunk_limit: u64,
) -> bool {
    offset == accepted_bytes
        && chunk_bytes > 0
        && chunk_bytes <= chunk_limit
        && accepted_bytes <= total_bytes
        && chunk_bytes <= total_bytes - accepted_bytes
}
#[doc = " Returns the conserved offset after an already-validated chunk."]
#[must_use]
pub const fn advance_chunk_offset(accepted_bytes: u64, chunk_bytes: u64) -> Option<u64> {
    accepted_bytes.checked_add(chunk_bytes)
}
#[doc = " Checks legal artifact completion."]
#[must_use]
pub const fn transfer_complete(
    accepted_bytes: u64,
    total_bytes: u64,
    digest_matches: bool,
    cancelled: bool,
) -> bool {
    !cancelled && accepted_bytes == total_bytes && digest_matches
}
#[doc = " Checks one terminal output record before accepting its bytes."]
#[must_use]
pub const fn terminal_output(
    last_sequence: u64,
    observed_sequence: u64,
    accepted_bytes: u64,
    observed_offset: u64,
    exited: bool,
) -> bool {
    !exited
        && last_sequence < u64::MAX
        && observed_sequence == last_sequence + 1
        && observed_offset == accepted_bytes
}
#[doc = " Checks one final terminal exit record."]
#[must_use]
pub const fn terminal_exit(last_sequence: u64, exit_sequence: u64, already_exited: bool) -> bool {
    !already_exited && last_sequence < u64::MAX && exit_sequence == last_sequence + 1
}
#[doc = " Checks one independently configured resource bound."]
#[must_use]
pub const fn within_bound(observed: usize, maximum: usize) -> bool {
    maximum > 0 && observed <= maximum
}
