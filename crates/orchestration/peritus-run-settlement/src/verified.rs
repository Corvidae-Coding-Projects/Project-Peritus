//! Executable refinement predicates for settlement invariants.

#[doc = " Executable acceptance relation used by ordinary callers and proofs."]
#[allow(
    clippy::fn_params_excessive_bools,
    reason = "the refinement exposes each independent acceptance premise"
)]
#[must_use]
pub const fn acceptance_allowed(
    candidate_present: bool,
    gates_current_and_satisfied: bool,
    obligations_current_and_satisfied: bool,
    review_current_and_blocker_free: bool,
) -> bool {
    candidate_present
        && gates_current_and_satisfied
        && obligations_current_and_satisfied
        && review_current_and_blocker_free
}
#[doc = " Checks monotonic checkpoint sequence and stage for one exact candidate."]
#[must_use]
pub const fn checkpoint_advances(
    previous_sequence: u64,
    next_sequence: u64,
    previous_stage: u8,
    next_stage: u8,
) -> bool {
    previous_sequence < next_sequence && previous_stage <= next_stage
}
#[doc = " Checks that current evidence was produced for this candidate no later than its checkpoint."]
#[must_use]
pub const fn evidence_is_current(
    same_candidate: bool,
    evidence_sequence: u64,
    candidate_sequence: u64,
    marked_current: bool,
) -> bool {
    marked_current && same_candidate && evidence_sequence <= candidate_sequence
}
#[doc = " Executable exactly-once settlement predicate."]
#[must_use]
pub const fn terminal_transition_allowed(already_terminal: bool) -> bool {
    !already_terminal
}
