//! Canonical aggregate resource sums with exact fallback semantics.

use crate::{ResourceKind, ResourceVector, SchedulerReservation, WorkerId};

pub(super) enum Aggregate {
    Empty,
    Value(ResourceVector),
    Failed,
}
impl Aggregate {
    pub(super) fn quantity(&self, kind: ResourceKind) -> u64 {
        match self {
            Self::Value(vector) => vector.quantity(kind),
            Self::Empty | Self::Failed => 0,
        }
    }
}
fn prepend_global(
    aggregate: Aggregate,
    reservations: &[SchedulerReservation],
    index: usize,
) -> Aggregate {
    let reservation = &reservations[index];
    match aggregate {
        Aggregate::Empty => {
            let vector = reservation.resources().clone();
            Aggregate::Value(vector)
        }
        Aggregate::Value(current) => {
            let Some(vector) = current.aggregate_add(reservation.resources()) else {
                return Aggregate::Failed;
            };
            Aggregate::Value(vector)
        }
        Aggregate::Failed => Aggregate::Failed,
    }
}
fn prepend_worker(
    aggregate: Aggregate,
    reservations: &[SchedulerReservation],
    index: usize,
    worker: WorkerId,
) -> Aggregate {
    let reservation = &reservations[index];
    if !reservation.worker_id().same(&worker) {
        return aggregate;
    }
    match aggregate {
        Aggregate::Empty => {
            let vector = reservation.resources().clone();
            Aggregate::Value(vector)
        }
        Aggregate::Value(current) => {
            let Some(vector) = current.aggregate_add(reservation.resources()) else {
                return Aggregate::Failed;
            };
            Aggregate::Value(vector)
        }
        Aggregate::Failed => Aggregate::Failed,
    }
}
pub(super) fn global(reservations: &[SchedulerReservation]) -> Aggregate {
    let mut result = Aggregate::Empty;
    let mut index = reservations.len();
    while index > 0 {
        index -= 1;
        result = prepend_global(result, reservations, index);
    }
    result
}
pub(super) fn worker(reservations: &[SchedulerReservation], worker_id: WorkerId) -> Aggregate {
    let mut result = Aggregate::Empty;
    let mut index = reservations.len();
    while index > 0 {
        index -= 1;
        result = prepend_worker(result, reservations, index, worker_id);
    }
    result
}
