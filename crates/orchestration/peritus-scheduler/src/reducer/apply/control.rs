//! Cancellation, pause/drain, abandonment, exhaustion, and terminal control.

use crate::state::mutation;
use crate::{
    DispatchId, SchedulerError, SchedulerErrorKind, SchedulerEventKind, SchedulerPhase,
    SchedulerState, SchedulerTerminal, WorkId, WorkPhase, WorkTerminal,
};

pub(super) fn cancel(
    state: &mut SchedulerState,
    work_id: WorkId,
    descendants: bool,
) -> Result<SchedulerEventKind, SchedulerError> {
    let root = state.work_item(work_id).ok_or_else(|| unknown("work is not retained"))?;
    if root.phase() == WorkPhase::Terminal {
        return Err(crate::reducer::illegal("work is already terminal"));
    }
    let mut affected = vec![work_id];
    if descendants {
        loop {
            let before = affected.len();
            for record in state.work() {
                if record
                    .spec()
                    .parent()
                    .is_some_and(|parent| affected.binary_search(&parent).is_ok())
                    && affected.binary_search(&record.spec().id()).is_err()
                {
                    affected.push(record.spec().id());
                    affected.sort_unstable();
                }
            }
            if affected.len() == before {
                break;
            }
        }
    }
    affected.retain(|id| {
        state.work_item(*id).is_some_and(|record| record.phase() != WorkPhase::Terminal)
    });
    for id in &affected {
        let active = state.reservations().iter().any(|reservation| reservation.work_id() == *id);
        let record = mutation::work_mut(state, *id)
            .ok_or_else(|| unknown("affected cancellation work disappeared"))?;
        if active {
            record.set_phase(WorkPhase::Cancelling);
        } else {
            record.terminalize(WorkTerminal::Cancelled);
        }
    }
    Ok(SchedulerEventKind::WorkCancelled { work_id, descendants, affected })
}

pub(super) fn acknowledge_cancel(
    state: &mut SchedulerState,
    dispatch_id: DispatchId,
) -> Result<SchedulerEventKind, SchedulerError> {
    let work_id =
        state.reservation(dispatch_id).ok_or_else(|| unknown("dispatch is not active"))?.work_id();
    let phase =
        state.work_item(work_id).ok_or_else(|| unknown("reservation work disappeared"))?.phase();
    if phase != WorkPhase::Cancelling {
        return Err(crate::reducer::illegal("dispatch work is not cancelling"));
    }
    mutation::remove_reservation(state, dispatch_id)
        .ok_or_else(|| unknown("cancelling reservation disappeared"))?;
    mutation::work_mut(state, work_id)
        .ok_or_else(|| unknown("cancelling work disappeared"))?
        .terminalize(WorkTerminal::Cancelled);
    Ok(SchedulerEventKind::CancellationAcknowledged { dispatch_id })
}

pub(super) fn exhaust(
    state: &mut SchedulerState,
    work_id: WorkId,
    cause: peritus_types::Sha256Digest,
) -> Result<SchedulerEventKind, SchedulerError> {
    let work = mutation::work_mut(state, work_id).ok_or_else(|| unknown("work is not retained"))?;
    if matches!(
        work.phase(),
        WorkPhase::Reserved | WorkPhase::Running | WorkPhase::Cancelling | WorkPhase::Terminal
    ) {
        return Err(crate::reducer::illegal(
            "active or terminal work cannot be explicitly exhausted",
        ));
    }
    work.terminalize(WorkTerminal::Exhausted { cause_digest: cause });
    Ok(SchedulerEventKind::WorkExhausted { work_id, cause_digest: cause })
}

pub(super) fn abandon(
    state: &mut SchedulerState,
    dispatch_id: DispatchId,
    cause: peritus_types::Sha256Digest,
) -> Result<SchedulerEventKind, SchedulerError> {
    let work_id =
        state.reservation(dispatch_id).ok_or_else(|| unknown("dispatch is not active"))?.work_id();
    mutation::remove_reservation(state, dispatch_id)
        .ok_or_else(|| unknown("abandoned reservation disappeared"))?;
    let work =
        mutation::work_mut(state, work_id).ok_or_else(|| unknown("abandoned work disappeared"))?;
    if work.phase() == WorkPhase::Cancelling {
        work.terminalize(WorkTerminal::Cancelled);
    } else {
        work.terminalize(WorkTerminal::Abandoned { cause_digest: cause });
    }
    Ok(SchedulerEventKind::DispatchAbandoned { dispatch_id, cause_digest: cause })
}

pub(super) fn pause(state: &mut SchedulerState) -> Result<SchedulerEventKind, SchedulerError> {
    let phase = match state.phase() {
        SchedulerPhase::Active => SchedulerPhase::Paused,
        SchedulerPhase::Draining => SchedulerPhase::DrainingPaused,
        _ => return Err(crate::reducer::illegal("scheduler is already paused or terminal")),
    };
    mutation::set_phase(state, phase);
    Ok(SchedulerEventKind::SchedulerPaused)
}

pub(super) fn resume(state: &mut SchedulerState) -> Result<SchedulerEventKind, SchedulerError> {
    let phase = match state.phase() {
        SchedulerPhase::Paused => SchedulerPhase::Active,
        SchedulerPhase::DrainingPaused => SchedulerPhase::Draining,
        _ => return Err(crate::reducer::illegal("scheduler is not paused")),
    };
    mutation::set_phase(state, phase);
    Ok(SchedulerEventKind::SchedulerResumed)
}

pub(super) fn drain_scheduler(
    state: &mut SchedulerState,
) -> Result<SchedulerEventKind, SchedulerError> {
    let phase = match state.phase() {
        SchedulerPhase::Active => SchedulerPhase::Draining,
        SchedulerPhase::Paused => SchedulerPhase::DrainingPaused,
        _ => return Err(crate::reducer::illegal("scheduler is already draining")),
    };
    mutation::set_phase(state, phase);
    Ok(SchedulerEventKind::SchedulerDrainRequested)
}

pub(super) fn finalize(state: &mut SchedulerState) -> Result<SchedulerEventKind, SchedulerError> {
    if !state.all_work_terminal() || !state.reservations().is_empty() {
        return Err(crate::reducer::illegal(
            "scheduler cannot finalize with nonterminal work or directives",
        ));
    }
    let terminal = SchedulerTerminal::evaluate(state.work());
    mutation::set_terminal(state, terminal.clone());
    Ok(SchedulerEventKind::SchedulerFinalized { terminal })
}

fn unknown(detail: &'static str) -> SchedulerError {
    crate::error::reject(SchedulerErrorKind::UnknownIdentity, detail)
}
