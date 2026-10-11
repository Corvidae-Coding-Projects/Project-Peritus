//! One exact reservation release during worker-loss application.

use peritus_types::Sha256Digest;

use crate::state::mutation;
use crate::{DispatchId, LossOutcome, SchedulerState, WorkPhase, WorkTerminal};

use super::classify;

#[doc = " Existing worker-loss lookup failure classes preserved by the extracted kernel."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::reducer::apply) enum LossReleaseError {
    ReservationDisappeared,
    WorkDisappeared,
}
#[doc = " Classifies and releases one live reservation from the pre-mutation worker-loss snapshot."]
pub(in crate::reducer::apply) fn release_one(
    state: &mut SchedulerState,
    dispatch_id: DispatchId,
    failure_digest: Sha256Digest,
) -> Result<LossOutcome, LossReleaseError> {
    let Some(reservation) = state.reservation(dispatch_id) else {
        return Err(LossReleaseError::ReservationDisappeared);
    };
    let work_id = reservation.work_id();
    let Some(record) = state.work_item(work_id) else {
        return Err(LossReleaseError::WorkDisappeared);
    };
    let outcome = classify(record, dispatch_id);
    let released = match &outcome {
        LossOutcome::Requeued { .. } => {
            mutation::release_to_phase(state, dispatch_id, work_id, WorkPhase::Queued)
        }
        LossOutcome::Cancelled { .. } => {
            mutation::release_to_terminal(state, dispatch_id, work_id, WorkTerminal::Cancelled)
        }
        LossOutcome::Exhausted { .. } => mutation::release_to_terminal(
            state,
            dispatch_id,
            work_id,
            WorkTerminal::Exhausted { cause_digest: failure_digest },
        ),
        LossOutcome::Ambiguous { .. } => mutation::release_to_terminal(
            state,
            dispatch_id,
            work_id,
            WorkTerminal::Ambiguous { dispatch_id },
        ),
        LossOutcome::Failed { .. } => mutation::release_to_terminal(
            state,
            dispatch_id,
            work_id,
            WorkTerminal::Failed { failure_digest },
        ),
    };
    let Some(_removed) = released else {
        return Err(LossReleaseError::WorkDisappeared);
    };
    Ok(outcome)
}
