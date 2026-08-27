//! Executable A3 safety predicates paired with Verus specifications.

use vstd::prelude::*;

verus! {

/// Mathematical support relation for one application-protocol version candidate.
pub open spec fn version_supported_spec(
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

/// Checks one candidate against one supported inclusive range.
#[must_use]
pub const fn version_supported(
    major: u16,
    minor: u16,
    supported_major: u16,
    minimum_minor: u16,
    maximum_minor: u16,
) -> (supported: bool)
    ensures supported == version_supported_spec(
        major, minor, supported_major, minimum_minor, maximum_minor,
    )
{
    major > 0
        && major == supported_major
        && minimum_minor <= maximum_minor
        && minimum_minor <= minor
        && minor <= maximum_minor
}

/// Mathematical successful-negotiation projection for `INV-023`.
pub open spec fn negotiation_safe_spec(
    mutually_supported: bool,
    required_features_present: bool,
    limits_nonzero: bool,
    selected_session: bool,
) -> bool {
    mutually_supported && required_features_present && limits_nonzero && selected_session
}

/// Checks all premises required for a successful negotiated session.
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
) -> (safe: bool)
    ensures safe == negotiation_safe_spec(
        mutually_supported, required_features_present, limits_nonzero, selected_session,
    )
{
    mutually_supported && required_features_present && limits_nonzero && selected_session
}

/// Returns the unique successor cursor without numeric wraparound.
#[must_use]
pub const fn next_cursor(last: u64) -> (next: Option<u64>)
    ensures
        next.is_some() ==> next.unwrap() == last + 1,
        next.is_none() ==> last == u64::MAX,
{
    last.checked_add(1)
}

/// Mathematical source-position delivery relation for `INV-024`.
pub open spec fn delivery_advances_spec(scanned: u64, observed: u64) -> bool {
    scanned < observed
}

/// Checks that one distinct delivery strictly advances the scanned source position.
#[must_use]
pub const fn delivery_advances(scanned: u64, observed: u64) -> (valid: bool)
    ensures valid == delivery_advances_spec(scanned, observed)
{
    scanned < observed
}

/// Mathematical cumulative-acknowledgement legality relation.
pub open spec fn ack_legal_spec(
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

/// Checks that a cumulative acknowledgement closes an actually delivered prefix.
#[must_use]
pub const fn ack_legal(
    last_acknowledged: u64,
    last_delivered: u64,
    acknowledged: u64,
    gap_open: bool,
    delivered_member: bool,
) -> (valid: bool)
    ensures valid == ack_legal_spec(
        last_acknowledged, last_delivered, acknowledged, gap_open, delivered_member,
    )
{
    !gap_open
        && last_acknowledged <= acknowledged
        && acknowledged <= last_delivered
        && (acknowledged == last_acknowledged || delivered_member)
}

/// Redelivery is legal only when every stable event-identity dimension matches.
pub open spec fn redelivery_identity_spec(
    same_cursor: bool,
    same_event: bool,
    same_frame_digest: bool,
    same_subscription: bool,
    attempt_advances: bool,
) -> bool {
    same_cursor && same_event && same_frame_digest && same_subscription && attempt_advances
}

/// Checks the exact at-least-once redelivery identity relation.
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
) -> (valid: bool)
    ensures valid == redelivery_identity_spec(
        same_cursor, same_event, same_frame_digest, same_subscription, attempt_advances,
    )
{
    same_cursor && same_event && same_frame_digest && same_subscription && attempt_advances
}

/// Mathematical contiguous artifact-chunk relation for `INV-025`.
pub open spec fn chunk_accepted_spec(
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

/// Checks one chunk without permitting overlap, gaps, limit excess, or total-size overflow.
#[must_use]
pub const fn chunk_accepted(
    accepted_bytes: u64,
    offset: u64,
    chunk_bytes: u64,
    total_bytes: u64,
    chunk_limit: u64,
) -> (valid: bool)
    ensures valid == chunk_accepted_spec(
        accepted_bytes, offset, chunk_bytes, total_bytes, chunk_limit,
    )
{
    offset == accepted_bytes
        && chunk_bytes > 0
        && chunk_bytes <= chunk_limit
        && accepted_bytes <= total_bytes
        && chunk_bytes <= total_bytes - accepted_bytes
}

/// Returns the conserved offset after an already-validated chunk.
#[must_use]
pub const fn advance_chunk_offset(accepted_bytes: u64, chunk_bytes: u64) -> (next: Option<u64>)
    ensures next.is_some() ==> next.unwrap() == accepted_bytes + chunk_bytes,
{
    accepted_bytes.checked_add(chunk_bytes)
}

/// Completion requires exact conserved size and the observed digest.
pub open spec fn transfer_complete_spec(
    accepted_bytes: u64,
    total_bytes: u64,
    digest_matches: bool,
    cancelled: bool,
) -> bool {
    !cancelled && accepted_bytes == total_bytes && digest_matches
}

/// Checks legal artifact completion.
#[must_use]
pub const fn transfer_complete(
    accepted_bytes: u64,
    total_bytes: u64,
    digest_matches: bool,
    cancelled: bool,
) -> (valid: bool)
    ensures valid == transfer_complete_spec(
        accepted_bytes, total_bytes, digest_matches, cancelled,
    )
{
    !cancelled && accepted_bytes == total_bytes && digest_matches
}

/// Mathematical terminal-output ordering relation for `INV-026`.
pub open spec fn terminal_output_spec(
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

/// Checks one terminal output record before accepting its bytes.
#[must_use]
pub const fn terminal_output(
    last_sequence: u64,
    observed_sequence: u64,
    accepted_bytes: u64,
    observed_offset: u64,
    exited: bool,
) -> (valid: bool)
    ensures valid == terminal_output_spec(
        last_sequence, observed_sequence, accepted_bytes, observed_offset, exited,
    )
{
    !exited
        && last_sequence < u64::MAX
        && observed_sequence == last_sequence + 1
        && observed_offset == accepted_bytes
}

/// A terminal exit advances sequence once and can occur only once.
pub open spec fn terminal_exit_spec(
    last_sequence: u64,
    exit_sequence: u64,
    already_exited: bool,
) -> bool {
    !already_exited && last_sequence < u64::MAX && exit_sequence == last_sequence + 1
}

/// Checks one final terminal exit record.
#[must_use]
pub const fn terminal_exit(
    last_sequence: u64,
    exit_sequence: u64,
    already_exited: bool,
) -> (valid: bool)
    ensures valid == terminal_exit_spec(last_sequence, exit_sequence, already_exited)
{
    !already_exited && last_sequence < u64::MAX && exit_sequence == last_sequence + 1
}

/// Mathematical independent bound relation for `INV-027`.
pub open spec fn within_bound_spec(observed: usize, maximum: usize) -> bool {
    maximum > 0 && observed <= maximum
}

/// Checks one independently configured resource bound.
#[must_use]
pub const fn within_bound(observed: usize, maximum: usize) -> (valid: bool)
    ensures valid == within_bound_spec(observed, maximum)
{
    maximum > 0 && observed <= maximum
}

/// Proves that a legal cumulative acknowledgement never exceeds delivery.
pub proof fn legal_ack_never_exceeds_delivery(
    last_acknowledged: u64,
    last_delivered: u64,
    acknowledged: u64,
    gap_open: bool,
    delivered_member: bool,
)
    requires ack_legal_spec(
        last_acknowledged,
        last_delivered,
        acknowledged,
        gap_open,
        delivered_member,
    )
    ensures acknowledged <= last_delivered
{
}

/// Proves that a legal accepted chunk fits within the declared remaining size.
pub proof fn accepted_chunk_fits(
    accepted_bytes: u64,
    offset: u64,
    chunk_bytes: u64,
    total_bytes: u64,
    chunk_limit: u64,
)
    requires chunk_accepted_spec(
        accepted_bytes, offset, chunk_bytes, total_bytes, chunk_limit,
    )
    ensures accepted_bytes + chunk_bytes <= total_bytes
{
}

} // verus!
