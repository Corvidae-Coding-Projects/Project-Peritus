//! Executable counterparts to the D2 mathematical predicates.

use peritus_types::RevisionTuple;

use crate::{ReviewRunPhase, ReviewRunState, ReviewTerminalKind, ReviewTransition};

/// Returns exact equality of every revision component.
#[must_use]
pub fn evidence_is_fresh(observed: RevisionTuple, current: RevisionTuple) -> bool {
    observed == current
}

/// Returns whether every current finding has one permitted current closure.
#[must_use]
pub fn findings_are_conserved(state: &ReviewRunState) -> bool {
    state.unconserved_current_findings().is_empty()
}

/// Returns whether every independently named current quorum dimension passes.
#[must_use]
pub const fn quorum_is_complete(state: &ReviewRunState) -> bool {
    state.quorum().complete()
}

/// Returns the executable no-implicit-success invariant.
#[must_use]
pub fn no_implicit_success(state: &ReviewRunState) -> bool {
    state.terminal().is_none_or(|terminal| {
        terminal.kind() != ReviewTerminalKind::Completed
            || (state.phase() == ReviewRunPhase::Terminal
                && quorum_is_complete(state)
                && findings_are_conserved(state)
                && !state.oscillation().triggered())
    })
}

/// Returns exact complete-state replay equivalence.
#[must_use]
pub fn replay_equivalent(expected: &ReviewRunState, observed: &ReviewRunState) -> bool {
    expected == observed && expected.state_digest() == observed.state_digest()
}

/// Returns whether a transition advances exactly once with all predecessor/successor fences.
#[must_use]
pub fn transition_is_legal(prior: &ReviewRunState, transition: &ReviewTransition) -> bool {
    let event = transition.event();
    let successor = transition.state();
    prior.phase() == ReviewRunPhase::Active
        && event.run_id() == prior.run_id()
        && event.previous_event() == Some(prior.last_event_id())
        && event.prior_state_digest() == prior.state_digest()
        && event.sequence().get() == prior.sequence().get().saturating_add(1)
        && successor.sequence() == event.sequence()
        && successor.last_event_id() == event.id()
        && successor.state_digest() == event.successor_state_digest()
}
