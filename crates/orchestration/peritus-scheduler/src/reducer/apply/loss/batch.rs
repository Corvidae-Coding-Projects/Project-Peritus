//! Ordered worker-loss release batch and exact lost-worker transition.

use peritus_types::Sha256Digest;

use crate::{DispatchId, SchedulerEventKind, SchedulerState, WorkerId, WorkerPhase};

mod release_plan;

mod worker_target;

use release_plan::release_plan;

use worker_target::validate_worker_target;

#[doc = " Existing worker-loss failure classes retained by the batch boundary."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::reducer::apply) enum WorkerLossError {
    WorkerMissing,
    AlreadyLostOrRemoved,
    ReservationDisappeared,
    WorkDisappeared,
    LostWorkerDisappeared,
}
#[doc = " Releases the exact precomputed dispatch/digest plan, then marks the worker lost."]
#[doc = ""]
#[doc = " Digest values are ordinary inputs. This boundary proves their exact propagation into terminal"]
#[doc = " payloads without making a claim about how the caller computed them."]
pub(in crate::reducer::apply) fn apply_worker_loss(
    state: &mut SchedulerState,
    worker_id: WorkerId,
    plan: &[(DispatchId, Sha256Digest)],
) -> Result<SchedulerEventKind, WorkerLossError> {
    validate_worker_target(state, worker_id)?;
    let outcomes = match release_plan(state, plan) {
        Ok(outcomes) => outcomes,
        Err(error) => {
            return Err(error);
        }
    };
    let marked = crate::state::mutation::set_worker_phase(state, worker_id, WorkerPhase::Lost);
    if !marked {
        return Err(WorkerLossError::LostWorkerDisappeared);
    }
    let event = SchedulerEventKind::WorkerLost { worker_id, outcomes };
    Ok(event)
}
