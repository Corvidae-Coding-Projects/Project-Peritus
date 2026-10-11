//! Finite execution of the exact precomputed worker-loss release plan.

use peritus_types::Sha256Digest;

use crate::{DispatchId, LossOutcome, SchedulerState};

use super::super::release_one::{LossReleaseError, release_one};

use super::WorkerLossError;

pub(super) fn release_plan(
    state: &mut SchedulerState,
    plan: &[(DispatchId, Sha256Digest)],
) -> Result<Vec<LossOutcome>, WorkerLossError> {
    let mut outcomes = Vec::with_capacity(plan.len());
    let mut index = 0;
    while index < plan.len() {
        let (dispatch_id, failure_digest) = plan[index];
        let outcome = match release_one(state, dispatch_id, failure_digest) {
            Ok(outcome) => outcome,
            Err(error) => {
                return Err(match error {
                    LossReleaseError::ReservationDisappeared => {
                        WorkerLossError::ReservationDisappeared
                    }
                    LossReleaseError::WorkDisappeared => WorkerLossError::WorkDisappeared,
                });
            }
        };
        outcomes.push(outcome);
        index += 1;
    }
    Ok(outcomes)
}
