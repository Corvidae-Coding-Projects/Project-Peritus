//! Domain-separated canonical scheduler hashing and stable tags.

mod encoder;

use peritus_types::Sha256Digest;

use crate::{
    ExecutionClass, RecoveryPolicy, SchedulerBinding, SchedulerPhase, SchedulerReservation,
    SchedulerState, SchedulerTerminal, SchedulerTerminalKind, WorkPhase, WorkRecord, WorkSpec,
    WorkTerminal, WorkerDescriptor, WorkerPhase, WorkerRecord,
};

use encoder::Encoder;

/// Hashes every immutable scheduler binding field under a stable domain separator.
#[must_use]
pub fn binding_digest(binding: &SchedulerBinding) -> Sha256Digest {
    let mut out = Encoder::new(b"peritus-d3-scheduler-binding-v1\0");
    encode_binding(&mut out, binding);
    out.hash()
}

/// Hashes every scheduler state field while logically zeroing its state-digest field.
#[must_use]
pub fn state_digest(state: &SchedulerState) -> Sha256Digest {
    let mut out = Encoder::new(b"peritus-d3-scheduler-state-v1\0");
    encode_binding(&mut out, state.binding());
    out.u8(scheduler_phase_tag(state.phase()));
    out.u64(state.sequence().get());
    out.raw(state.last_event_id().as_bytes());
    out.len(state.workers().len());
    for worker in state.workers() {
        encode_worker(&mut out, worker);
    }
    out.len(state.work().len());
    for work in state.work() {
        encode_work(&mut out, work);
    }
    out.len(state.reservations().len());
    for reservation in state.reservations() {
        encode_reservation(&mut out, reservation);
    }
    out.len(state.used_dispatches().len());
    for dispatch in state.used_dispatches() {
        out.raw(dispatch.as_bytes());
    }
    out.u64(state.enqueue_ordinal());
    out.u64(state.dispatch_ordinal());
    out.len(state.used_commands().len());
    for command in state.used_commands() {
        out.raw(command.as_bytes());
    }
    out.option(state.terminal(), encode_terminal);
    out.hash()
}

/// Hashes terminal truth independently of its digest field.
#[must_use]
pub fn terminal_digest(terminal: &SchedulerTerminal) -> Sha256Digest {
    let mut out = Encoder::new(b"peritus-d3-scheduler-terminal-v1\0");
    encode_terminal_fields(&mut out, terminal);
    out.hash()
}

fn encode_binding(out: &mut Encoder, value: &SchedulerBinding) {
    out.raw(value.run_id().as_bytes());
    out.raw(value.scheduler_id().as_bytes());
    out.revision(value.revision());
    let limits = value.limits();
    out.u32(limits.queued_work());
    out.u32(limits.retained_work());
    out.u16(limits.workers());
    out.u16(limits.dependencies_per_work());
    out.u16(limits.resource_dimensions());
    out.u16(limits.active_reservations());
    out.u16(limits.attempts_per_work());
    out.u16(limits.bypass_count());
    out.u16(limits.dispatch_batch_size());
    out.u64(limits.payload_bytes());
    out.u64(limits.state_bytes());
    encode_resources(out, value.capacity());
}

fn encode_worker(out: &mut Encoder, value: &WorkerRecord) {
    encode_descriptor(out, value.descriptor());
    out.u8(worker_phase_tag(value.phase()));
}

fn encode_descriptor(out: &mut Encoder, value: &WorkerDescriptor) {
    out.raw(value.id().as_bytes());
    out.raw(value.owner().as_bytes());
    out.len(value.classes().len());
    for class in value.classes() {
        out.u8(execution_class_tag(*class));
    }
    encode_resources(out, value.capacity());
    out.u16(value.concurrency());
}

fn encode_work(out: &mut Encoder, value: &WorkRecord) {
    encode_spec(out, value.spec());
    out.u8(work_phase_tag(value.phase()));
    out.u64(value.enqueue_ordinal());
    out.u16(value.bypasses());
    out.u16(value.attempts_started());
    out.option(value.retry_cause().as_ref(), |encoder, digest| encoder.digest(*digest));
    out.option(value.terminal(), encode_work_terminal);
}

fn encode_spec(out: &mut Encoder, value: &WorkSpec) {
    out.raw(value.id().as_bytes());
    out.raw(value.owner().as_bytes());
    out.revision(value.revision());
    out.u8(execution_class_tag(value.class()));
    out.u8(value.priority());
    encode_resources(out, value.request());
    out.option(value.budget_reservation().as_ref(), |encoder, id| encoder.raw(id.as_bytes()));
    out.len(value.dependencies().len());
    for dependency in value.dependencies() {
        out.raw(dependency.as_bytes());
    }
    out.option(value.parent().as_ref(), |encoder, id| encoder.raw(id.as_bytes()));
    out.u16(value.maximum_attempts().get());
    out.u8(recovery_policy_tag(value.recovery()));
    out.digest(value.payload_digest());
}

fn encode_reservation(out: &mut Encoder, value: &SchedulerReservation) {
    out.raw(value.work_id().as_bytes());
    out.raw(value.dispatch_id().as_bytes());
    out.raw(value.worker_id().as_bytes());
    out.raw(value.owner().as_bytes());
    out.u16(value.attempt().get());
    out.revision(value.revision());
    encode_resources(out, value.resources());
    out.digest(value.dispatch_token());
    out.bool(value.started());
}

fn encode_resources(out: &mut Encoder, value: &crate::ResourceVector) {
    out.len(value.entries().len());
    for entry in value.entries() {
        out.u16(entry.kind().tag());
        out.u64(entry.quantity().get());
    }
}

fn encode_terminal(out: &mut Encoder, value: &SchedulerTerminal) {
    encode_terminal_fields(out, value);
    out.digest(value.digest());
}

fn encode_terminal_fields(out: &mut Encoder, value: &SchedulerTerminal) {
    out.u8(terminal_kind_tag(value.kind()));
    out.len(value.non_successful_work().len());
    for work in value.non_successful_work() {
        out.raw(work.as_bytes());
    }
}

fn encode_work_terminal(out: &mut Encoder, value: &WorkTerminal) {
    match value {
        WorkTerminal::Succeeded { result_digest } => {
            out.u8(1);
            out.digest(*result_digest);
        }
        WorkTerminal::Failed { failure_digest } => {
            out.u8(2);
            out.digest(*failure_digest);
        }
        WorkTerminal::DependencyFailed { dependency } => {
            out.u8(3);
            out.raw(dependency.as_bytes());
        }
        WorkTerminal::Cancelled => out.u8(4),
        WorkTerminal::Ambiguous { dispatch_id } => {
            out.u8(5);
            out.raw(dispatch_id.as_bytes());
        }
        WorkTerminal::Exhausted { cause_digest } => {
            out.u8(6);
            out.digest(*cause_digest);
        }
        WorkTerminal::Abandoned { cause_digest } => {
            out.u8(7);
            out.digest(*cause_digest);
        }
    }
}

const fn execution_class_tag(value: ExecutionClass) -> u8 {
    match value {
        ExecutionClass::Model => 1,
        ExecutionClass::Tool => 2,
        ExecutionClass::Gate => 3,
        ExecutionClass::Review => 4,
        ExecutionClass::Coordination => 5,
    }
}
const fn recovery_policy_tag(value: RecoveryPolicy) -> u8 {
    match value {
        RecoveryPolicy::RetrySafe => 1,
        RecoveryPolicy::Ambiguous => 2,
        RecoveryPolicy::Fail => 3,
    }
}
const fn worker_phase_tag(value: WorkerPhase) -> u8 {
    match value {
        WorkerPhase::Available => 1,
        WorkerPhase::Busy => 2,
        WorkerPhase::Draining => 3,
        WorkerPhase::Lost => 4,
        WorkerPhase::Removed => 5,
    }
}
const fn work_phase_tag(value: WorkPhase) -> u8 {
    match value {
        WorkPhase::WaitingDependencies => 1,
        WorkPhase::Queued => 2,
        WorkPhase::Reserved => 3,
        WorkPhase::Running => 4,
        WorkPhase::RetryPending => 5,
        WorkPhase::Cancelling => 6,
        WorkPhase::Terminal => 7,
    }
}
const fn scheduler_phase_tag(value: SchedulerPhase) -> u8 {
    match value {
        SchedulerPhase::Active => 1,
        SchedulerPhase::Paused => 2,
        SchedulerPhase::Draining => 3,
        SchedulerPhase::DrainingPaused => 4,
        SchedulerPhase::Terminal => 5,
    }
}
const fn terminal_kind_tag(value: SchedulerTerminalKind) -> u8 {
    match value {
        SchedulerTerminalKind::Completed => 1,
        SchedulerTerminalKind::Failed => 2,
        SchedulerTerminalKind::DependencyFailed => 3,
        SchedulerTerminalKind::Ambiguous => 4,
        SchedulerTerminalKind::Exhausted => 5,
        SchedulerTerminalKind::Cancelled => 6,
    }
}
