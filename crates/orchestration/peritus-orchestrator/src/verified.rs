//! Executable E0 invariants and focused Verus proof roots.

use crate::{
    ChildObservation, KernelAcceptanceOutcome, OrchestratorPhase, OrchestratorState,
    OrchestratorTerminalKind, OrchestratorTransition, TerminalCause,
};

/// Returns whether the service, mutating agents, and reviewers remain pairwise separated.
#[must_use]
pub fn roles_are_separated(state: &OrchestratorState) -> bool {
    let ownership = state.ownership();
    let service = ownership.service_actor();
    let writer = ownership.writer().actor();
    let fixer = ownership.fixer().actor();
    service != writer
        && service != fixer
        && writer != fixer
        && !ownership.reviewers().is_empty()
        && ownership.reviewers().windows(2).all(|pair| pair[0].actor() < pair[1].actor())
        && ownership.reviewers().iter().all(|reviewer| {
            reviewer.actor() != service && reviewer.actor() != writer && reviewer.actor() != fixer
        })
}

/// Returns whether candidate and quality-cycle history advance together without identity reuse.
#[must_use]
pub fn candidate_cycles_are_fresh(state: &OrchestratorState) -> bool {
    let candidates = state.candidate_history();
    let cycles = state.quality_cycle_history();
    candidates.len() == cycles.len()
        && candidates
            .iter()
            .zip(cycles)
            .all(|(candidate, cycle)| candidate.revision() == cycle.revision())
        && candidates.windows(2).all(|pair| {
            crate::state::revision_successor(pair[0].revision(), pair[1].revision())
                && !pair[0].materially_equal(&pair[1])
        })
        && candidates.iter().enumerate().all(|(index, candidate)| {
            candidates[..index].iter().all(|prior| {
                !prior.reuses_material(candidate) && prior.digest() != candidate.digest()
            })
        })
        && cycles.iter().enumerate().all(|(index, cycle)| {
            cycles[..index].iter().all(|prior| {
                prior.gate_run_id() != cycle.gate_run_id()
                    && prior.scheduler_run_id() != cycle.scheduler_run_id()
                    && prior.collaboration_run_id() != cycle.collaboration_run_id()
                    && prior.scheduler_id() != cycle.scheduler_id()
                    && prior.collaboration_id() != cycle.collaboration_id()
                    && prior.digest() != cycle.digest()
            })
        })
}

/// Returns whether every retained counter is within its independent immutable ceiling.
#[must_use]
pub fn counters_are_bounded(state: &OrchestratorState) -> bool {
    let counters = state.counters();
    let binding = state.binding();
    let limits = binding.limits();
    [
        counters.revisions() > 0,
        counters.revisions() <= limits.revisions(),
        counters.writer_cycles() <= limits.writer_cycles(),
        counters.fixer_cycles() <= limits.fixer_cycles(),
        counters.gate_cycles() <= binding.effective_gate_cycles(),
        counters.review_cycles() <= binding.effective_review_cycles(),
        counters.handoffs() <= limits.handoffs(),
        counters.child_directives() <= limits.child_directives(),
        counters.retained_observations() <= limits.retained_observations(),
        counters.cancellation_reconciliations() <= limits.cancellation_reconciliations(),
    ]
    .into_iter()
    .all(|bounded| bounded)
}

/// Returns whether a terminal state is quiescent and acceptance came from the planned B0 event.
#[must_use]
pub fn terminal_is_truthful(state: &OrchestratorState) -> bool {
    let Some(terminal) = state.terminal().copied() else {
        return state.phase() != OrchestratorPhase::Terminal;
    };
    if state.phase() != OrchestratorPhase::Terminal
        || terminal.revision() != state.current_candidate().revision()
        || !state.active_children().is_empty()
        || state.pending_directive().is_some()
    {
        return false;
    }
    if terminal.kind() != OrchestratorTerminalKind::Accepted {
        return true;
    }
    let Some(certificate) = state.acceptance_certificate() else {
        return false;
    };
    let plan = certificate.kernel_plan();
    let begun = state.children().iter().any(|child| {
        matches!(
            child,
            ChildObservation::KernelAcceptance(observation)
                if observation.outcome() == KernelAcceptanceOutcome::Begun
                    && observation.run_id() == state.binding().run_id()
                    && observation.revision() == certificate.revision()
                    && observation.command_id() == plan.begin_command_id()
                    && observation.event_id() == plan.begin_event_id()
                    && observation.previous_event_id() == plan.expected_previous_kernel_event()
        )
    });
    let accepted = state.children().iter().any(|child| {
        matches!(
            child,
            ChildObservation::KernelAcceptance(observation)
                if observation.outcome() == KernelAcceptanceOutcome::Accepted
                    && observation.run_id() == state.binding().run_id()
                    && observation.revision() == certificate.revision()
                    && observation.command_id() == plan.evaluate_command_id()
                    && observation.event_id() == plan.evaluate_event_id()
                    && observation.previous_event_id() == Some(plan.evaluate_previous_event_id())
        )
    });
    terminal.cause() == TerminalCause::KernelAccepted
        && terminal.cause_digest() == certificate.digest()
        && begun
        && accepted
}

/// Returns whether cancellation/settlement state can never itself imply acceptance.
#[must_use]
pub fn cancellation_dominates(state: &OrchestratorState) -> bool {
    if state.phase() == OrchestratorPhase::Cancelling {
        state.terminal().is_none()
            && state
                .pending_terminal()
                .is_none_or(|terminal| terminal.kind() != OrchestratorTerminalKind::Accepted)
    } else {
        state.terminal().is_none_or(|terminal| {
            terminal.kind() != OrchestratorTerminalKind::Cancelled
                || terminal.cause() == TerminalCause::CancellationReconciled
        })
    }
}

/// Returns exact complete-state replay equivalence.
#[must_use]
pub fn replay_equivalent(expected: &OrchestratorState, observed: &OrchestratorState) -> bool {
    expected == observed && expected.state_digest() == observed.state_digest()
}

/// Returns whether one accepted transition carries every sequence and digest fence exactly once.
#[must_use]
pub fn transition_is_legal(prior: &OrchestratorState, transition: &OrchestratorTransition) -> bool {
    let event = transition.event();
    let successor = transition.state();
    prior.phase() != OrchestratorPhase::Terminal
        && event.run_id() == prior.binding().run_id()
        && event.previous_event() == Some(prior.last_event_id())
        && event.prior_state_digest() == prior.state_digest()
        && event.sequence().get() == prior.sequence().get().saturating_add(1)
        && successor.sequence() == event.sequence()
        && successor.last_event_id() == event.id()
        && successor.state_digest() == event.successor_state_digest()
        && successor.used_commands().last() == Some(&event.command_id())
}
