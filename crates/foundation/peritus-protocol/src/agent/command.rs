//! Canonical inert D0 command records.

use super::phase::{read_phase, write_phase};
use super::wire::{checked_payload, read_counters, read_payload, write_counters, write_payload};
use super::{AgentCountersDto, AgentPhaseDto};
use crate::SCHEMA_V1;
use crate::primitive::{
    read_digest, read_id, read_option_id, read_revision, write_digest, write_id, write_option_id,
    write_revision,
};
use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
    CodecLimits,
};
use peritus_types::{CommandId, EventId, RevisionTuple, Sha256Digest, TurnId};
use std::fmt;

/// Stable semantic class of one D0 reducer command.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AgentCommandKindDto {
    /// Open the revision-bound inner turn.
    StartTurn,
    /// Accept a prepared context observation.
    RecordContextPlan,
    /// Accept a model request plan before submission.
    RecordModelRequest,
    /// Record that one model attempt was started.
    StartModelAttempt,
    /// Accept one normalized provider event observation.
    ObserveProviderEvent,
    /// Accept the fully reduced provider terminal.
    FinishModelResponse,
    /// Install complete inert model tool-call proposals.
    ProposeToolCalls,
    /// Accept a C4 prepared-call observation.
    PrepareToolCall,
    /// Record that independent authority is required.
    RequestToolAuthorization,
    /// Accept committed independent authority receipts.
    AuthorizeToolCall,
    /// Record an authority denial without an effect.
    DenyToolCall,
    /// Accept a C4 dispatch observation.
    DispatchToolCall,
    /// Accept one C4 terminal or active tool observation.
    ObserveToolResult,
    /// Record stable ordinal model-facing results.
    RecordToolResults,
    /// Record a bounded legal provider retry.
    ScheduleRetry,
    /// Install an inert completion proposal.
    ProposeCompletion,
    /// Complete the inner turn after its durable proposal.
    CompleteTurn,
    /// Pause and retain the exact resumable phase.
    PauseTurn,
    /// Resume after recovery preconditions are checked.
    ResumeTurn,
    /// Begin cancellation of owned work.
    CancelTurn,
    /// Terminate with an explicit failure.
    FailTurn,
    /// Accept an explicit recovery classification.
    ClassifyRecovery,
    /// Terminate after a checked hard limit is crossed.
    ExhaustLimits,
}

/// One complete, syntax-checked but unprivileged agent reducer command record.
#[derive(Clone, Eq, Hash, PartialEq)]
pub struct AgentCommandDto {
    command_id: CommandId,
    event_id: EventId,
    turn_id: TurnId,
    expected_sequence: u64,
    expected_previous_event_id: Option<EventId>,
    revision: RevisionTuple,
    phase: AgentPhaseDto,
    kind: AgentCommandKindDto,
    successor_state_digest: Sha256Digest,
    counters: AgentCountersDto,
    payload_digest: Sha256Digest,
    payload: Vec<u8>,
}

impl fmt::Debug for AgentCommandDto {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentCommandDto")
            .field("command_id", &self.command_id)
            .field("event_id", &self.event_id)
            .field("turn_id", &self.turn_id)
            .field("expected_sequence", &self.expected_sequence)
            .field("expected_previous_event_id", &self.expected_previous_event_id)
            .field("revision", &self.revision)
            .field("phase", &self.phase)
            .field("kind", &self.kind)
            .field("successor_state_digest", &self.successor_state_digest)
            .field("counters", &self.counters)
            .field("payload_digest", &self.payload_digest)
            .field("payload_len", &self.payload.len())
            .finish()
    }
}

impl AgentCommandDto {
    /// Creates a bounded inert command record and binds its exact opaque content digest.
    ///
    /// # Errors
    ///
    /// Returns a codec limit error when the opaque payload exceeds the supplied limits.
    #[allow(clippy::too_many_arguments, reason = "durable replay bindings remain explicit")]
    pub fn new(
        command_id: CommandId,
        event_id: EventId,
        turn_id: TurnId,
        expected_sequence: u64,
        expected_previous_event_id: Option<EventId>,
        revision: RevisionTuple,
        phase: AgentPhaseDto,
        kind: AgentCommandKindDto,
        successor_state_digest: Sha256Digest,
        counters: AgentCountersDto,
        payload: Vec<u8>,
        limits: CodecLimits,
    ) -> Result<Self, CodecError> {
        if (expected_sequence == 0) != expected_previous_event_id.is_none() {
            return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, 0));
        }
        let (payload_digest, payload) = checked_payload(payload, limits)?;
        Ok(Self {
            command_id,
            event_id,
            turn_id,
            expected_sequence,
            expected_previous_event_id,
            revision,
            phase,
            kind,
            successor_state_digest,
            counters,
            payload_digest,
            payload,
        })
    }

    /// Returns the stable command identity.
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
    /// Returns the event identity reserved by this command.
    #[must_use]
    pub const fn event_id(&self) -> EventId {
        self.event_id
    }
    /// Returns the bound B0 turn identity.
    #[must_use]
    pub const fn turn_id(&self) -> TurnId {
        self.turn_id
    }
    /// Returns the expected prior aggregate sequence; zero denotes genesis.
    #[must_use]
    pub const fn expected_sequence(&self) -> u64 {
        self.expected_sequence
    }
    /// Returns the exact expected causal predecessor.
    #[must_use]
    pub const fn expected_previous_event_id(&self) -> Option<EventId> {
        self.expected_previous_event_id
    }
    /// Returns the exact revision binding.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Returns the phase in which the command was accepted.
    #[must_use]
    pub const fn phase(&self) -> AgentPhaseDto {
        self.phase
    }
    /// Returns the stable command kind.
    #[must_use]
    pub const fn kind(&self) -> AgentCommandKindDto {
        self.kind
    }
    /// Returns the claimed successor state digest for checked D0 replay.
    #[must_use]
    pub const fn successor_state_digest(&self) -> Sha256Digest {
        self.successor_state_digest
    }
    /// Returns the successor bounded counters.
    #[must_use]
    pub const fn counters(&self) -> AgentCountersDto {
        self.counters
    }
    /// Returns the exact opaque-content digest.
    #[must_use]
    pub const fn payload_digest(&self) -> Sha256Digest {
        self.payload_digest
    }
    /// Borrows bounded inert command-specific bytes.
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

impl CanonicalEncode for AgentCommandDto {
    const FAMILY: u16 = 40;
    const SCHEMA_VERSION: u16 = SCHEMA_V1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        write_id(writer, self.command_id.as_bytes())?;
        write_id(writer, self.event_id.as_bytes())?;
        write_id(writer, self.turn_id.as_bytes())?;
        writer.write_u64(self.expected_sequence)?;
        write_option_id(writer, self.expected_previous_event_id, EventId::into_bytes)?;
        write_revision(writer, &self.revision)?;
        write_phase(writer, self.phase)?;
        writer.write_u16(command_tag(self.kind))?;
        write_digest(writer, &self.successor_state_digest)?;
        write_counters(writer, self.counters)?;
        write_payload(writer, self.payload_digest, &self.payload)
    }
}

impl CanonicalDecode for AgentCommandDto {
    const FAMILY: u16 = 40;
    const SCHEMA_VERSION: u16 = SCHEMA_V1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let command_id = read_id(reader, CommandId::new)?;
        let event_id = read_id(reader, EventId::new)?;
        let turn_id = read_id(reader, TurnId::new)?;
        let expected_sequence = reader.read_u64()?;
        let expected_previous_event_id = read_option_id(reader, EventId::new)?;
        if (expected_sequence == 0) != expected_previous_event_id.is_none() {
            return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, 0));
        }
        let revision = read_revision(reader)?;
        let phase = read_phase(reader)?;
        let kind = read_command_kind(reader)?;
        let successor_state_digest = read_digest(reader)?;
        let counters = read_counters(reader)?;
        let (payload_digest, payload) = read_payload(reader)?;
        Ok(Self {
            command_id,
            event_id,
            turn_id,
            expected_sequence,
            expected_previous_event_id,
            revision,
            phase,
            kind,
            successor_state_digest,
            counters,
            payload_digest,
            payload,
        })
    }
}

const fn command_tag(kind: AgentCommandKindDto) -> u16 {
    use AgentCommandKindDto as K;
    match kind {
        K::StartTurn => 1,
        K::RecordContextPlan => 2,
        K::RecordModelRequest => 3,
        K::StartModelAttempt => 4,
        K::ObserveProviderEvent => 5,
        K::FinishModelResponse => 6,
        K::ProposeToolCalls => 7,
        K::PrepareToolCall => 8,
        K::RequestToolAuthorization => 9,
        K::AuthorizeToolCall => 10,
        K::DenyToolCall => 11,
        K::DispatchToolCall => 12,
        K::ObserveToolResult => 13,
        K::RecordToolResults => 14,
        K::ScheduleRetry => 15,
        K::ProposeCompletion => 16,
        K::CompleteTurn => 17,
        K::PauseTurn => 18,
        K::ResumeTurn => 19,
        K::CancelTurn => 20,
        K::FailTurn => 21,
        K::ClassifyRecovery => 22,
        K::ExhaustLimits => 23,
    }
}

fn read_command_kind(reader: &mut CanonicalReader<'_>) -> Result<AgentCommandKindDto, CodecError> {
    use AgentCommandKindDto as K;
    let offset = reader.offset();
    match reader.read_u16()? {
        1 => Ok(K::StartTurn),
        2 => Ok(K::RecordContextPlan),
        3 => Ok(K::RecordModelRequest),
        4 => Ok(K::StartModelAttempt),
        5 => Ok(K::ObserveProviderEvent),
        6 => Ok(K::FinishModelResponse),
        7 => Ok(K::ProposeToolCalls),
        8 => Ok(K::PrepareToolCall),
        9 => Ok(K::RequestToolAuthorization),
        10 => Ok(K::AuthorizeToolCall),
        11 => Ok(K::DenyToolCall),
        12 => Ok(K::DispatchToolCall),
        13 => Ok(K::ObserveToolResult),
        14 => Ok(K::RecordToolResults),
        15 => Ok(K::ScheduleRetry),
        16 => Ok(K::ProposeCompletion),
        17 => Ok(K::CompleteTurn),
        18 => Ok(K::PauseTurn),
        19 => Ok(K::ResumeTurn),
        20 => Ok(K::CancelTurn),
        21 => Ok(K::FailTurn),
        22 => Ok(K::ClassifyRecovery),
        23 => Ok(K::ExhaustLimits),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}
