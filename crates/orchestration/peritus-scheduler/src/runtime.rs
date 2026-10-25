//! Inert effect directives reconstructed from committed state.

use crate::{DispatchId, SchedulerReservation, SchedulerState, WorkId, WorkPhase, WorkerId};

/// Effect-shell work derived only from already durable scheduler state.
// Direct verification preserves the documented enum fields without relying on synthesized
// projections from the pinned Verus macro.

#[derive(Debug, Eq, PartialEq)]
pub enum SchedulerDirective {
    /// Idempotently deliver an unacknowledged committed dispatch.
    Dispatch(SchedulerReservation),
    /// Ask an owner to terminate one cancelling dispatch.
    Cancel {
        /// Cancelling dispatch identity.
        dispatch_id: DispatchId,
        /// Work whose execution must stop.
        work_id: WorkId,
        /// Worker receiving the cancellation request.
        worker_id: WorkerId,
    },
}

impl SchedulerDirective {}
impl Clone for SchedulerDirective {
    fn clone(&self) -> Self {
        match self {
            Self::Dispatch(reservation) => Self::Dispatch(reservation.clone()),
            Self::Cancel { dispatch_id, work_id, worker_id } => {
                Self::Cancel { dispatch_id: *dispatch_id, work_id: *work_id, worker_id: *worker_id }
            }
        }
    }
}
fn directive_for(
    state: &SchedulerState,
    reservation: &SchedulerReservation,
) -> Option<SchedulerDirective> {
    let record = state.work_item(reservation.work_id())?;
    let phase = record.phase();
    if phase.same(WorkPhase::Reserved) && !reservation.started() {
        let directive = SchedulerDirective::Dispatch(reservation.clone());
        Some(directive)
    } else if phase.same(WorkPhase::Cancelling) {
        let directive = SchedulerDirective::Cancel {
            dispatch_id: reservation.dispatch_id(),
            work_id: reservation.work_id(),
            worker_id: reservation.worker_id(),
        };
        Some(directive)
    } else {
        None
    }
}
#[doc = " Returns bounded pending directives in canonical dispatch order."]
#[must_use]
pub fn pending_directives(state: &SchedulerState) -> Vec<SchedulerDirective> {
    let reservations = state.reservations();
    let batch_size = state.binding().limits().dispatch_batch_size() as usize;
    let mut result = Vec::new();
    let mut index = 0;
    while index < reservations.len() && result.len() < batch_size {
        let reservation = &reservations[index];
        let directive = directive_for(state, reservation);
        if let Some(value) = directive {
            result.push(value);
        }
        index += 1;
    }
    result
}
