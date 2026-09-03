//! Executable refinement predicates for settlement invariants.

use vstd::prelude::*;

verus! {

/// Mathematical acceptance relation for a current candidate.
pub open spec fn acceptance_allowed_spec(
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

/// Executable acceptance relation used by ordinary callers and proofs.
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
) -> (allowed: bool)
    ensures allowed == acceptance_allowed_spec(
        candidate_present,
        gates_current_and_satisfied,
        obligations_current_and_satisfied,
        review_current_and_blocker_free,
    )
{
    candidate_present
        && gates_current_and_satisfied
        && obligations_current_and_satisfied
        && review_current_and_blocker_free
}

/// Acceptance exposes every premise instead of collapsing incomplete evidence into success.
pub proof fn acceptance_implies_all_premises(
    candidate_present: bool,
    gates_current_and_satisfied: bool,
    obligations_current_and_satisfied: bool,
    review_current_and_blocker_free: bool,
)
    requires acceptance_allowed_spec(
        candidate_present,
        gates_current_and_satisfied,
        obligations_current_and_satisfied,
        review_current_and_blocker_free,
    ),
    ensures
        candidate_present,
        gates_current_and_satisfied,
        obligations_current_and_satisfied,
        review_current_and_blocker_free,
{}

/// Mathematical same-candidate checkpoint-advance relation.
pub open spec fn checkpoint_advances_spec(
    previous_sequence: u64,
    next_sequence: u64,
    previous_stage: u8,
    next_stage: u8,
) -> bool {
    previous_sequence < next_sequence && previous_stage <= next_stage
}

/// Checks monotonic checkpoint sequence and stage for one exact candidate.
#[must_use]
pub const fn checkpoint_advances(
    previous_sequence: u64,
    next_sequence: u64,
    previous_stage: u8,
    next_stage: u8,
) -> (advances: bool)
    ensures advances == checkpoint_advances_spec(
        previous_sequence,
        next_sequence,
        previous_stage,
        next_stage,
    )
{
    previous_sequence < next_sequence && previous_stage <= next_stage
}

/// Mathematical evidence-freshness relation.
pub open spec fn evidence_is_current_spec(
    same_candidate: bool,
    evidence_sequence: u64,
    candidate_sequence: u64,
    marked_current: bool,
) -> bool {
    marked_current && same_candidate && evidence_sequence <= candidate_sequence
}

/// Checks that current evidence was produced for this candidate no later than its checkpoint.
#[must_use]
pub const fn evidence_is_current(
    same_candidate: bool,
    evidence_sequence: u64,
    candidate_sequence: u64,
    marked_current: bool,
) -> (current: bool)
    ensures current == evidence_is_current_spec(
        same_candidate,
        evidence_sequence,
        candidate_sequence,
        marked_current,
    )
{
    marked_current && same_candidate && evidence_sequence <= candidate_sequence
}

/// Evidence explicitly marked stale can never satisfy the current-evidence relation.
pub proof fn stale_evidence_cannot_qualify(
    same_candidate: bool,
    evidence_sequence: u64,
    candidate_sequence: u64,
)
    ensures !evidence_is_current_spec(
        same_candidate,
        evidence_sequence,
        candidate_sequence,
        false,
    ),
{}

/// Candidate content and conversation revisions are both part of the freshness binding.
pub open spec fn candidate_binding_spec(
    same_run: bool,
    same_workspace: bool,
    same_digest: bool,
    same_conversation_revision: bool,
) -> bool {
    same_run && same_workspace && same_digest && same_conversation_revision
}

/// Changing either candidate content or conversation revision invalidates the prior binding.
pub proof fn candidate_or_revision_change_breaks_binding(
    same_digest: bool,
    same_conversation_revision: bool,
)
    requires !same_digest || !same_conversation_revision,
    ensures !candidate_binding_spec(true, true, same_digest, same_conversation_revision),
{}

/// Exactly-once settlement permits a terminal transition only from an active reducer.
pub open spec fn terminal_transition_allowed_spec(already_terminal: bool) -> bool {
    !already_terminal
}

/// Executable exactly-once settlement predicate.
#[must_use]
pub const fn terminal_transition_allowed(already_terminal: bool) -> (allowed: bool)
    ensures allowed == terminal_transition_allowed_spec(already_terminal)
{
    !already_terminal
}

/// Candidate availability and strict acceptance are distinct dispositions.
pub proof fn candidate_available_is_not_accepted()
    ensures crate::RunDisposition::CandidateAvailable != crate::RunDisposition::Accepted,
{
}

} // verus!
