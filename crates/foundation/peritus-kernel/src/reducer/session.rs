//! Session transitions.

use super::AppliedCommand;
use crate::{
    KernelAggregate, KernelCommand, KernelError, KernelErrorKind, KernelEventKind, KernelSubject,
    LifecycleEntity, RunPhase, SessionPhase,
};
use vstd::prelude::*;

verus! {

pub(super) fn apply(
    state: &mut KernelAggregate,
    command: &KernelCommand,
) -> Result<AppliedCommand, KernelError> {
    let subject = KernelSubject::Session(state.session.id());
    match command {
        KernelCommand::PauseSession => {
            if state.session.phase() != SessionPhase::Open || has_unpaused_run(state) {
                return Err(KernelError::entity(KernelErrorKind::IllegalPhase, LifecycleEntity::Session));
            }
            state.session.set_phase(SessionPhase::Paused);
            Ok(AppliedCommand::new(KernelEventKind::SessionPaused, subject))
        }
        KernelCommand::ResumeSession => {
            if state.session.phase() != SessionPhase::Paused {
                return Err(KernelError::entity(KernelErrorKind::IllegalPhase, LifecycleEntity::Session));
            }
            state.session.set_phase(SessionPhase::Open);
            Ok(AppliedCommand::new(KernelEventKind::SessionResumed, subject))
        }
        KernelCommand::CloseSession => {
            if state.session.phase() == SessionPhase::Closed {
                return Err(KernelError::entity(KernelErrorKind::IllegalPhase, LifecycleEntity::Session));
            }
            if state.has_live_run() {
                return Err(KernelError::entity(KernelErrorKind::LiveChild, LifecycleEntity::Run));
            }
            state.session.set_phase(SessionPhase::Closed);
            Ok(AppliedCommand::new(KernelEventKind::SessionClosed, subject))
        }
        _ => Err(KernelError::entity(KernelErrorKind::IllegalPhase, LifecycleEntity::Session)),
    }
}

fn has_unpaused_run(state: &KernelAggregate) -> bool {
    let mut index = 0;
    while index < state.runs.len()
        invariant index <= state.runs.len(),
        decreases state.runs.len() - index,
    {
        let phase = state.runs[index].phase();
        if !phase.is_terminal() && phase != RunPhase::Paused { return true; }
        index += 1;
    }
    false
}

} // verus!
