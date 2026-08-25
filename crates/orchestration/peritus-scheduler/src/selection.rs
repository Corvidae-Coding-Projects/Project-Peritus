//! Pure deterministic bounded-bypass selection.

use core::cmp::Ordering;

use crate::{ResourceVector, SchedulerState, WorkId, WorkPhase, WorkerId, WorkerPhase};

/// One deterministic feasible work/worker choice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Selection {
    work_id: WorkId,
    worker_id: WorkerId,
}

impl Selection {
    /// Returns selected work.
    #[must_use]
    pub const fn work_id(self) -> WorkId {
        self.work_id
    }
    /// Returns selected worker.
    #[must_use]
    pub const fn worker_id(self) -> WorkerId {
        self.worker_id
    }
}

/// Selects the next feasible item by aged-first, priority, enqueue ordinal, identity, then worker.
#[must_use]
pub fn select_next(state: &SchedulerState) -> Option<Selection> {
    let mut feasible = state
        .work()
        .iter()
        .filter_map(|work| {
            if work.phase() != WorkPhase::Queued {
                return None;
            }
            let worker = first_feasible_worker(state, work.spec().id())?;
            Some((work, worker))
        })
        .collect::<Vec<_>>();
    feasible.sort_by(|(left, left_worker), (right, right_worker)| {
        compare_work(state, left, right).then_with(|| left_worker.cmp(right_worker))
    });
    feasible
        .first()
        .map(|(work, worker)| Selection { work_id: work.spec().id(), worker_id: *worker })
}

/// Returns whether an item is feasible under current global and at least one worker capacity.
#[must_use]
pub fn is_feasible(state: &SchedulerState, work_id: WorkId) -> bool {
    first_feasible_worker(state, work_id).is_some()
}

fn compare_work(
    state: &SchedulerState,
    left: &crate::WorkRecord,
    right: &crate::WorkRecord,
) -> Ordering {
    let limit = state.binding().limits().bypass_count();
    let left_aged = left.bypasses() >= limit;
    let right_aged = right.bypasses() >= limit;
    right_aged
        .cmp(&left_aged)
        .then_with(|| right.spec().priority().cmp(&left.spec().priority()))
        .then_with(|| left.enqueue_ordinal().cmp(&right.enqueue_ordinal()))
        .then_with(|| left.spec().id().cmp(&right.spec().id()))
}

fn first_feasible_worker(state: &SchedulerState, work_id: WorkId) -> Option<WorkerId> {
    let work = state.work_item(work_id)?;
    let global_used = state.used_resources().ok().flatten();
    if !fits_after(
        global_used.as_ref(),
        work.spec().request(),
        state.binding().capacity(),
        state.binding().limits().resource_dimensions(),
    ) {
        return None;
    }
    state.workers().iter().find_map(|worker| {
        if worker.phase() != WorkerPhase::Available
            || worker.descriptor().owner() != work.spec().owner()
            || !worker.descriptor().supports(work.spec().class())
        {
            return None;
        }
        let active: Vec<_> = state
            .reservations()
            .iter()
            .filter(|reservation| reservation.worker_id() == worker.descriptor().id())
            .collect();
        if active.len() >= usize::from(worker.descriptor().concurrency()) {
            return None;
        }
        let used = resource_sum(active.into_iter(), state.binding().limits().resource_dimensions())
            .ok()?;
        fits_after(
            used.as_ref(),
            work.spec().request(),
            worker.descriptor().capacity(),
            state.binding().limits().resource_dimensions(),
        )
        .then_some(worker.descriptor().id())
    })
}

fn fits_after(
    current: Option<&ResourceVector>,
    request: &ResourceVector,
    capacity: &ResourceVector,
    dimensions: u16,
) -> bool {
    current.map_or_else(
        || request.fits_within(capacity),
        |current| {
            current
                .checked_add(request, dimensions)
                .is_ok_and(|combined| combined.fits_within(capacity))
        },
    )
}

fn resource_sum<'a>(
    values: impl Iterator<Item = &'a crate::SchedulerReservation>,
    dimensions: u16,
) -> Result<Option<ResourceVector>, ()> {
    let mut sum = None;
    for reservation in values {
        sum = Some(sum.map_or_else(
            || Ok(reservation.resources().clone()),
            |current: ResourceVector| {
                current.checked_add(reservation.resources(), dimensions).map_err(|_| ())
            },
        )?);
    }
    Ok(sum)
}
