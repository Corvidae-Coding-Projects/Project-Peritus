//! Inert effect directives reconstructed from committed state.

use crate::{DispatchId, SchedulerReservation, SchedulerState, WorkId, WorkPhase, WorkerId};

/// Effect-shell work derived only from already durable scheduler state.
#[derive(Clone, Debug, Eq, PartialEq)]
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

/// Returns bounded pending directives in canonical dispatch order.
#[must_use]
pub fn pending_directives(state: &SchedulerState) -> Vec<SchedulerDirective> {
    state
        .reservations()
        .iter()
        .filter_map(|reservation| {
            let phase = state.work_item(reservation.work_id())?.phase();
            match phase {
                WorkPhase::Reserved if !reservation.started() => {
                    Some(SchedulerDirective::Dispatch(reservation.clone()))
                }
                WorkPhase::Cancelling => Some(SchedulerDirective::Cancel {
                    dispatch_id: reservation.dispatch_id(),
                    work_id: reservation.work_id(),
                    worker_id: reservation.worker_id(),
                }),
                _ => None,
            }
        })
        .take(usize::from(state.binding().limits().dispatch_batch_size()))
        .collect()
}
