//! Canonical semantic digests independent of B3 frame headers.

mod encoder;

use peritus_role::HarnessRole;
use peritus_types::Sha256Digest;

use crate::{
    ArtifactHandoff, CollaborationBinding, CollaborationMessage, CollaborationPhase,
    CollaborationState, CollaborationTask, CollaborationTerminal, CollaborationTerminalKind,
    Delegation, JoinPolicy, ReservationObservation, TaskPhase, TaskTerminal, TaskTerminalKind,
};
use encoder::Encoder;

/// Hashes the immutable complete collaboration binding.
#[must_use]
pub fn binding_digest(binding: &CollaborationBinding) -> Sha256Digest {
    let mut out = Encoder::new(b"peritus-d3-collaboration-binding-v1\0");
    encode_binding_fields(&mut out, binding);
    out.hash()
}

/// Hashes complete authoritative state except its digest field.
#[must_use]
pub fn state_digest(state: &CollaborationState) -> Sha256Digest {
    let mut out = Encoder::new(b"peritus-d3-collaboration-state-v1\0");
    encode_binding(&mut out, state.binding());
    out.u8(phase_tag(state.phase()));
    out.u64(state.sequence().get());
    out.raw(state.last_event_id().as_bytes());
    out.len(state.tasks().len());
    for task in state.tasks() {
        encode_task(&mut out, task);
    }
    out.len(state.messages().len());
    for delivery in state.messages() {
        encode_message(&mut out, delivery.message());
        out.bool(delivery.acknowledged());
    }
    out.len(state.used_commands().len());
    for command in state.used_commands() {
        out.raw(command.as_bytes());
    }
    out.option(state.terminal(), encode_terminal);
    out.hash()
}

/// Hashes one terminal except its own digest field.
#[must_use]
pub fn terminal_digest(terminal: &CollaborationTerminal) -> Sha256Digest {
    let mut out = Encoder::new(b"peritus-d3-collaboration-terminal-v1\0");
    encode_terminal_fields(&mut out, terminal);
    out.hash()
}

pub const fn phase_tag(value: CollaborationPhase) -> u8 {
    match value {
        CollaborationPhase::Active => 1,
        CollaborationPhase::Paused => 2,
        CollaborationPhase::Terminal => 3,
    }
}

pub const fn task_phase_tag(value: TaskPhase) -> u8 {
    match value {
        TaskPhase::Offered => 1,
        TaskPhase::Accepted => 2,
        TaskPhase::Active => 3,
        TaskPhase::Cancelling => 4,
        TaskPhase::Terminal => 5,
    }
}

pub const fn task_terminal_tag(value: TaskTerminalKind) -> u8 {
    match value {
        TaskTerminalKind::Succeeded => 1,
        TaskTerminalKind::Failed => 2,
        TaskTerminalKind::Rejected => 3,
        TaskTerminalKind::Cancelled => 4,
        TaskTerminalKind::Abandoned => 5,
    }
}

pub const fn collaboration_terminal_tag(value: CollaborationTerminalKind) -> u8 {
    match value {
        CollaborationTerminalKind::Completed => 1,
        CollaborationTerminalKind::Failed => 2,
        CollaborationTerminalKind::Cancelled => 3,
        CollaborationTerminalKind::Abandoned => 4,
        CollaborationTerminalKind::UnsatisfiedJoin => 5,
    }
}

pub const fn join_tag(value: JoinPolicy) -> u8 {
    match value {
        JoinPolicy::NoChildren => 1,
        JoinPolicy::AllRequired => 2,
        JoinPolicy::AnyRequired => 3,
    }
}

pub const fn role_tag(value: HarnessRole) -> u8 {
    match value {
        HarnessRole::Writer => 1,
        HarnessRole::Reviewer => 2,
        HarnessRole::Fixer => 3,
        HarnessRole::Evaluator => 4,
        HarnessRole::Evolver => 5,
    }
}

fn encode_binding(out: &mut Encoder, value: &CollaborationBinding) {
    encode_binding_fields(out, value);
    out.digest(value.digest());
}

fn encode_binding_fields(out: &mut Encoder, value: &CollaborationBinding) {
    out.raw(value.id().as_bytes());
    out.raw(value.run_id().as_bytes());
    out.revision(value.revision());
    out.raw(value.scheduler_id().as_bytes());
    out.raw(value.root_task_id().as_bytes());
    let limits = value.limits();
    out.u32(limits.tasks());
    out.u16(limits.depth());
    out.u16(limits.fan_out());
    out.u32(limits.messages());
    out.u16(limits.recipients());
    out.u32(limits.payload_bytes());
    out.u16(limits.artifact_references());
    out.u64(limits.command_bytes());
    out.u64(limits.state_bytes());
    encode_delegation(out, value.root_assignment());
}

fn encode_task(out: &mut Encoder, value: &CollaborationTask) {
    encode_delegation(out, value.assignment());
    out.u8(task_phase_tag(value.phase()));
    out.option(value.reservation(), encode_reservation);
    out.option(value.terminal(), encode_task_terminal);
}

fn encode_delegation(out: &mut Encoder, value: &Delegation) {
    out.raw(value.task_id().as_bytes());
    out.raw(value.root_task_id().as_bytes());
    out.option(value.parent_task_id(), |out, id| out.raw(id.as_bytes()));
    out.u16(value.depth());
    out.raw(value.owner().as_bytes());
    out.u8(role_tag(value.role()));
    out.raw(value.parent_owner().as_bytes());
    out.raw(value.work_id().as_bytes());
    out.digest(value.goal_digest());
    out.bool(value.required());
    out.u8(join_tag(value.join_policy()));
}

fn encode_reservation(out: &mut Encoder, value: ReservationObservation) {
    out.raw(value.work_id().as_bytes());
    out.raw(value.dispatch_id().as_bytes());
    out.raw(value.owner().as_bytes());
    out.revision(value.revision());
}

fn encode_message(out: &mut Encoder, value: &CollaborationMessage) {
    out.raw(value.id().as_bytes());
    out.raw(value.root_task_id().as_bytes());
    out.raw(value.task_id().as_bytes());
    out.raw(value.sender().as_bytes());
    out.raw(value.receiver().as_bytes());
    out.u32(value.ordinal());
    out.option(value.predecessor(), |out, id| out.raw(id.as_bytes()));
    out.text(value.media_type());
    out.u32(value.payload_bytes());
    out.digest(value.content_digest());
    out.option(value.artifact(), encode_artifact);
    out.revision(value.revision());
}

fn encode_artifact(out: &mut Encoder, value: ArtifactHandoff) {
    out.raw(value.artifact_id().as_bytes());
    out.digest(value.artifact_digest());
    out.digest(value.evidence_digest());
    out.revision(value.revision());
}

fn encode_task_terminal(out: &mut Encoder, value: TaskTerminal) {
    out.u8(task_terminal_tag(value.kind()));
    out.option(value.handoff(), encode_artifact);
    out.digest(value.cause_digest());
}

fn encode_terminal(out: &mut Encoder, value: &CollaborationTerminal) {
    encode_terminal_fields(out, value);
    out.digest(value.digest());
}

fn encode_terminal_fields(out: &mut Encoder, value: &CollaborationTerminal) {
    out.u8(collaboration_terminal_tag(value.kind()));
    out.len(value.blocking_tasks().len());
    for task in value.blocking_tasks() {
        out.raw(task.as_bytes());
    }
}
