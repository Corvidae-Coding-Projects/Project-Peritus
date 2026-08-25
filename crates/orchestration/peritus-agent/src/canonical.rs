//! Domain-canonical encoding used only for deterministic state fences.

use crate::{
    AgentFailureKind, AgentPhase, AgentTurnState, CompletionRequest, ToolIdempotency,
    ToolResultStatus, ToolSideEffect, ToolSlotPhase,
};
use peritus_policy::ActorRole;
use peritus_types::{RevisionTuple, Sha256Digest};

mod event_wire;
pub use event_wire::{decode_command_kind, encode_command_kind};

pub fn state_bytes(state: &AgentTurnState) -> Vec<u8> {
    let mut out = Encoder::new();
    out.raw(b"peritus-agent-state-v1");
    let binding = &state.binding;
    out.raw(binding.turn_id().as_bytes());
    out.raw(binding.attempt_id().as_bytes());
    out.raw(binding.actor_id().as_bytes());
    out.u8(role_tag(binding.role()));
    out.raw(binding.session_id().as_bytes());
    out.raw(binding.environment_id().as_bytes());
    out.revision(binding.revision());
    out.raw(binding.provider_profile_id().as_bytes());
    out.u64(binding.provider_profile_revision().get());
    out.u64(binding.limits_revision().get());
    encode_limits(&mut out, state.limits);
    encode_counters(&mut out, state.counters);
    out.u8(state.phase.tag());
    out.option(state.paused_from, |out, phase| out.u8(AgentPhase::Active(phase).tag()));
    out.u64(state.logical_revision.get());
    out.u64(state.sequence.get());
    out.raw(state.last_event_id.as_bytes());
    out.option(state.context, |out, context| {
        out.digest(context.render_digest());
        out.digest(context.plan_digest());
        out.digest(context.memory_digest());
        out.digest(context.estimator_digest());
        out.option(context.compaction_digest(), Encoder::digest);
    });
    encode_model(&mut out, state.model);
    out.option(state.tools.as_ref(), encode_tools);
    out.option(state.tool_transcript_digest, Encoder::digest);
    out.option(state.completion.as_ref(), encode_completion);
    out.option(state.failure.as_ref(), |out, failure| {
        out.u8(failure_tag(failure.kind()));
        if let AgentFailureKind::Exhausted(dimension) = failure.kind() {
            out.u8(dimension as u8);
        }
        out.text(failure.detail().as_str());
    });
    out.bool(state.unresolved_indeterminate);
    out.bool(state.unresolved_indeterminate);
    out.finish()
}

pub fn state_digest(state: &AgentTurnState) -> Sha256Digest {
    peritus_codec::sha256(&state_bytes(state))
}

fn encode_limits(out: &mut Encoder, limits: crate::AgentLimits) {
    out.u16(limits.max_tool_calls());
    out.u32(limits.max_provider_events());
    out.u16(limits.max_context_cycles());
    out.u64(limits.max_output_bytes());
    out.u64(limits.max_tool_result_bytes());
    out.u16(limits.max_concurrent_tool_calls());
    out.u32(limits.max_transitions());
}

fn encode_counters(out: &mut Encoder, counters: crate::AgentCounters) {
    out.u16(counters.tool_calls());
    out.u32(counters.provider_events());
    out.u16(counters.context_cycles());
    out.u64(counters.output_bytes());
    out.u64(counters.tool_result_bytes());
    out.u16(counters.active_tool_calls());
    out.u16(counters.peak_concurrent_tool_calls());
    out.u32(counters.transitions());
}

fn encode_model(out: &mut Encoder, model: crate::ModelState) {
    out.option(model.call_id(), |out, id| out.digest(id.digest()));
    out.option(model.request_digest(), Encoder::digest);
    out.option(model.response_digest(), Encoder::digest);
    out.option(model.stream_digest(), Encoder::digest);
    out.u64(model.cursor());
    out.bool(model.in_flight());
    out.bool(model.normal_terminal());
    out.bool(model.incomplete_items());
    out.bool(model.usage_settled());
    out.u32(model.retry_count());
    out.bool(model.retry_pending());
    out.bool(model.resume_exact());
    out.u32(model.retry_count());
    out.bool(model.retry_pending());
    out.bool(model.resume_exact());
}

fn encode_tools(out: &mut Encoder, batch: &crate::ToolBatch) {
    out.len(batch.slots().len());
    for slot in batch.slots() {
        let proposal = slot.proposal();
        out.u16(proposal.ordinal().get());
        out.digest(proposal.model_call_id().digest());
        out.raw(proposal.action_id().as_bytes());
        out.text(proposal.capability().as_str());
        out.u16(proposal.version().major());
        out.u16(proposal.version().minor());
        out.digest(proposal.argument_digest());
        out.digest(proposal.prepared_digest());
        out.digest(proposal.replay_identity());
        out.revision(proposal.revision());
        out.u64(proposal.deadline().epoch().get());
        out.u64(proposal.deadline().tick_millis());
        out.u8(match proposal.side_effect() {
            ToolSideEffect::None => 0,
            ToolSideEffect::Workspace => 1,
            ToolSideEffect::Process => 2,
            ToolSideEffect::External => 3,
        });
        out.u8(match proposal.idempotency() {
            ToolIdempotency::Idempotent => 0,
            ToolIdempotency::ReplayTerminalOnly => 1,
            ToolIdempotency::NonIdempotent => 2,
        });
        out.u8(match slot.phase() {
            ToolSlotPhase::Proposed => 0,
            ToolSlotPhase::AwaitingAuthorization => 1,
            ToolSlotPhase::Authorized => 2,
            ToolSlotPhase::Dispatched => 3,
            ToolSlotPhase::Active => 4,
            ToolSlotPhase::Terminal => 5,
        });
        out.option(slot.authority_digest(), Encoder::digest);
        out.u32(slot.next_progress_sequence());
        out.option(slot.last_progress_digest(), Encoder::digest);
        out.option(slot.result(), |out, result| {
            out.u8(match result.status() {
                ToolResultStatus::Succeeded => 0,
                ToolResultStatus::Failed => 1,
                ToolResultStatus::Denied => 2,
                ToolResultStatus::Cancelled => 3,
                ToolResultStatus::Indeterminate => 4,
            });
            out.digest(result.result_digest());
            out.u64(result.model_visible_bytes());
            out.len(result.evidence().len());
            for evidence in result.evidence() {
                out.raw(evidence.as_bytes());
            }
        });
    }
}

fn encode_completion(out: &mut Encoder, completion: &crate::CompletionProposal) {
    out.text(completion.summary().as_str());
    out.len(completion.evidence().len());
    for evidence in completion.evidence() {
        out.raw(evidence.id().as_bytes());
        out.revision(evidence.revision());
    }
    out.len(completion.uncertainties().len());
    for uncertainty in completion.uncertainties() {
        out.text(uncertainty.as_str());
    }
    out.revision(completion.revision());
    out.digest(completion.transcripts().context());
    out.digest(completion.transcripts().model());
    out.digest(completion.transcripts().tools());
    out.u8(match completion.requested() {
        CompletionRequest::RunGates => 0,
        CompletionRequest::RequestReview => 1,
        CompletionRequest::ContinueFixing => 2,
        CompletionRequest::RequestAuthority => 3,
        CompletionRequest::ReportBlocked => 4,
    });
}

const fn role_tag(role: ActorRole) -> u8 {
    match role {
        ActorRole::Writer => 0,
        ActorRole::Fixer => 1,
        ActorRole::Reviewer => 2,
        ActorRole::Evaluator => 3,
        ActorRole::GateRunner => 4,
        ActorRole::Orchestrator => 5,
        ActorRole::EvolutionAgent => 6,
        ActorRole::HumanAuthority => 7,
        ActorRole::DaemonService => 8,
        ActorRole::ProviderToolWorker => 9,
        ActorRole::Plugin => 10,
    }
}

const fn failure_tag(kind: AgentFailureKind) -> u8 {
    match kind {
        AgentFailureKind::Provider => 0,
        AgentFailureKind::Tool => 1,
        AgentFailureKind::Context => 2,
        AgentFailureKind::Protocol => 3,
        AgentFailureKind::Exhausted(_) => 4,
        AgentFailureKind::Indeterminate => 5,
    }
}

struct Encoder {
    bytes: Vec<u8>,
}
impl Encoder {
    const fn new() -> Self {
        Self { bytes: Vec::new() }
    }
    fn finish(self) -> Vec<u8> {
        self.bytes
    }
    fn raw(&mut self, value: &[u8]) {
        self.bytes.extend_from_slice(value);
    }
    fn bool(&mut self, value: bool) {
        self.u8(u8::from(value));
    }
    fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }
    fn u16(&mut self, value: u16) {
        self.raw(&value.to_be_bytes());
    }
    fn u32(&mut self, value: u32) {
        self.raw(&value.to_be_bytes());
    }
    fn u64(&mut self, value: u64) {
        self.raw(&value.to_be_bytes());
    }
    fn len(&mut self, value: usize) {
        self.u64(u64::try_from(value).expect("checked domain bounds fit u64"));
    }
    fn text(&mut self, value: &str) {
        self.len(value.len());
        self.raw(value.as_bytes());
    }
    fn digest(&mut self, value: Sha256Digest) {
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
    fn revision(&mut self, value: RevisionTuple) {
        self.raw(value.acceptance_spec_id().as_bytes());
        self.raw(value.harness_id().as_bytes());
        self.raw(value.workspace_id().as_bytes());
        self.u64(value.workspace_generation().get());
        self.u64(value.workspace_revision().get());
        self.raw(value.policy_id().as_bytes());
        self.raw(value.provider_profile_id().as_bytes());
    }
}
