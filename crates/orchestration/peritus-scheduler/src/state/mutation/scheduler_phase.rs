//! Verified production scheduler lifecycle controls.

use crate::{SchedulerCommandKind, SchedulerPhase, SchedulerState};

#[doc = " Exact deterministic result of applying an actual scheduler phase-control command."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhaseCommandOutcome {
    #[doc = " The command applied its exact lifecycle transition."]
    Applied,
    #[doc = " Pause was rejected from the current lifecycle."]
    IllegalPause,
    #[doc = " Resume was rejected from the current lifecycle."]
    IllegalResume,
    #[doc = " Drain was rejected from the current lifecycle."]
    IllegalDrain,
    #[doc = " The supplied actual command was not a scheduler phase control."]
    NotPhaseCommand,
}
#[doc = " Applies the exact Active/Draining pause transition when admitted."]
const fn pause_scheduler(state: &mut SchedulerState) -> bool {
    let phase = match state.phase() {
        SchedulerPhase::Active => SchedulerPhase::Paused,
        SchedulerPhase::Draining => SchedulerPhase::DrainingPaused,
        _ => return false,
    };
    super::set_phase(state, phase);
    true
}
#[doc = " Applies the exact Paused/DrainingPaused resume transition when admitted."]
const fn resume_scheduler(state: &mut SchedulerState) -> bool {
    let phase = match state.phase() {
        SchedulerPhase::Paused => SchedulerPhase::Active,
        SchedulerPhase::DrainingPaused => SchedulerPhase::Draining,
        _ => return false,
    };
    super::set_phase(state, phase);
    true
}
#[doc = " Applies the exact Active/Paused drain transition when admitted."]
const fn drain_scheduler(state: &mut SchedulerState) -> bool {
    let phase = match state.phase() {
        SchedulerPhase::Active => SchedulerPhase::Draining,
        SchedulerPhase::Paused => SchedulerPhase::DrainingPaused,
        _ => return false,
    };
    super::set_phase(state, phase);
    true
}
#[doc = " Applies the actual scheduler phase-control vocabulary through the verified kernel."]
pub const fn apply_phase_command(
    state: &mut SchedulerState,
    command: &SchedulerCommandKind,
) -> PhaseCommandOutcome {
    match command {
        SchedulerCommandKind::PauseScheduler => {
            if pause_scheduler(state) {
                PhaseCommandOutcome::Applied
            } else {
                PhaseCommandOutcome::IllegalPause
            }
        }
        SchedulerCommandKind::ResumeScheduler => {
            if resume_scheduler(state) {
                PhaseCommandOutcome::Applied
            } else {
                PhaseCommandOutcome::IllegalResume
            }
        }
        SchedulerCommandKind::DrainScheduler => {
            if drain_scheduler(state) {
                PhaseCommandOutcome::Applied
            } else {
                PhaseCommandOutcome::IllegalDrain
            }
        }
        _ => PhaseCommandOutcome::NotPhaseCommand,
    }
}
