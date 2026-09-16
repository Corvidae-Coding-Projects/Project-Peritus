//! Verified production scheduler lifecycle controls.

use vstd::prelude::*;

use crate::{SchedulerCommandKind, SchedulerPhase, SchedulerState};

verus! {

/// Exact deterministic result of applying an actual scheduler phase-control command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhaseCommandOutcome {
    /// The command applied its exact lifecycle transition.
    Applied,
    /// Pause was rejected from the current lifecycle.
    IllegalPause,
    /// Resume was rejected from the current lifecycle.
    IllegalResume,
    /// Drain was rejected from the current lifecycle.
    IllegalDrain,
    /// The supplied actual command was not a scheduler phase control.
    NotPhaseCommand,
}

/// Computes the exact result class for one actual phase-control command.
pub open spec fn phase_command_outcome(
    command: &SchedulerCommandKind,
    phase: SchedulerPhase,
) -> PhaseCommandOutcome {
    match command {
        SchedulerCommandKind::PauseScheduler => {
            if phase == SchedulerPhase::Active || phase == SchedulerPhase::Draining {
                PhaseCommandOutcome::Applied
            } else {
                PhaseCommandOutcome::IllegalPause
            }
        },
        SchedulerCommandKind::ResumeScheduler => {
            if phase == SchedulerPhase::Paused || phase == SchedulerPhase::DrainingPaused {
                PhaseCommandOutcome::Applied
            } else {
                PhaseCommandOutcome::IllegalResume
            }
        },
        SchedulerCommandKind::DrainScheduler => {
            if phase == SchedulerPhase::Active || phase == SchedulerPhase::Paused {
                PhaseCommandOutcome::Applied
            } else {
                PhaseCommandOutcome::IllegalDrain
            }
        },
        _ => PhaseCommandOutcome::NotPhaseCommand,
    }
}

/// Computes the exact lifecycle after one actual phase-control command.
pub open spec fn phase_command_successor(
    command: &SchedulerCommandKind,
    phase: SchedulerPhase,
) -> SchedulerPhase {
    match command {
        SchedulerCommandKind::PauseScheduler => match phase {
            SchedulerPhase::Active => SchedulerPhase::Paused,
            SchedulerPhase::Draining => SchedulerPhase::DrainingPaused,
            _ => phase,
        },
        SchedulerCommandKind::ResumeScheduler => match phase {
            SchedulerPhase::Paused => SchedulerPhase::Active,
            SchedulerPhase::DrainingPaused => SchedulerPhase::Draining,
            _ => phase,
        },
        SchedulerCommandKind::DrainScheduler => match phase {
            SchedulerPhase::Active => SchedulerPhase::Draining,
            SchedulerPhase::Paused => SchedulerPhase::DrainingPaused,
            _ => phase,
        },
        _ => phase,
    }
}

/// Every authoritative field outside scheduler phase is unchanged by phase control.
pub open spec fn phase_update_preserves_other_state(
    before: &SchedulerState,
    after: &SchedulerState,
) -> bool {
    &&& after.spec_sequence() == before.spec_sequence()
    &&& after.spec_last_event_id() == before.spec_last_event_id()
    &&& after.spec_state_digest() == before.spec_state_digest()
    &&& after.spec_binding() == before.spec_binding()
    &&& after.spec_workers() == before.spec_workers()
    &&& after.spec_work() == before.spec_work()
    &&& after.spec_reservations() == before.spec_reservations()
    &&& after.spec_used_dispatches() == before.spec_used_dispatches()
    &&& after.spec_enqueue_ordinal() == before.spec_enqueue_ordinal()
    &&& after.spec_dispatch_ordinal() == before.spec_dispatch_ordinal()
    &&& after.spec_used_commands() == before.spec_used_commands()
    &&& after.spec_terminal() == before.spec_terminal()
}

/// Exact outcome and complete state effect of one production phase command.
pub open spec fn phase_command_matches(
    before: &SchedulerState,
    after: &SchedulerState,
    command: &SchedulerCommandKind,
    outcome: PhaseCommandOutcome,
) -> bool {
    &&& outcome == phase_command_outcome(command, before.spec_phase())
    &&& after.spec_phase() == phase_command_successor(command, before.spec_phase())
    &&& phase_update_preserves_other_state(before, after)
    &&& outcome != PhaseCommandOutcome::Applied ==> *after == *before
}

/// Applies the exact Active/Draining pause transition when admitted.
const fn pause_scheduler(state: &mut SchedulerState) -> (applied: bool)
    ensures
        old(state).spec_reservation_reducer_ready()
            ==> final(state).spec_reservation_reducer_ready(),
        applied == matches!(
            old(state).spec_phase(),
            SchedulerPhase::Active | SchedulerPhase::Draining,
        ),
        applied ==> final(state).spec_phase() == match old(state).spec_phase() {
            SchedulerPhase::Active => SchedulerPhase::Paused,
            SchedulerPhase::Draining => SchedulerPhase::DrainingPaused,
            _ => old(state).spec_phase(),
        },
        !applied ==> *final(state) == *old(state),
        !applied ==> final(state).spec_phase() == old(state).spec_phase(),
        final(state).spec_binding() == old(state).spec_binding(),
        final(state).spec_workers() == old(state).spec_workers(),
        final(state).spec_work() == old(state).spec_work(),
        final(state).spec_reservations() == old(state).spec_reservations(),
        final(state).spec_used_dispatches() == old(state).spec_used_dispatches(),
        phase_update_preserves_other_state(old(state), final(state)),
{
    let phase = match state.phase() {
        SchedulerPhase::Active => SchedulerPhase::Paused,
        SchedulerPhase::Draining => SchedulerPhase::DrainingPaused,
        _ => return false,
    };
    super::set_phase(state, phase);
    true
}

/// Applies the exact Paused/DrainingPaused resume transition when admitted.
const fn resume_scheduler(state: &mut SchedulerState) -> (applied: bool)
    ensures
        old(state).spec_reservation_reducer_ready()
            ==> final(state).spec_reservation_reducer_ready(),
        applied == matches!(
            old(state).spec_phase(),
            SchedulerPhase::Paused | SchedulerPhase::DrainingPaused,
        ),
        applied ==> final(state).spec_phase() == match old(state).spec_phase() {
            SchedulerPhase::Paused => SchedulerPhase::Active,
            SchedulerPhase::DrainingPaused => SchedulerPhase::Draining,
            _ => old(state).spec_phase(),
        },
        !applied ==> *final(state) == *old(state),
        !applied ==> final(state).spec_phase() == old(state).spec_phase(),
        final(state).spec_binding() == old(state).spec_binding(),
        final(state).spec_workers() == old(state).spec_workers(),
        final(state).spec_work() == old(state).spec_work(),
        final(state).spec_reservations() == old(state).spec_reservations(),
        final(state).spec_used_dispatches() == old(state).spec_used_dispatches(),
        phase_update_preserves_other_state(old(state), final(state)),
{
    let phase = match state.phase() {
        SchedulerPhase::Paused => SchedulerPhase::Active,
        SchedulerPhase::DrainingPaused => SchedulerPhase::Draining,
        _ => return false,
    };
    super::set_phase(state, phase);
    true
}

/// Applies the exact Active/Paused drain transition when admitted.
const fn drain_scheduler(state: &mut SchedulerState) -> (applied: bool)
    ensures
        old(state).spec_reservation_reducer_ready()
            ==> final(state).spec_reservation_reducer_ready(),
        applied == matches!(
            old(state).spec_phase(),
            SchedulerPhase::Active | SchedulerPhase::Paused,
        ),
        applied ==> final(state).spec_phase() == match old(state).spec_phase() {
            SchedulerPhase::Active => SchedulerPhase::Draining,
            SchedulerPhase::Paused => SchedulerPhase::DrainingPaused,
            _ => old(state).spec_phase(),
        },
        !applied ==> *final(state) == *old(state),
        !applied ==> final(state).spec_phase() == old(state).spec_phase(),
        final(state).spec_binding() == old(state).spec_binding(),
        final(state).spec_workers() == old(state).spec_workers(),
        final(state).spec_work() == old(state).spec_work(),
        final(state).spec_reservations() == old(state).spec_reservations(),
        final(state).spec_used_dispatches() == old(state).spec_used_dispatches(),
        phase_update_preserves_other_state(old(state), final(state)),
{
    let phase = match state.phase() {
        SchedulerPhase::Active => SchedulerPhase::Draining,
        SchedulerPhase::Paused => SchedulerPhase::DrainingPaused,
        _ => return false,
    };
    super::set_phase(state, phase);
    true
}

/// Applies the actual scheduler phase-control vocabulary through the verified kernel.
pub const fn apply_phase_command(
    state: &mut SchedulerState,
    command: &SchedulerCommandKind,
) -> (outcome: PhaseCommandOutcome)
    ensures
        outcome == phase_command_outcome(command, old(state).spec_phase()),
        final(state).spec_phase()
            == phase_command_successor(command, old(state).spec_phase()),
        old(state).spec_reservation_reducer_ready()
            ==> final(state).spec_reservation_reducer_ready(),
        final(state).spec_binding() == old(state).spec_binding(),
        final(state).spec_workers() == old(state).spec_workers(),
        final(state).spec_work() == old(state).spec_work(),
        final(state).spec_reservations() == old(state).spec_reservations(),
        final(state).spec_used_dispatches() == old(state).spec_used_dispatches(),
        phase_command_matches(old(state), final(state), command, outcome),
{
    let outcome = match command {
        SchedulerCommandKind::PauseScheduler => {
            if pause_scheduler(state) {
                PhaseCommandOutcome::Applied
            } else {
                PhaseCommandOutcome::IllegalPause
            }
        },
        SchedulerCommandKind::ResumeScheduler => {
            if resume_scheduler(state) {
                PhaseCommandOutcome::Applied
            } else {
                PhaseCommandOutcome::IllegalResume
            }
        },
        SchedulerCommandKind::DrainScheduler => {
            if drain_scheduler(state) {
                PhaseCommandOutcome::Applied
            } else {
                PhaseCommandOutcome::IllegalDrain
            }
        },
        _ => PhaseCommandOutcome::NotPhaseCommand,
    };
    proof {
        reveal(phase_command_matches);
        reveal(phase_update_preserves_other_state);
    }
    outcome
}

} // verus!
