//! Bounded canonical payload codec for pure D0 command/event facts.

use crate::{
    AgentCommandKind, AgentErrorCode, AgentFailure, AgentFailureKind, AgentLimitDimension,
    AgentLimits, AgentOperation, AgentRecovery, AgentRejection, CompletionProposal,
    CompletionRequest, ContextRecord, EvidenceReference, ModelCallId, ModelTerminalRecord,
    ProviderEventRecord, ProviderRetryClass, ProviderRetryRecord, SafeText, ToolIdempotency,
    ToolOrdinal, ToolProposal, ToolResultRecord, ToolResultStatus, ToolSideEffect, ToolVersion,
    TranscriptDigests,
};
use peritus_policy::AuthorityInstant;
use peritus_types::{
    AcceptanceSpecId, ActionId, CapabilityName, EvidenceId, Generation, HarnessId, PolicyId,
    ProviderProfileId, RevisionNumber, RevisionTuple, Sha256Digest, WorkspaceId,
};

pub fn encode_command_kind(kind: &AgentCommandKind) -> Vec<u8> {
    let mut writer = Writer::new();
    writer.raw(b"peritus-agent-command-v1");
    encode_kind(&mut writer, kind);
    writer.finish()
}

pub fn decode_command_kind(bytes: &[u8]) -> Result<AgentCommandKind, AgentRejection> {
    let mut reader = Reader::new(bytes);
    reader.expect(b"peritus-agent-command-v1")?;
    let kind = decode_kind(&mut reader)?;
    if !reader.finished() {
        return Err(wire_error("command payload has trailing bytes"));
    }
    Ok(kind)
}

fn encode_kind(w: &mut Writer, kind: &AgentCommandKind) {
    match kind {
        AgentCommandKind::ContextPrepared(value) => {
            w.u8(1);
            context(w, *value);
        }
        AgentCommandKind::ModelRequestStarted { call_id, request_digest } => {
            w.u8(2);
            w.digest(call_id.digest());
            w.digest(*request_digest);
        }
        AgentCommandKind::ProviderEventObserved(value) => {
            w.u8(3);
            provider(w, value);
        }
        AgentCommandKind::ProviderRetryScheduled(value) => {
            w.u8(4);
            retry(w, *value);
        }
        AgentCommandKind::ToolCallsProposed { terminal, proposals } => {
            w.u8(5);
            model_terminal(w, *terminal);
            w.len(proposals.len());
            for proposal in proposals {
                tool_proposal(w, proposal);
            }
        }
        AgentCommandKind::CompletionProposed { terminal, proposal } => {
            w.u8(6);
            model_terminal(w, *terminal);
            completion(w, proposal);
        }
        AgentCommandKind::AuthorizationStarted => w.u8(7),
        AgentCommandKind::ToolAuthorized { ordinal, authority_digest } => {
            w.u8(8);
            w.u16(ordinal.get());
            w.digest(*authority_digest);
        }
        AgentCommandKind::ToolDenied { ordinal, result } => {
            w.u8(9);
            w.u16(ordinal.get());
            tool_result(w, result);
        }
        AgentCommandKind::ToolExecutionStarted => w.u8(10),
        AgentCommandKind::ToolDispatched { ordinal } => {
            w.u8(11);
            w.u16(ordinal.get());
        }
        AgentCommandKind::ToolActivated { ordinal } => {
            w.u8(12);
            w.u16(ordinal.get());
        }
        AgentCommandKind::ToolProgressObserved { ordinal, sequence, progress_digest } => {
            w.u8(13);
            w.u16(ordinal.get());
            w.u32(*sequence);
            w.digest(*progress_digest);
        }
        AgentCommandKind::ToolCompleted { ordinal, result } => {
            w.u8(14);
            w.u16(ordinal.get());
            tool_result(w, result);
        }
        AgentCommandKind::ResultRecordingStarted => w.u8(15),
        AgentCommandKind::ResultsRecorded { transcript_digest } => {
            w.u8(16);
            w.digest(*transcript_digest);
        }
        AgentCommandKind::Paused => w.u8(17),
        AgentCommandKind::Resumed { recovery_checked } => {
            w.u8(18);
            w.bool(*recovery_checked);
        }
        AgentCommandKind::CancellationRequested => w.u8(19),
        AgentCommandKind::CancellationFinished => w.u8(20),
        AgentCommandKind::Failed(value) => {
            w.u8(21);
            failure(w, value);
        }
        AgentCommandKind::Exhausted(value) => {
            w.u8(22);
            failure(w, value);
        }
        AgentCommandKind::CompletionCommitted => w.u8(23),
    }
}

fn decode_kind(r: &mut Reader<'_>) -> Result<AgentCommandKind, AgentRejection> {
    Ok(match r.u8()? {
        1 => AgentCommandKind::ContextPrepared(read_context(r)?),
        2 => AgentCommandKind::ModelRequestStarted {
            call_id: ModelCallId::new(r.digest()?)?,
            request_digest: r.digest()?,
        },
        3 => AgentCommandKind::ProviderEventObserved(read_provider(r)?),
        4 => AgentCommandKind::ProviderRetryScheduled(read_retry(r)?),
        5 => {
            let terminal = read_model_terminal(r)?;
            let count = r.bounded_len(usize::from(AgentLimits::HARD_MAX_TOOL_CALLS))?;
            let mut proposals = Vec::with_capacity(count);
            for _ in 0..count {
                proposals.push(read_tool_proposal(r)?);
            }
            AgentCommandKind::ToolCallsProposed { terminal, proposals }
        }
        6 => AgentCommandKind::CompletionProposed {
            terminal: read_model_terminal(r)?,
            proposal: read_completion(r)?,
        },
        7 => AgentCommandKind::AuthorizationStarted,
        8 => AgentCommandKind::ToolAuthorized {
            ordinal: ToolOrdinal::new(r.u16()?),
            authority_digest: r.digest()?,
        },
        9 => AgentCommandKind::ToolDenied {
            ordinal: ToolOrdinal::new(r.u16()?),
            result: read_tool_result(r)?,
        },
        10 => AgentCommandKind::ToolExecutionStarted,
        11 => AgentCommandKind::ToolDispatched { ordinal: ToolOrdinal::new(r.u16()?) },
        12 => AgentCommandKind::ToolActivated { ordinal: ToolOrdinal::new(r.u16()?) },
        13 => AgentCommandKind::ToolProgressObserved {
            ordinal: ToolOrdinal::new(r.u16()?),
            sequence: r.u32()?,
            progress_digest: r.digest()?,
        },
        14 => AgentCommandKind::ToolCompleted {
            ordinal: ToolOrdinal::new(r.u16()?),
            result: read_tool_result(r)?,
        },
        15 => AgentCommandKind::ResultRecordingStarted,
        16 => AgentCommandKind::ResultsRecorded { transcript_digest: r.digest()? },
        17 => AgentCommandKind::Paused,
        18 => AgentCommandKind::Resumed { recovery_checked: r.bool()? },
        19 => AgentCommandKind::CancellationRequested,
        20 => AgentCommandKind::CancellationFinished,
        21 => AgentCommandKind::Failed(read_failure(r)?),
        22 => AgentCommandKind::Exhausted(read_failure(r)?),
        23 => AgentCommandKind::CompletionCommitted,
        _ => return Err(wire_error("unknown command payload tag")),
    })
}

fn context(w: &mut Writer, value: ContextRecord) {
    w.digest(value.render_digest());
    w.digest(value.plan_digest());
    w.digest(value.memory_digest());
    w.digest(value.estimator_digest());
    w.option_digest(value.compaction_digest());
}
fn read_context(r: &mut Reader<'_>) -> Result<ContextRecord, AgentRejection> {
    Ok(ContextRecord::new(r.digest()?, r.digest()?, r.digest()?, r.digest()?, r.option_digest()?))
}
fn provider(w: &mut Writer, value: &ProviderEventRecord) {
    w.u64(value.cursor());
    w.digest(value.event_digest());
    w.u64(value.output_bytes());
    w.bool(value.duplicate());
    w.bytes(value.encoded_envelope());
}
fn read_provider(r: &mut Reader<'_>) -> Result<ProviderEventRecord, AgentRejection> {
    let cursor = r.u64()?;
    let digest = r.digest()?;
    let output_bytes = r.u64()?;
    let duplicate = r.bool()?;
    let envelope = r.bytes(ProviderEventRecord::MAX_ENVELOPE_BYTES)?;
    if envelope.is_empty() {
        Ok(ProviderEventRecord::new(cursor, digest, output_bytes, duplicate))
    } else {
        ProviderEventRecord::with_envelope(cursor, digest, output_bytes, duplicate, envelope)
    }
}
fn retry(w: &mut Writer, value: ProviderRetryRecord) {
    w.digest(value.failure_digest());
    w.digest(value.request_digest());
    match value.class() {
        ProviderRetryClass::ExactResume { cursor } => {
            w.u8(1);
            w.u64(cursor);
        }
        ProviderRetryClass::SafeNewRequest => w.u8(2),
    }
}
fn read_retry(r: &mut Reader<'_>) -> Result<ProviderRetryRecord, AgentRejection> {
    let failure = r.digest()?;
    let request = r.digest()?;
    let class = match r.u8()? {
        1 => ProviderRetryClass::ExactResume { cursor: r.u64()? },
        2 => ProviderRetryClass::SafeNewRequest,
        _ => return Err(wire_error("unknown retry class")),
    };
    Ok(ProviderRetryRecord::new(failure, request, class))
}
fn model_terminal(w: &mut Writer, value: ModelTerminalRecord) {
    w.digest(value.response_digest());
    w.bool(value.normal_terminal());
    w.bool(value.incomplete_items());
    w.bool(value.usage_settled());
}
fn read_model_terminal(r: &mut Reader<'_>) -> Result<ModelTerminalRecord, AgentRejection> {
    Ok(ModelTerminalRecord::new(r.digest()?, r.bool()?, r.bool()?, r.bool()?))
}

fn tool_proposal(w: &mut Writer, value: &ToolProposal) {
    w.u16(value.ordinal().get());
    w.digest(value.model_call_id().digest());
    w.raw(value.action_id().as_bytes());
    w.text(value.capability().as_str());
    w.u16(value.version().major());
    w.u16(value.version().minor());
    w.digest(value.argument_digest());
    w.digest(value.prepared_digest());
    w.digest(value.replay_identity());
    w.revision(value.revision());
    w.u64(value.deadline().epoch().get());
    w.u64(value.deadline().tick_millis());
    w.u8(value.side_effect() as u8);
    w.u8(value.idempotency() as u8);
}
fn read_tool_proposal(r: &mut Reader<'_>) -> Result<ToolProposal, AgentRejection> {
    let ordinal = ToolOrdinal::new(r.u16()?);
    let call = ModelCallId::new(r.digest()?)?;
    let action = r.id(ActionId::new)?;
    let capability = CapabilityName::new(r.text(CapabilityName::MAX_LENGTH)?)
        .map_err(|_| wire_error("invalid capability"))?;
    let version = ToolVersion::new(r.u16()?, r.u16()?)?;
    let argument = r.digest()?;
    let prepared = r.digest()?;
    let replay = r.digest()?;
    let revision = r.revision()?;
    let epoch = Generation::new(r.u64()?).map_err(|_| wire_error("invalid authority epoch"))?;
    let tick = r.u64()?;
    let side_effect = match r.u8()? {
        0 => ToolSideEffect::None,
        1 => ToolSideEffect::Workspace,
        2 => ToolSideEffect::Process,
        3 => ToolSideEffect::External,
        _ => return Err(wire_error("unknown side-effect tag")),
    };
    let idempotency = match r.u8()? {
        0 => ToolIdempotency::Idempotent,
        1 => ToolIdempotency::ReplayTerminalOnly,
        2 => ToolIdempotency::NonIdempotent,
        _ => return Err(wire_error("unknown idempotency tag")),
    };
    Ok(ToolProposal::new(
        ordinal,
        call,
        action,
        capability,
        version,
        argument,
        prepared,
        replay,
        revision,
        AuthorityInstant::new(epoch, tick),
        side_effect,
        idempotency,
    ))
}

fn tool_result(w: &mut Writer, value: &ToolResultRecord) {
    w.u8(value.status() as u8);
    w.digest(value.result_digest());
    w.u64(value.model_visible_bytes());
    w.len(value.evidence().len());
    for id in value.evidence() {
        w.raw(id.as_bytes());
    }
}
fn read_tool_result(r: &mut Reader<'_>) -> Result<ToolResultRecord, AgentRejection> {
    let status = match r.u8()? {
        0 => ToolResultStatus::Succeeded,
        1 => ToolResultStatus::Failed,
        2 => ToolResultStatus::Denied,
        3 => ToolResultStatus::Cancelled,
        4 => ToolResultStatus::Indeterminate,
        _ => return Err(wire_error("unknown tool result tag")),
    };
    let digest = r.digest()?;
    let bytes = r.u64()?;
    let count = r.bounded_len(ToolResultRecord::MAX_EVIDENCE)?;
    let mut evidence = Vec::with_capacity(count);
    for _ in 0..count {
        evidence.push(r.id(EvidenceId::new)?);
    }
    ToolResultRecord::new(status, digest, bytes, evidence)
}

fn completion(w: &mut Writer, value: &CompletionProposal) {
    w.text(value.summary().as_str());
    w.len(value.evidence().len());
    for evidence in value.evidence() {
        w.raw(evidence.id().as_bytes());
        w.revision(evidence.revision());
    }
    w.len(value.uncertainties().len());
    for item in value.uncertainties() {
        w.text(item.as_str());
    }
    w.revision(value.revision());
    w.digest(value.transcripts().context());
    w.digest(value.transcripts().model());
    w.digest(value.transcripts().tools());
    w.u8(value.requested() as u8);
}
fn read_completion(r: &mut Reader<'_>) -> Result<CompletionProposal, AgentRejection> {
    let summary = SafeText::new(r.text(SafeText::MAX_BYTES)?)?;
    let count = r.bounded_len(CompletionProposal::MAX_EVIDENCE)?;
    let mut evidence = Vec::with_capacity(count);
    for _ in 0..count {
        evidence.push(EvidenceReference::new(r.id(EvidenceId::new)?, r.revision()?));
    }
    let count = r.bounded_len(CompletionProposal::MAX_UNCERTAINTIES)?;
    let mut uncertainties = Vec::with_capacity(count);
    for _ in 0..count {
        uncertainties.push(SafeText::new(r.text(SafeText::MAX_BYTES)?)?);
    }
    let revision = r.revision()?;
    let transcripts = TranscriptDigests::new(r.digest()?, r.digest()?, r.digest()?);
    let requested = match r.u8()? {
        0 => CompletionRequest::RunGates,
        1 => CompletionRequest::RequestReview,
        2 => CompletionRequest::ContinueFixing,
        3 => CompletionRequest::RequestAuthority,
        4 => CompletionRequest::ReportBlocked,
        _ => return Err(wire_error("unknown completion request")),
    };
    CompletionProposal::new(summary, evidence, uncertainties, revision, transcripts, requested)
}

fn failure(w: &mut Writer, value: &AgentFailure) {
    match value.kind() {
        AgentFailureKind::Provider => w.u8(0),
        AgentFailureKind::Tool => w.u8(1),
        AgentFailureKind::Context => w.u8(2),
        AgentFailureKind::Protocol => w.u8(3),
        AgentFailureKind::Exhausted(value) => {
            w.u8(4);
            w.u8(value as u8);
        }
        AgentFailureKind::Indeterminate => w.u8(5),
    }
    w.text(value.detail().as_str());
}
fn read_failure(r: &mut Reader<'_>) -> Result<AgentFailure, AgentRejection> {
    let kind = match r.u8()? {
        0 => AgentFailureKind::Provider,
        1 => AgentFailureKind::Tool,
        2 => AgentFailureKind::Context,
        3 => AgentFailureKind::Protocol,
        4 => AgentFailureKind::Exhausted(read_dimension(r.u8()?)?),
        5 => AgentFailureKind::Indeterminate,
        _ => return Err(wire_error("unknown failure kind")),
    };
    Ok(AgentFailure::new(kind, SafeText::new(r.text(SafeText::MAX_BYTES)?)?))
}
const fn read_dimension(tag: u8) -> Result<AgentLimitDimension, AgentRejection> {
    match tag {
        0 => Ok(AgentLimitDimension::ToolCalls),
        1 => Ok(AgentLimitDimension::ProviderEvents),
        2 => Ok(AgentLimitDimension::ContextCycles),
        3 => Ok(AgentLimitDimension::OutputBytes),
        4 => Ok(AgentLimitDimension::ToolResultBytes),
        5 => Ok(AgentLimitDimension::ConcurrentToolCalls),
        6 => Ok(AgentLimitDimension::Transitions),
        _ => Err(wire_error("unknown limit dimension")),
    }
}

struct Writer {
    bytes: Vec<u8>,
}
impl Writer {
    const fn new() -> Self {
        Self { bytes: Vec::new() }
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
    fn bool(&mut self, value: bool) {
        self.u8(u8::from(value));
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
        self.u32(u32::try_from(value).expect("bounded wire length"));
    }
    fn text(&mut self, value: &str) {
        self.len(value.len());
        self.raw(value.as_bytes());
    }
    fn bytes(&mut self, value: &[u8]) {
        self.len(value.len());
        self.raw(value);
    }
    fn digest(&mut self, value: Sha256Digest) {
        self.raw(value.as_bytes());
    }
    fn option_digest(&mut self, value: Option<Sha256Digest>) {
        match value {
            Some(value) => {
                self.u8(1);
                self.digest(value);
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

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl<'a> Reader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }
    const fn finished(&self) -> bool {
        self.offset == self.bytes.len()
    }
    fn take(&mut self, length: usize) -> Result<&'a [u8], AgentRejection> {
        let end =
            self.offset.checked_add(length).ok_or_else(|| wire_error("wire offset overflow"))?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| wire_error("truncated command payload"))?;
        self.offset = end;
        Ok(value)
    }
    fn expect(&mut self, expected: &[u8]) -> Result<(), AgentRejection> {
        if self.take(expected.len())? == expected {
            Ok(())
        } else {
            Err(wire_error("command payload domain mismatch"))
        }
    }
    fn u8(&mut self) -> Result<u8, AgentRejection> {
        Ok(self.take(1)?[0])
    }
    fn bool(&mut self) -> Result<bool, AgentRejection> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(wire_error("invalid boolean")),
        }
    }
    fn u16(&mut self) -> Result<u16, AgentRejection> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().expect("exact slice")))
    }
    fn u32(&mut self) -> Result<u32, AgentRejection> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().expect("exact slice")))
    }
    fn u64(&mut self) -> Result<u64, AgentRejection> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into().expect("exact slice")))
    }
    fn bounded_len(&mut self, max: usize) -> Result<usize, AgentRejection> {
        let value = usize::try_from(self.u32()?)
            .map_err(|_| wire_error("wire length cannot be represented"))?;
        if value <= max { Ok(value) } else { Err(wire_error("wire collection exceeds bound")) }
    }
    fn text(&mut self, max: usize) -> Result<String, AgentRejection> {
        let length = self.bounded_len(max)?;
        String::from_utf8(self.take(length)?.to_vec())
            .map_err(|_| wire_error("wire text is not UTF-8"))
    }
    fn bytes(&mut self, max: usize) -> Result<Vec<u8>, AgentRejection> {
        let length = self.bounded_len(max)?;
        Ok(self.take(length)?.to_vec())
    }
    fn digest(&mut self) -> Result<Sha256Digest, AgentRejection> {
        Ok(Sha256Digest::new(self.take(32)?.try_into().expect("exact slice")))
    }
    fn option_digest(&mut self) -> Result<Option<Sha256Digest>, AgentRejection> {
        match self.u8()? {
            0 => Ok(None),
            1 => self.digest().map(Some),
            _ => Err(wire_error("invalid option tag")),
        }
    }
    fn id<T, E>(
        &mut self,
        make: impl FnOnce([u8; 16]) -> Result<T, E>,
    ) -> Result<T, AgentRejection> {
        make(self.take(16)?.try_into().expect("exact slice"))
            .map_err(|_| wire_error("invalid identifier"))
    }
    fn revision(&mut self) -> Result<RevisionTuple, AgentRejection> {
        Ok(RevisionTuple::new(
            self.id(AcceptanceSpecId::new)?,
            self.id(HarnessId::new)?,
            self.id(WorkspaceId::new)?,
            Generation::new(self.u64()?).map_err(|_| wire_error("invalid generation"))?,
            RevisionNumber::new(self.u64()?).map_err(|_| wire_error("invalid revision"))?,
            self.id(PolicyId::new)?,
            self.id(ProviderProfileId::new)?,
        ))
    }
}

const fn wire_error(detail: &'static str) -> AgentRejection {
    AgentRejection::new(
        AgentErrorCode::ReplayMismatch,
        AgentOperation::Replay,
        AgentRecovery::RestartTurn,
        detail,
    )
}
