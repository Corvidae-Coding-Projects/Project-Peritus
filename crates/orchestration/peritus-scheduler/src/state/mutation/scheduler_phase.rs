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
        !applied ==> final(state).spec_phase() == old(state).spec_phase(),
        final(state).spec_binding() == old(state).spec_binding(),
        final(state).spec_workers() == old(state).spec_workers(),
        final(state).spec_work() == old(state).spec_work(),
        final(state).spec_reservations() == old(state).spec_reservations(),
        final(state).spec_used_dispatches() == old(state).spec_used_dispatches(),
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
        !applied ==> final(state).spec_phase() == old(state).spec_phase(),
        final(state).spec_binding() == old(state).spec_binding(),
        final(state).spec_workers() == old(state).spec_workers(),
        final(state).spec_work() == old(state).spec_work(),
        final(state).spec_reservations() == old(state).spec_reservations(),
        final(state).spec_used_dispatches() == old(state).spec_used_dispatches(),
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
        !applied ==> final(state).spec_phase() == old(state).spec_phase(),
        final(state).spec_binding() == old(state).spec_binding(),
        final(state).spec_workers() == old(state).spec_workers(),
        final(state).spec_work() == old(state).spec_work(),
        final(state).spec_reservations() == old(state).spec_reservations(),
        final(state).spec_used_dispatches() == old(state).spec_used_dispatches(),
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
{
    match command {
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
    }
}

} // verus!
