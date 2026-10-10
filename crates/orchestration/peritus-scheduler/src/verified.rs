//! Executable scheduler invariants and focused Verus proof roots.

use std::collections::BTreeSet;

use crate::{
    SchedulerPhase, SchedulerState, SchedulerTerminalKind, SchedulerTransition, WorkId, WorkPhase,
    WorkTerminal,
};

mod reservations;

/// Returns whether every dependency of work is terminal-successful.
#[must_use]
pub fn dependencies_are_ready(state: &SchedulerState, work_id: WorkId) -> bool {
    state.work_item(work_id).is_some_and(|work| {
        work.spec().dependencies().iter().all(|id| {
            state
                .work_item(*id)
                .and_then(crate::WorkRecord::terminal)
                .is_some_and(|terminal| matches!(terminal, WorkTerminal::Succeeded { .. }))
        })
    })
}

/// Returns whether global and worker resource/concurrency capacities contain every reservation.
#[must_use]
pub fn reservations_fit(state: &SchedulerState) -> bool {
    let limits = state.binding().limits();
    let Ok(global) = resource_sum(state.reservations().iter(), limits.resource_dimensions()) else {
        return false;
    };
    if global.is_some_and(|used| !used.fits_within(state.binding().capacity())) {
        return false;
    }
    state.workers().iter().all(|worker| {
        let owned: Vec<_> = state
            .reservations()
            .iter()
            .filter(|reservation| reservation.worker_id() == worker.descriptor().id())
            .collect();
        if owned.len() > usize::from(worker.descriptor().concurrency()) {
            return false;
        }
        let Ok(used) = resource_sum(owned.into_iter(), limits.resource_dimensions()) else {
            return false;
        };
        used.is_none_or(|used| used.fits_within(worker.descriptor().capacity()))
    })
}

fn resource_sum<'a>(
    mut values: impl Iterator<Item = &'a crate::SchedulerReservation>,
    dimensions: u16,
) -> Result<Option<crate::ResourceVector>, crate::SchedulerError> {
    values.try_fold(None::<crate::ResourceVector>, |sum, reservation| {
        sum.map_or_else(
            || Ok(Some(reservation.resources().clone())),
            |current| current.checked_add(reservation.resources(), dimensions).map(Some),
        )
    })
}

/// Returns whether every live dispatch uniquely owns one work attempt and vice versa.
#[must_use]
pub fn unique_dispatch_ownership(state: &SchedulerState) -> bool {
    let mut dispatches = BTreeSet::new();
    let mut work = BTreeSet::new();
    let reservations_valid = state.reservations().iter().all(|reservation| {
        dispatches.insert(reservation.dispatch_id())
            && work.insert(reservation.work_id())
            && state.work_item(reservation.work_id()).is_some_and(|record| {
                matches!(
                    record.phase(),
                    WorkPhase::Reserved | WorkPhase::Running | WorkPhase::Cancelling
                ) && record.attempts_started() == reservation.attempt().get()
            })
    });
    reservations_valid
        && state.work().iter().all(|record| {
            let count = state
                .reservations()
                .iter()
                .filter(|reservation| reservation.work_id() == record.spec().id())
                .count();
            if matches!(
                record.phase(),
                WorkPhase::Reserved | WorkPhase::Running | WorkPhase::Cancelling
            ) {
                count == 1
            } else {
                count == 0
            }
        })
}

/// Returns whether attempts are nonzero when owned and never exceed immutable per-work bounds.
#[must_use]
pub fn attempts_are_monotonic(state: &SchedulerState) -> bool {
    state.work().iter().all(|record| {
        record.attempts_started() <= record.spec().maximum_attempts().get()
            && (!matches!(
                record.phase(),
                WorkPhase::Reserved | WorkPhase::Running | WorkPhase::Cancelling
            ) || record.attempts_started() > 0)
    })
}

/// Returns executable truthful-terminal invariant.
#[must_use]
pub fn no_implicit_success(state: &SchedulerState) -> bool {
    state.terminal().is_none_or(|terminal| {
        state.phase() == SchedulerPhase::Terminal
            && state.reservations().is_empty()
            && state.all_work_terminal()
            && (terminal.kind() != SchedulerTerminalKind::Completed
                || state.work().iter().all(|record| {
                    matches!(record.terminal(), Some(WorkTerminal::Succeeded { .. }))
                }))
    })
}

/// Returns exact complete-state replay equivalence.
#[must_use]
pub fn replay_equivalent(expected: &SchedulerState, observed: &SchedulerState) -> bool {
    expected == observed && expected.state_digest() == observed.state_digest()
}

/// Returns whether a transition advances exactly once with all state/event fences.
#[must_use]
pub fn transition_is_legal(prior: &SchedulerState, transition: &SchedulerTransition) -> bool {
    let event = transition.event();
    let successor = transition.state();
    prior.phase() != SchedulerPhase::Terminal
        && event.run_id() == prior.run_id()
        && event.previous_event() == Some(prior.last_event_id())
        && event.prior_state_digest() == prior.state_digest()
        && event.sequence().get() == prior.sequence().get().saturating_add(1)
        && successor.sequence() == event.sequence()
        && successor.last_event_id() == event.id()
        && successor.state_digest() == event.successor_state_digest()
}
