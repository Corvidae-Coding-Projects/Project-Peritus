//! Exact phase-event construction before diagnostic error mapping.

use crate::state::mutation;
use crate::{SchedulerCommandKind, SchedulerEventKind, SchedulerState};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhaseControlRejection {
    IllegalPause,
    IllegalResume,
    IllegalDrain,
    NotPhaseCommand,
}
impl PhaseControlRejection {}
pub(super) const fn apply_phase_event(
    state: &mut SchedulerState,
    command: &SchedulerCommandKind,
) -> Result<SchedulerEventKind, PhaseControlRejection> {
    let outcome = mutation::apply_phase_command(state, command);

    match outcome {
        mutation::PhaseCommandOutcome::Applied => match command {
            SchedulerCommandKind::PauseScheduler => Ok(SchedulerEventKind::SchedulerPaused),
            SchedulerCommandKind::ResumeScheduler => Ok(SchedulerEventKind::SchedulerResumed),
            SchedulerCommandKind::DrainScheduler => Ok(SchedulerEventKind::SchedulerDrainRequested),
            _ => Err(PhaseControlRejection::NotPhaseCommand),
        },
        mutation::PhaseCommandOutcome::IllegalPause => Err(PhaseControlRejection::IllegalPause),
        mutation::PhaseCommandOutcome::IllegalResume => Err(PhaseControlRejection::IllegalResume),
        mutation::PhaseCommandOutcome::IllegalDrain => Err(PhaseControlRejection::IllegalDrain),
        mutation::PhaseCommandOutcome::NotPhaseCommand => {
            Err(PhaseControlRejection::NotPhaseCommand)
        }
    }
}
