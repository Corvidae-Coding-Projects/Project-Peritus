//! Verified semantic clone for the complete scheduler aggregate.

use super::SchedulerState;

use crate::{SchedulerReservation, SchedulerTerminal, WorkRecord, WorkerRecord};

impl SchedulerState {}
fn clone_worker_records(values: &[WorkerRecord]) -> Vec<WorkerRecord> {
    let mut result = Vec::with_capacity(values.len());
    let mut index = 0;
    while index < values.len() {
        result.push(values[index].clone());
        index += 1;
    }
    result
}
fn clone_work_records(values: &[WorkRecord]) -> Vec<WorkRecord> {
    let mut result = Vec::with_capacity(values.len());
    let mut index = 0;
    while index < values.len() {
        result.push(values[index].clone());
        index += 1;
    }
    result
}
fn clone_reservations(values: &[SchedulerReservation]) -> Vec<SchedulerReservation> {
    let mut result = Vec::with_capacity(values.len());
    let mut index = 0;
    while index < values.len() {
        result.push(values[index].clone());
        index += 1;
    }
    result
}
fn clone_dispatch_ids(values: &[crate::DispatchId]) -> Vec<crate::DispatchId> {
    let mut result = Vec::with_capacity(values.len());
    let mut index = 0;
    while index < values.len() {
        result.push(values[index]);
        index += 1;
    }
    result
}
fn clone_command_ids(values: &[peritus_types::CommandId]) -> Vec<peritus_types::CommandId> {
    let mut result = Vec::with_capacity(values.len());
    let mut index = 0;
    while index < values.len() {
        result.push(values[index]);
        index += 1;
    }
    result
}
fn clone_terminal(value: Option<&SchedulerTerminal>) -> Option<SchedulerTerminal> {
    let terminal = value?;
    Some(terminal.clone())
}
impl Clone for SchedulerState {
    fn clone(&self) -> Self {
        let result = Self {
            binding: self.binding.clone(),
            phase: self.phase,
            sequence: self.sequence,
            last_event_id: self.last_event_id,
            state_digest: self.state_digest,
            workers: clone_worker_records(&self.workers),
            work: clone_work_records(&self.work),
            reservations: clone_reservations(&self.reservations),
            used_dispatches: clone_dispatch_ids(&self.used_dispatches),
            enqueue_ordinal: self.enqueue_ordinal,
            dispatch_ordinal: self.dispatch_ordinal,
            used_commands: clone_command_ids(&self.used_commands),
            terminal: clone_terminal(self.terminal.as_ref()),
        };
        result
    }
}
