//! Canonical inert D0 event records.

use super::phase::{read_phase, write_phase};
use super::wire::{read_counters, read_payload, write_counters, write_payload};
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
use peritus_types::{CommandId, EventId, EventSequence, RevisionTuple, Sha256Digest, TurnId};
use std::fmt;

/// Stable semantic class of one successful D0 reducer event.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AgentEventKindDto {
    /// The revision-bound turn was opened.
    TurnStarted,
    /// A context/render plan was committed.
    ContextPlanRecorded,
    /// An exact provider request plan was committed.
    ModelRequestRecorded,
    /// A model submission attempt was observed.
    ModelAttemptStarted,
    /// One normalized provider event was observed.
    ProviderEventObserved,
    /// A complete provider response was reduced.
    ModelResponseFinished,
    /// Complete inert tool calls were proposed.
    ToolCallsProposed,
    /// A tool call was prepared through C4.
    ToolCallPrepared,
    /// A tool call entered independent authorization.
    ToolAuthorizationRequested,
    /// Committed authority was observed.
    ToolCallAuthorized,
    /// Authority was denied without dispatch.
    ToolCallDenied,
    /// A C4 dispatch was observed.
    ToolCallDispatched,
    /// A C4 tool observation was recorded.
    ToolResultObserved,
    /// Tool results were stabilized in proposal order.
    ToolResultsRecorded,
    /// A bounded legal provider retry was scheduled.
    RetryScheduled,
    /// A non-accepting completion proposal was installed.
    CompletionProposed,
    /// The inner turn completed.
    TurnCompleted,
    /// The turn paused while retaining its prior phase.
    TurnPaused,
    /// The paused turn resumed.
    TurnResumed,
    /// Cancellation began while effects are reconciled.
    TurnCancellationStarted,
    /// The turn reached cancelled terminal state.
    TurnCancelled,
    /// The turn reached failed terminal state.
    TurnFailed,
    /// An outstanding effect received an explicit recovery classification.
    EffectRecoveryClassified,
    /// A hard local or B1 limit was exhausted.
    LimitsExhausted,
}

/// One immutable D0 event decoded as data, never as a durable commit receipt.
#[derive(Clone, Eq, Hash, PartialEq)]
pub struct AgentEventDto {
    id: EventId,
    command_id: CommandId,
    sequence: EventSequence,
    previous_event_id: Option<EventId>,
    turn_id: TurnId,
    revision: RevisionTuple,
    phase: AgentPhaseDto,
    kind: AgentEventKindDto,
    successor_state_digest: Sha256Digest,
    counters: AgentCountersDto,
    payload_digest: Sha256Digest,
    payload: Vec<u8>,
}

impl fmt::Debug for AgentEventDto {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentEventDto")
            .field("id", &self.id)
            .field("command_id", &self.command_id)
            .field("sequence", &self.sequence)
            .field("previous_event_id", &self.previous_event_id)
            .field("turn_id", &self.turn_id)
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

impl AgentEventDto {
    /// Creates a bounded event and binds the exact inert event-specific payload.
    ///
    /// # Errors
    ///
    /// Returns a codec limit error when the opaque payload exceeds the supplied limits.
    #[allow(clippy::too_many_arguments, reason = "durable replay bindings remain explicit")]
    pub fn new(
        id: EventId,
        command_id: CommandId,
        sequence: EventSequence,
        previous_event_id: Option<EventId>,
        turn_id: TurnId,
        revision: RevisionTuple,
        phase: AgentPhaseDto,
        kind: AgentEventKindDto,
        successor_state_digest: Sha256Digest,
        counters: AgentCountersDto,
        payload: Vec<u8>,
        limits: CodecLimits,
    ) -> Result<Self, CodecError> {
        if (sequence.get() == 1) != previous_event_id.is_none() {
            return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, 0));
        }
        let (payload_digest, payload) = super::wire::checked_payload(payload, limits)?;
        Ok(Self {
            id,
            command_id,
            sequence,
            previous_event_id,
            turn_id,
            revision,
            phase,
            kind,
            successor_state_digest,
            counters,
            payload_digest,
            payload,
        })
    }

    /// Returns the event identity.
    #[must_use]
    pub const fn id(&self) -> EventId {
        self.id
    }
    /// Returns the causative command identity.
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
    /// Returns the positive agent aggregate sequence.
    #[must_use]
    pub const fn sequence(&self) -> EventSequence {
        self.sequence
    }
    /// Returns the exact causal predecessor.
    #[must_use]
    pub const fn previous_event_id(&self) -> Option<EventId> {
        self.previous_event_id
    }
    /// Returns the bound B0 turn identity.
    #[must_use]
    pub const fn turn_id(&self) -> TurnId {
        self.turn_id
    }
    /// Returns the immutable revision tuple.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Returns the successor phase.
    #[must_use]
    pub const fn phase(&self) -> AgentPhaseDto {
        self.phase
    }
    /// Returns the stable event kind.
    #[must_use]
    pub const fn kind(&self) -> AgentEventKindDto {
        self.kind
    }
    /// Returns the reducer-computed successor state digest claim.
    #[must_use]
    pub const fn successor_state_digest(&self) -> Sha256Digest {
        self.successor_state_digest
    }
    /// Returns the successor bounded counters.
    #[must_use]
    pub const fn counters(&self) -> AgentCountersDto {
        self.counters
    }
    /// Returns the domain-separated opaque payload digest.
    #[must_use]
    pub const fn payload_digest(&self) -> Sha256Digest {
        self.payload_digest
    }
    /// Borrows bounded inert event-specific bytes.
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

impl CanonicalEncode for AgentEventDto {
    const FAMILY: u16 = 41;
    const SCHEMA_VERSION: u16 = SCHEMA_V1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        write_id(writer, self.id.as_bytes())?;
        write_id(writer, self.command_id.as_bytes())?;
        writer.write_u64(self.sequence.get())?;
        write_option_id(writer, self.previous_event_id, EventId::into_bytes)?;
        write_id(writer, self.turn_id.as_bytes())?;
        write_revision(writer, &self.revision)?;
        write_phase(writer, self.phase)?;
        writer.write_u16(event_tag(self.kind))?;
        write_digest(writer, &self.successor_state_digest)?;
        write_counters(writer, self.counters)?;
        write_payload(writer, self.payload_digest, &self.payload)
    }
}

impl CanonicalDecode for AgentEventDto {
    const FAMILY: u16 = 41;
    const SCHEMA_VERSION: u16 = SCHEMA_V1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let id = read_id(reader, EventId::new)?;
        let command_id = read_id(reader, CommandId::new)?;
        let sequence_offset = reader.offset();
        let sequence = EventSequence::new(reader.read_u64()?)
            .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, sequence_offset))?;
        let previous_event_id = read_option_id(reader, EventId::new)?;
        if (sequence.get() == 1) != previous_event_id.is_none() {
            return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, sequence_offset));
        }
        let turn_id = read_id(reader, TurnId::new)?;
        let revision = read_revision(reader)?;
        let phase = read_phase(reader)?;
        let kind = read_event_kind(reader)?;
        let successor_state_digest = read_digest(reader)?;
        let counters = read_counters(reader)?;
        let (payload_digest, payload) = read_payload(reader)?;
        Ok(Self {
            id,
            command_id,
            sequence,
            previous_event_id,
            turn_id,
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

const fn event_tag(kind: AgentEventKindDto) -> u16 {
    use AgentEventKindDto as K;
    match kind {
        K::TurnStarted => 1,
        K::ContextPlanRecorded => 2,
        K::ModelRequestRecorded => 3,
        K::ModelAttemptStarted => 4,
        K::ProviderEventObserved => 5,
        K::ModelResponseFinished => 6,
        K::ToolCallsProposed => 7,
        K::ToolCallPrepared => 8,
        K::ToolAuthorizationRequested => 9,
        K::ToolCallAuthorized => 10,
        K::ToolCallDenied => 11,
        K::ToolCallDispatched => 12,
        K::ToolResultObserved => 13,
        K::ToolResultsRecorded => 14,
        K::RetryScheduled => 15,
        K::CompletionProposed => 16,
        K::TurnCompleted => 17,
        K::TurnPaused => 18,
        K::TurnResumed => 19,
        K::TurnCancellationStarted => 20,
        K::TurnCancelled => 21,
        K::TurnFailed => 22,
        K::EffectRecoveryClassified => 23,
        K::LimitsExhausted => 24,
    }
}

fn read_event_kind(reader: &mut CanonicalReader<'_>) -> Result<AgentEventKindDto, CodecError> {
    use AgentEventKindDto as K;
    let offset = reader.offset();
    match reader.read_u16()? {
        1 => Ok(K::TurnStarted),
        2 => Ok(K::ContextPlanRecorded),
        3 => Ok(K::ModelRequestRecorded),
        4 => Ok(K::ModelAttemptStarted),
        5 => Ok(K::ProviderEventObserved),
        6 => Ok(K::ModelResponseFinished),
        7 => Ok(K::ToolCallsProposed),
        8 => Ok(K::ToolCallPrepared),
        9 => Ok(K::ToolAuthorizationRequested),
        10 => Ok(K::ToolCallAuthorized),
        11 => Ok(K::ToolCallDenied),
        12 => Ok(K::ToolCallDispatched),
        13 => Ok(K::ToolResultObserved),
        14 => Ok(K::ToolResultsRecorded),
        15 => Ok(K::RetryScheduled),
        16 => Ok(K::CompletionProposed),
        17 => Ok(K::TurnCompleted),
        18 => Ok(K::TurnPaused),
        19 => Ok(K::TurnResumed),
        20 => Ok(K::TurnCancellationStarted),
        21 => Ok(K::TurnCancelled),
        22 => Ok(K::TurnFailed),
        23 => Ok(K::EffectRecoveryClassified),
        24 => Ok(K::LimitsExhausted),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}
