//! Exact phase-event construction before diagnostic error mapping.

use crate::state::mutation;
use crate::{SchedulerCommandKind, SchedulerEventKind, SchedulerState};
use vstd::prelude::*;

verus! {

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhaseControlRejection {
    IllegalPause,
    IllegalResume,
    IllegalDrain,
    NotPhaseCommand,
}

impl PhaseControlRejection {
    pub open spec fn outcome(self) -> mutation::PhaseCommandOutcome {
        match self {
            Self::IllegalPause => mutation::PhaseCommandOutcome::IllegalPause,
            Self::IllegalResume => mutation::PhaseCommandOutcome::IllegalResume,
            Self::IllegalDrain => mutation::PhaseCommandOutcome::IllegalDrain,
            Self::NotPhaseCommand => mutation::PhaseCommandOutcome::NotPhaseCommand,
        }
    }
}

pub open spec fn phase_event_matches(
    command: &SchedulerCommandKind,
    event: &SchedulerEventKind,
) -> bool {
    match (command, event) {
        (SchedulerCommandKind::PauseScheduler, SchedulerEventKind::SchedulerPaused)
        | (SchedulerCommandKind::ResumeScheduler, SchedulerEventKind::SchedulerResumed)
        | (
            SchedulerCommandKind::DrainScheduler,
            SchedulerEventKind::SchedulerDrainRequested,
        ) => true,
        _ => false,
    }
}

/// Exact typed control result before stable `SchedulerError` diagnostics are attached.
pub open spec fn phase_control_matches(
    before: &SchedulerState,
    after: &SchedulerState,
    command: &SchedulerCommandKind,
    result: &Result<SchedulerEventKind, PhaseControlRejection>,
) -> bool {
    match result {
        Ok(event) => {
            &&& mutation::phase_command_matches(
                before, after, command, mutation::PhaseCommandOutcome::Applied,
            )
            &&& phase_event_matches(command, event)
        },
        Err(rejection) => mutation::phase_command_matches(
            before, after, command, rejection.outcome(),
        ),
    }
}

pub(super) const fn apply_phase_event(
    state: &mut SchedulerState,
    command: &SchedulerCommandKind,
) -> (result: Result<SchedulerEventKind, PhaseControlRejection>)
    ensures phase_control_matches(old(state), final(state), command, &result),
{
    let outcome = mutation::apply_phase_command(state, command);
    let result = match outcome {
        mutation::PhaseCommandOutcome::Applied => match command {
            SchedulerCommandKind::PauseScheduler => Ok(SchedulerEventKind::SchedulerPaused),
            SchedulerCommandKind::ResumeScheduler => Ok(SchedulerEventKind::SchedulerResumed),
            SchedulerCommandKind::DrainScheduler => {
                Ok(SchedulerEventKind::SchedulerDrainRequested)
            },
            _ => {
                proof {
                    reveal(mutation::phase_command_matches);
                    assert(false);
                }
                Err(PhaseControlRejection::NotPhaseCommand)
            },
        },
        mutation::PhaseCommandOutcome::IllegalPause => {
            Err(PhaseControlRejection::IllegalPause)
        },
        mutation::PhaseCommandOutcome::IllegalResume => {
            Err(PhaseControlRejection::IllegalResume)
        },
        mutation::PhaseCommandOutcome::IllegalDrain => {
            Err(PhaseControlRejection::IllegalDrain)
        },
        mutation::PhaseCommandOutcome::NotPhaseCommand => {
            Err(PhaseControlRejection::NotPhaseCommand)
        },
    };
    proof {
        reveal(phase_control_matches);
        reveal(phase_event_matches);
    }
    result
}

} // verus!
