//! Plan-independent persistence for the two D1 lifecycle-only commands.

use peritus_journal::{CommittedBatch, SqliteJournal};

use super::{
    GateReplay, binding_error, commit_gate_transition, inconsistent, load_gate_replay,
    replay_required,
};
use crate::{
    GateCommand, GateCommandKind, GateError, GateEvent, GateEventKind, GateResumePhase,
    GateRunPhase, GateRunState, GateTransition,
};

/// Reconstructs and commits one plan-independent pause or resume transition.
///
/// The complete D1 checkpoint is decoded and checked against its canonical state digest and C0
/// aggregate head. Only [`GateCommandKind::PauseRun`] and [`GateCommandKind::ResumeRun`] are
/// admitted. An exact retry after the lifecycle event committed reconstructs the predecessor and
/// resolves the same C0 command/event without requiring an external gate plan.
///
/// # Errors
/// Rejects any other command kind, absent or corrupt state, stale predecessor fences, illegal
/// lifecycle phase, command identity conflict, or journal integrity failure.
pub fn commit_gate_lifecycle_transition(
    journal: &mut SqliteJournal,
    command: &GateCommand,
) -> Result<CommittedBatch, GateError> {
    if !matches!(command.kind(), GateCommandKind::PauseRun | GateCommandKind::ResumeRun) {
        return Err(binding_error("plan-free gate transition admits only pause or resume"));
    }
    let replay = load_gate_replay(journal, command.run_id())?;
    let state = replay
        .checked_lifecycle_state()?
        .ok_or_else(|| inconsistent("gate lifecycle command names an absent durable aggregate"))?;
    let transition = if command.expected_sequence() == state.sequence().get()
        && command.expected_previous_event() == Some(state.last_event_id())
        && command.prior_state_digest() == state.state_digest()
    {
        crate::reducer::decide_lifecycle(&state, command)?
    } else {
        resolve_lifecycle_retry(&state, replay.events(), command)?
    };
    commit_gate_transition(journal, command, &transition)
}

impl GateReplay {
    fn checked_lifecycle_state(&self) -> Result<Option<GateRunState>, GateError> {
        let Some(checkpoint) = self.checkpoint.clone() else {
            return if self.events.is_empty() {
                Ok(None)
            } else {
                Err(inconsistent("gate events exist without a lifecycle checkpoint"))
            };
        };
        let state = checkpoint.into_state();
        if crate::canonical::state_digest(&state) != state.state_digest() {
            return Err(inconsistent(
                "gate lifecycle checkpoint differs from its canonical state digest",
            ));
        }
        Ok(Some(state))
    }
}

fn resolve_lifecycle_retry(
    state: &GateRunState,
    events: &[GateEvent],
    command: &GateCommand,
) -> Result<GateTransition, GateError> {
    let event =
        events.last().ok_or_else(|| inconsistent("gate lifecycle retry has no durable event"))?;
    let envelope_matches = [
        command.command_id() == event.command_id(),
        command.event_id() == event.id(),
        command.run_id() == event.run_id(),
        command.revision() == event.revision(),
        command.expected_sequence().checked_add(1) == Some(event.sequence().get()),
        command.expected_previous_event() == event.previous_event(),
        command.prior_state_digest() == event.prior_state_digest(),
        event.successor_state_digest() == state.state_digest(),
    ]
    .into_iter()
    .all(core::convert::identity);
    if !envelope_matches {
        return Err(replay_required(
            "gate lifecycle command differs from the current head and exact committed retry",
        ));
    }
    let predecessor_phase = match (command.kind(), event.kind(), state.phase()) {
        (
            GateCommandKind::PauseRun,
            GateEventKind::RunPaused { resume_phase },
            GateRunPhase::Paused(observed),
        ) if resume_phase == &observed => resumed_phase(*resume_phase),
        (GateCommandKind::ResumeRun, GateEventKind::RunResumed { resume_phase }, observed)
            if resumed_phase(*resume_phase) == observed =>
        {
            GateRunPhase::Paused(*resume_phase)
        }
        _ => {
            return Err(inconsistent("gate lifecycle retry event and checkpoint phases differ"));
        }
    };
    let previous_event = command
        .expected_previous_event()
        .ok_or_else(|| binding_error("gate lifecycle retry cannot target a genesis predecessor"))?;
    let sequence = peritus_types::EventSequence::new(command.expected_sequence())
        .map_err(|_| binding_error("gate lifecycle retry predecessor sequence is invalid"))?;
    let mut predecessor = state.clone();
    crate::state::mutation::set_phase(&mut predecessor, predecessor_phase);
    crate::state::mutation::advance(
        &mut predecessor,
        sequence,
        previous_event,
        command.prior_state_digest(),
    );
    if crate::canonical::state_digest(&predecessor) != command.prior_state_digest() {
        return Err(inconsistent(
            "gate lifecycle retry predecessor differs from its canonical digest",
        ));
    }
    let transition = crate::reducer::decide_lifecycle(&predecessor, command)?;
    if transition.event() != event || transition.state() != state {
        return Err(inconsistent("gate lifecycle retry differs from the exact durable transition"));
    }
    Ok(transition)
}

const fn resumed_phase(phase: GateResumePhase) -> GateRunPhase {
    match phase {
        GateResumePhase::Active => GateRunPhase::Active,
        GateResumePhase::Cancelling => GateRunPhase::Cancelling,
    }
}
