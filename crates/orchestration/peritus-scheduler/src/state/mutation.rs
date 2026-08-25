//! Reducer-only state mutation and derived invariant maintenance.

use peritus_types::{CommandId, EventId, EventSequence, Sha256Digest};

use crate::{
    DispatchId, SchedulerPhase, SchedulerReservation, SchedulerState, SchedulerTerminal, WorkId,
    WorkPhase, WorkRecord, WorkTerminal, WorkerId, WorkerPhase, WorkerRecord,
};

pub fn worker_mut(state: &mut SchedulerState, id: WorkerId) -> Option<&mut WorkerRecord> {
    state
        .workers
        .binary_search_by_key(&id, |record| record.descriptor().id())
        .ok()
        .map(|index| &mut state.workers[index])
}

pub fn work_mut(state: &mut SchedulerState, id: WorkId) -> Option<&mut WorkRecord> {
    state
        .work
        .binary_search_by_key(&id, |record| record.spec().id())
        .ok()
        .map(|index| &mut state.work[index])
}

pub fn reservation_mut(
    state: &mut SchedulerState,
    id: DispatchId,
) -> Option<&mut SchedulerReservation> {
    state
        .reservations
        .binary_search_by_key(&id, SchedulerReservation::dispatch_id)
        .ok()
        .map(|index| &mut state.reservations[index])
}

pub fn insert_worker(state: &mut SchedulerState, value: WorkerRecord) {
    let at = state
        .workers
        .binary_search_by_key(&value.descriptor().id(), |record| record.descriptor().id())
        .unwrap_or_else(|index| index);
    state.workers.insert(at, value);
}

pub fn insert_work(state: &mut SchedulerState, value: WorkRecord) {
    let at = state
        .work
        .binary_search_by_key(&value.spec().id(), |record| record.spec().id())
        .unwrap_or_else(|index| index);
    state.work.insert(at, value);
}

pub fn insert_reservation(state: &mut SchedulerState, value: SchedulerReservation) {
    let at = state
        .reservations
        .binary_search_by_key(&value.dispatch_id(), SchedulerReservation::dispatch_id)
        .unwrap_or_else(|index| index);
    state.reservations.insert(at, value);
}

pub fn retain_dispatch_identity(state: &mut SchedulerState, id: DispatchId) {
    let at = state.used_dispatches.binary_search(&id).unwrap_or_else(|index| index);
    state.used_dispatches.insert(at, id);
}

pub fn remove_reservation(
    state: &mut SchedulerState,
    id: DispatchId,
) -> Option<SchedulerReservation> {
    state
        .reservations
        .binary_search_by_key(&id, SchedulerReservation::dispatch_id)
        .ok()
        .map(|index| state.reservations.remove(index))
}

pub fn next_enqueue_ordinal(state: &mut SchedulerState) -> Option<u64> {
    let value = state.enqueue_ordinal.checked_add(1)?;
    state.enqueue_ordinal = value;
    Some(value)
}

pub const fn increment_dispatch_ordinal(state: &mut SchedulerState) -> bool {
    if let Some(value) = state.dispatch_ordinal.checked_add(1) {
        state.dispatch_ordinal = value;
        true
    } else {
        false
    }
}

pub const fn set_phase(state: &mut SchedulerState, phase: SchedulerPhase) {
    state.phase = phase;
}

pub fn set_terminal(state: &mut SchedulerState, terminal: SchedulerTerminal) {
    state.phase = SchedulerPhase::Terminal;
    state.terminal = Some(terminal);
}

pub fn advance_cursor(
    state: &mut SchedulerState,
    sequence: EventSequence,
    event_id: EventId,
    command_id: CommandId,
) {
    state.sequence = sequence;
    state.last_event_id = event_id;
    state.used_commands.push(command_id);
}

pub const fn set_state_digest(state: &mut SchedulerState, digest: Sha256Digest) {
    state.state_digest = digest;
}

pub fn refresh(state: &mut SchedulerState) {
    propagate_dependencies(state);
    refresh_worker_phases(state);
}

fn propagate_dependencies(state: &mut SchedulerState) {
    loop {
        let mut changes = Vec::new();
        for record in &state.work {
            if !matches!(record.phase(), WorkPhase::WaitingDependencies | WorkPhase::Queued) {
                continue;
            }
            let mut failed = None;
            let mut all_success = true;
            for dependency in record.spec().dependencies() {
                let observed = state.work_item(*dependency);
                match observed.and_then(WorkRecord::terminal) {
                    Some(WorkTerminal::Succeeded { .. }) => {}
                    Some(_) => {
                        failed = Some(*dependency);
                        break;
                    }
                    None => all_success = false,
                }
            }
            if let Some(dependency) = failed {
                changes.push((record.spec().id(), Some(dependency)));
            } else if all_success && record.phase() == WorkPhase::WaitingDependencies {
                changes.push((record.spec().id(), None));
            }
        }
        if changes.is_empty() {
            break;
        }
        for (id, failed) in changes {
            if let Some(record) = work_mut(state, id) {
                if let Some(dependency) = failed {
                    record.terminalize(WorkTerminal::DependencyFailed { dependency });
                } else {
                    record.set_phase(WorkPhase::Queued);
                }
            }
        }
    }
}

fn refresh_worker_phases(state: &mut SchedulerState) {
    let ids: Vec<_> = state.workers.iter().map(|worker| worker.descriptor().id()).collect();
    for id in ids {
        let Some(worker) = state.worker(id) else { continue };
        if matches!(
            worker.phase(),
            WorkerPhase::Draining | WorkerPhase::Lost | WorkerPhase::Removed
        ) {
            continue;
        }
        let active =
            state.reservations.iter().filter(|reservation| reservation.worker_id() == id).count();
        let phase = if active >= usize::from(worker.descriptor().concurrency()) {
            WorkerPhase::Busy
        } else {
            WorkerPhase::Available
        };
        if let Some(worker) = worker_mut(state, id) {
            worker.set_phase(phase);
        }
    }
}
