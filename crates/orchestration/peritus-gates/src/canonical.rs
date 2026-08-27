//! Domain-canonical D1 state and terminal hashing.

use peritus_types::{GateId, RevisionTuple, Sha256Digest};
use sha2::{Digest, Sha256};

use crate::{
    GateAttemptResult, GateOutcomeKind, GateRunPhase, GateRunState, GateSlotPhase,
    GateTerminalKind, RecoveryRequirement, RetryPermission,
};

pub fn state_digest(state: &GateRunState) -> Sha256Digest {
    peritus_codec::sha256(&state_bytes(state))
}

pub fn state_bytes(state: &GateRunState) -> Vec<u8> {
    let mut out = Encoder::new(b"peritus-d1-gate-state-v1\0");
    out.raw(state.run_id().as_bytes());
    out.digest(state.plan_digest());
    out.revision(state.revision());
    out.digest(state.snapshot_digest());
    out.u16(state.maximum_attempts());
    out.u8(run_phase_tag(state.phase()));
    out.u64(state.sequence().get());
    out.raw(state.last_event_id().as_bytes());
    out.len(state.slots().len());
    for slot in state.slots() {
        out.raw(slot.gate_id().as_bytes());
        out.u8(slot_phase_tag(slot.phase()));
        out.u16(slot.attempts());
        out.option(slot.active_attempt(), encode_attempt);
        out.option(slot.last_result(), encode_result);
        out.option(slot.result_event(), |out, event| out.raw(event.as_bytes()));
        out.option(slot.evidence(), |out, receipt| {
            out.raw(receipt.run_id().as_bytes());
            out.raw(receipt.gate_id().as_bytes());
            encode_attempt(out, receipt.attempt());
            out.revision(receipt.revision());
            out.raw(receipt.result_event().as_bytes());
            out.u64(receipt.result_position());
            out.digest(receipt.result_digest());
            out.len(receipt.publication().required().len());
            for requirement in receipt.publication().required() {
                out.digest(requirement.digest());
            }
            out.len(receipt.quality_artifacts().len());
            for artifact in receipt.quality_artifacts() {
                encode_artifact(out, artifact);
            }
            out.digest(receipt.manifest_digest());
            out.digest(receipt.receipt_digest());
            out.len(receipt.evidence().len());
            for evidence in receipt.evidence() {
                out.digest(evidence.requirement_id().digest());
                out.raw(evidence.evidence_id().as_bytes());
                out.digest(evidence.record_digest());
                out.u64(evidence.journal_position());
                out.raw(evidence.producing_event().as_bytes());
            }
        });
        out.option(slot.blocked_by(), |out, gate| out.raw(gate.as_bytes()));
    }
    out.len(state.used_executions().len());
    for execution in state.used_executions() {
        out.raw(execution.as_bytes());
    }
    out.len(state.used_actions().len());
    for action in state.used_actions() {
        out.raw(action.as_bytes());
    }
    out.option(state.terminal(), |out, terminal| {
        out.u8(terminal_kind_tag(terminal.kind()));
        out.digest(terminal.digest());
        out.len(terminal.non_passing().len());
        for gate in terminal.non_passing() {
            out.raw(gate.as_bytes());
        }
    });
    out.finish()
}

pub fn terminal_digest(
    kind: GateTerminalKind,
    non_passing: &[GateId],
    state_digest: Sha256Digest,
) -> Sha256Digest {
    let mut hash = Sha256::new();
    hash.update(b"peritus-d1-gate-terminal-v1\0");
    hash.update([terminal_kind_tag(kind)]);
    hash.update(state_digest.as_bytes());
    hash.update(u64::try_from(non_passing.len()).unwrap_or(u64::MAX).to_be_bytes());
    for gate in non_passing {
        hash.update(gate.as_bytes());
    }
    Sha256Digest::new(hash.finalize().into())
}

fn encode_result(out: &mut Encoder, result: &GateAttemptResult) {
    out.raw(result.gate_id().as_bytes());
    out.u8(outcome_tag(result.kind()));
    out.digest(result.tool_result_digest());
    out.option(result.candidate_result_digest(), Encoder::digest);
    out.option(result.execution_plan_digest(), Encoder::digest);
    out.option(result.process_id(), |out, process| out.raw(process.as_bytes()));
    out.u8(retry_tag(result.retry_permission()));
    out.u8(recovery_tag(result.recovery_requirement()));
    out.len(result.artifacts().len());
    for artifact in result.artifacts() {
        encode_artifact(out, artifact);
    }
}

fn encode_attempt(out: &mut Encoder, attempt: crate::ActiveAttempt) {
    out.raw(attempt.execution_id().as_bytes());
    out.u16(attempt.ordinal().get());
    out.raw(attempt.action_id().as_bytes());
    out.digest(attempt.prepared_digest());
    out.digest(attempt.replay_digest());
    out.digest(attempt.snapshot_digest());
}

fn encode_artifact(out: &mut Encoder, artifact: &crate::GateArtifact) {
    out.digest(artifact.digest());
    out.u64(artifact.size());
    out.text(artifact.media_type());
    out.text(artifact.label());
}

pub const fn run_phase_tag(value: GateRunPhase) -> u8 {
    match value {
        GateRunPhase::Active => 1,
        GateRunPhase::Cancelling => 2,
        GateRunPhase::Terminal => 3,
        GateRunPhase::Paused(crate::GateResumePhase::Active) => 4,
        GateRunPhase::Paused(crate::GateResumePhase::Cancelling) => 5,
    }
}

pub const fn slot_phase_tag(value: GateSlotPhase) -> u8 {
    match value {
        GateSlotPhase::Pending => 1,
        GateSlotPhase::Prepared => 2,
        GateSlotPhase::Dispatched => 3,
        GateSlotPhase::RecoveryPending => 4,
        GateSlotPhase::RetryPending => 5,
        GateSlotPhase::EvidencePending => 6,
        GateSlotPhase::Passed => 7,
        GateSlotPhase::Failed => 8,
        GateSlotPhase::Blocked => 9,
        GateSlotPhase::Cancelled => 10,
    }
}

pub const fn terminal_kind_tag(value: GateTerminalKind) -> u8 {
    match value {
        GateTerminalKind::Passed => 1,
        GateTerminalKind::Failed => 2,
        GateTerminalKind::Cancelled => 3,
        GateTerminalKind::Indeterminate => 4,
    }
}

pub const fn outcome_tag(value: GateOutcomeKind) -> u8 {
    match value {
        GateOutcomeKind::Passed => 1,
        GateOutcomeKind::CandidateFailure => 2,
        GateOutcomeKind::InfrastructureFailure => 3,
        GateOutcomeKind::Cancelled => 4,
        GateOutcomeKind::TimedOut => 5,
        GateOutcomeKind::MalformedOutput => 6,
        GateOutcomeKind::IncompleteEvidence => 7,
    }
}

pub const fn retry_tag(value: RetryPermission) -> u8 {
    match value {
        RetryPermission::Never => 1,
        RetryPermission::FreshAction => 2,
        RetryPermission::AfterRecovery => 3,
    }
}

pub const fn recovery_tag(value: RecoveryRequirement) -> u8 {
    match value {
        RecoveryRequirement::None => 1,
        RecoveryRequirement::Reauthorize => 2,
        RecoveryRequirement::ReconcileWorkspace => 3,
        RecoveryRequirement::ReconcileProcess => 4,
        RecoveryRequirement::RepublishArtifact => 5,
        RecoveryRequirement::HumanReview => 6,
    }
}

struct Encoder {
    bytes: Vec<u8>,
}

impl Encoder {
    fn new(domain: &[u8]) -> Self {
        Self { bytes: domain.to_vec() }
    }
    fn finish(self) -> Vec<u8> {
        self.bytes
    }
    fn raw(&mut self, value: &[u8]) {
        self.bytes.extend_from_slice(value);
    }
    fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }
    fn u16(&mut self, value: u16) {
        self.raw(&value.to_be_bytes());
    }
    fn u64(&mut self, value: u64) {
        self.raw(&value.to_be_bytes());
    }
    fn len(&mut self, value: usize) {
        self.u64(u64::try_from(value).unwrap_or(u64::MAX));
    }
    fn digest(&mut self, value: Sha256Digest) {
        self.raw(value.as_bytes());
    }
    fn text(&mut self, value: &str) {
        self.len(value.len());
        self.raw(value.as_bytes());
    }
    fn option<T>(&mut self, value: Option<T>, encode: impl FnOnce(&mut Self, T)) {
        match value {
            Some(value) => {
                self.u8(1);
                encode(self, value);
            }
            None => self.u8(0),
        }
    }
    fn revision(&mut self, revision: RevisionTuple) {
        self.raw(revision.acceptance_spec_id().as_bytes());
        self.raw(revision.harness_id().as_bytes());
        self.raw(revision.workspace_id().as_bytes());
        self.u64(revision.workspace_generation().get());
        self.u64(revision.workspace_revision().get());
        self.raw(revision.policy_id().as_bytes());
        self.raw(revision.provider_profile_id().as_bytes());
    }
}
