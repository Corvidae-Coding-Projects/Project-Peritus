//! Executable replay and generation planning proved with Verus.

#[doc = " Returns the unique successor position, refusing numeric wraparound."]
#[must_use]
pub const fn next_position(last: u64) -> Option<u64> {
    last.checked_add(1)
}
#[doc = " Checks the exact global-position transition used by ordinary replay."]
#[must_use]
pub const fn position_transition(last: u64, observed: u64) -> bool {
    last < u64::MAX && observed == last + 1
}
#[doc = " Checks the exact per-aggregate sequence transition used by every fold."]
#[must_use]
pub const fn sequence_transition(last: u64, observed: u64) -> bool {
    last < u64::MAX && observed == last + 1
}
#[doc = " Selects startup reuse only when every checkpoint binding matches."]
#[must_use]
pub const fn checkpoint_current(
    checkpoint_position: u64,
    journal_position: u64,
    head_matches: bool,
    payload_matches: bool,
    schema_matches: bool,
) -> bool {
    checkpoint_position == journal_position && head_matches && payload_matches && schema_matches
}
#[doc = " Plans a fresh monotonically increasing shadow generation."]
#[must_use]
#[allow(
    clippy::manual_unwrap_or_default,
    clippy::option_if_let_else,
    reason = "the explicit match is const-stable and supported by Verus"
)]
pub const fn next_generation(highest: Option<u64>) -> Option<u64> {
    let base = match highest {
        Some(value) => value,
        None => 0,
    };
    base.checked_add(1)
}
