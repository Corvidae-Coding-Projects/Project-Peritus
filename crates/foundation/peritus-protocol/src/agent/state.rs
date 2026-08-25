//! Canonical inert D0 state checkpoints.

use super::AgentPhaseDto;
use super::phase::{read_phase, write_phase};
use super::wire::{read_counters, read_payload, write_counters, write_payload};
use crate::SCHEMA_V1;
use crate::primitive::{
    read_digest, read_id, read_revision, read_role, write_digest, write_id, write_revision,
    write_role,
};
use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
    CodecLimits,
};
use peritus_policy::ActorRole;
use peritus_types::{
    ActorId, AttemptId, EnvironmentId, EventId, EventSequence, RevisionNumber, RevisionTuple,
    SessionId, Sha256Digest, TurnId,
};
use std::fmt;

/// Checked monotonic D0-local structural counters not represented by B1.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct AgentCountersDto {
    tool_calls: u64,
    provider_events: u64,
    context_cycles: u64,
    output_bytes: u64,
    tool_result_bytes: u64,
    concurrent_calls_high_water: u64,
    transitions: u64,
}

impl AgentCountersDto {
    /// Creates an inert observation of all seven D0-local counters.
    #[allow(clippy::too_many_arguments, reason = "the closed counter vocabulary is explicit")]
    #[must_use]
    pub const fn new(
        tool_calls: u64,
        provider_events: u64,
        context_cycles: u64,
        output_bytes: u64,
        tool_result_bytes: u64,
        concurrent_calls_high_water: u64,
        transitions: u64,
    ) -> Self {
        Self {
            tool_calls,
            provider_events,
            context_cycles,
            output_bytes,
            tool_result_bytes,
            concurrent_calls_high_water,
            transitions,
        }
    }

    /// Returns proposed tool calls consumed.
    #[must_use]
    pub const fn tool_calls(self) -> u64 {
        self.tool_calls
    }
    /// Returns normalized provider events observed.
    #[must_use]
    pub const fn provider_events(self) -> u64 {
        self.provider_events
    }
    /// Returns context-planning cycles consumed.
    #[must_use]
    pub const fn context_cycles(self) -> u64 {
        self.context_cycles
    }
    /// Returns model output bytes observed.
    #[must_use]
    pub const fn output_bytes(self) -> u64 {
        self.output_bytes
    }
    /// Returns model-facing tool-result bytes recorded.
    #[must_use]
    pub const fn tool_result_bytes(self) -> u64 {
        self.tool_result_bytes
    }
    /// Returns the high-water number of concurrent calls.
    #[must_use]
    pub const fn concurrent_calls_high_water(self) -> u64 {
        self.concurrent_calls_high_water
    }
    /// Returns successful D0 transitions consumed.
    #[must_use]
    pub const fn transitions(self) -> u64 {
        self.transitions
    }
}

/// One complete durable agent-state checkpoint decoded as inert data.
#[derive(Clone, Eq, Hash, PartialEq)]
pub struct AgentStateDto {
    turn_id: TurnId,
    attempt_id: AttemptId,
    actor_id: ActorId,
    role: ActorRole,
    session_id: SessionId,
    environment_id: EnvironmentId,
    revision: RevisionTuple,
    provider_profile_revision: RevisionNumber,
    agent_limits_revision: RevisionNumber,
    logical_revision: RevisionNumber,
    aggregate_sequence: EventSequence,
    last_event_id: EventId,
    phase: AgentPhaseDto,
    counters: AgentCountersDto,
    state_digest: Sha256Digest,
    payload_digest: Sha256Digest,
    payload: Vec<u8>,
}

impl fmt::Debug for AgentStateDto {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentStateDto")
            .field("turn_id", &self.turn_id)
            .field("attempt_id", &self.attempt_id)
            .field("actor_id", &self.actor_id)
            .field("role", &self.role)
            .field("session_id", &self.session_id)
            .field("environment_id", &self.environment_id)
            .field("revision", &self.revision)
            .field("provider_profile_revision", &self.provider_profile_revision)
            .field("agent_limits_revision", &self.agent_limits_revision)
            .field("logical_revision", &self.logical_revision)
            .field("aggregate_sequence", &self.aggregate_sequence)
            .field("last_event_id", &self.last_event_id)
            .field("phase", &self.phase)
            .field("counters", &self.counters)
            .field("state_digest", &self.state_digest)
            .field("payload_digest", &self.payload_digest)
            .field("payload_len", &self.payload.len())
            .finish()
    }
}

impl AgentStateDto {
    /// Creates a bounded inert state checkpoint with every immutable turn binding explicit.
    ///
    /// # Errors
    ///
    /// Returns a codec limit error when the opaque payload exceeds the supplied limits.
    #[allow(clippy::too_many_arguments, reason = "durable turn bindings remain explicit")]
    pub fn new(
        turn_id: TurnId,
        attempt_id: AttemptId,
        actor_id: ActorId,
        role: ActorRole,
        session_id: SessionId,
        environment_id: EnvironmentId,
        revision: RevisionTuple,
        provider_profile_revision: RevisionNumber,
        agent_limits_revision: RevisionNumber,
        logical_revision: RevisionNumber,
        aggregate_sequence: EventSequence,
        last_event_id: EventId,
        phase: AgentPhaseDto,
        counters: AgentCountersDto,
        state_digest: Sha256Digest,
        payload: Vec<u8>,
        limits: CodecLimits,
    ) -> Result<Self, CodecError> {
        let (payload_digest, payload) = super::wire::checked_payload(payload, limits)?;
        Ok(Self {
            turn_id,
            attempt_id,
            actor_id,
            role,
            session_id,
            environment_id,
            revision,
            provider_profile_revision,
            agent_limits_revision,
            logical_revision,
            aggregate_sequence,
            last_event_id,
            phase,
            counters,
            state_digest,
            payload_digest,
            payload,
        })
    }

    /// Returns the bound B0 turn.
    #[must_use]
    pub const fn turn_id(&self) -> TurnId {
        self.turn_id
    }
    /// Returns the parent B0 attempt.
    #[must_use]
    pub const fn attempt_id(&self) -> AttemptId {
        self.attempt_id
    }
    /// Returns the acting principal.
    #[must_use]
    pub const fn actor_id(&self) -> ActorId {
        self.actor_id
    }
    /// Returns the canonical actor role.
    #[must_use]
    pub const fn role(&self) -> ActorRole {
        self.role
    }
    /// Returns the bound session.
    #[must_use]
    pub const fn session_id(&self) -> SessionId {
        self.session_id
    }
    /// Returns the bound environment.
    #[must_use]
    pub const fn environment_id(&self) -> EnvironmentId {
        self.environment_id
    }
    /// Returns the immutable authority/evidence revision.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Returns the immutable provider profile revision.
    #[must_use]
    pub const fn provider_profile_revision(&self) -> RevisionNumber {
        self.provider_profile_revision
    }
    /// Returns the immutable D0 limits revision.
    #[must_use]
    pub const fn agent_limits_revision(&self) -> RevisionNumber {
        self.agent_limits_revision
    }
    /// Returns the positive logical state revision.
    #[must_use]
    pub const fn logical_revision(&self) -> RevisionNumber {
        self.logical_revision
    }
    /// Returns the positive agent aggregate sequence.
    #[must_use]
    pub const fn aggregate_sequence(&self) -> EventSequence {
        self.aggregate_sequence
    }
    /// Returns the exact last applied event.
    #[must_use]
    pub const fn last_event_id(&self) -> EventId {
        self.last_event_id
    }
    /// Returns the detailed inner-turn phase.
    #[must_use]
    pub const fn phase(&self) -> AgentPhaseDto {
        self.phase
    }
    /// Returns all D0-local structural counters.
    #[must_use]
    pub const fn counters(&self) -> AgentCountersDto {
        self.counters
    }
    /// Returns the reducer-computed canonical state digest claim.
    #[must_use]
    pub const fn state_digest(&self) -> Sha256Digest {
        self.state_digest
    }
    /// Returns the exact digest of bounded opaque checkpoint data.
    #[must_use]
    pub const fn payload_digest(&self) -> Sha256Digest {
        self.payload_digest
    }
    /// Borrows bounded inert reducer-specific checkpoint bytes.
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

impl CanonicalEncode for AgentStateDto {
    const FAMILY: u16 = 42;
    const SCHEMA_VERSION: u16 = SCHEMA_V1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        write_id(writer, self.turn_id.as_bytes())?;
        write_id(writer, self.attempt_id.as_bytes())?;
        write_id(writer, self.actor_id.as_bytes())?;
        write_role(writer, self.role)?;
        write_id(writer, self.session_id.as_bytes())?;
        write_id(writer, self.environment_id.as_bytes())?;
        write_revision(writer, &self.revision)?;
        writer.write_u64(self.provider_profile_revision.get())?;
        writer.write_u64(self.agent_limits_revision.get())?;
        writer.write_u64(self.logical_revision.get())?;
        writer.write_u64(self.aggregate_sequence.get())?;
        write_id(writer, self.last_event_id.as_bytes())?;
        write_phase(writer, self.phase)?;
        write_counters(writer, self.counters)?;
        write_digest(writer, &self.state_digest)?;
        write_payload(writer, self.payload_digest, &self.payload)
    }
}

impl CanonicalDecode for AgentStateDto {
    const FAMILY: u16 = 42;
    const SCHEMA_VERSION: u16 = SCHEMA_V1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let turn_id = read_id(reader, TurnId::new)?;
        let attempt_id = read_id(reader, AttemptId::new)?;
        let actor_id = read_id(reader, ActorId::new)?;
        let role = read_role(reader)?;
        let session_id = read_id(reader, SessionId::new)?;
        let environment_id = read_id(reader, EnvironmentId::new)?;
        let revision = read_revision(reader)?;
        let provider_profile_revision = read_positive_revision(reader)?;
        let agent_limits_revision = read_positive_revision(reader)?;
        let logical_revision = read_positive_revision(reader)?;
        let sequence_offset = reader.offset();
        let aggregate_sequence = EventSequence::new(reader.read_u64()?)
            .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, sequence_offset))?;
        let last_event_id = read_id(reader, EventId::new)?;
        let phase = read_phase(reader)?;
        let counters = read_counters(reader)?;
        let state_digest = read_digest(reader)?;
        let (payload_digest, payload) = read_payload(reader)?;
        Ok(Self {
            turn_id,
            attempt_id,
            actor_id,
            role,
            session_id,
            environment_id,
            revision,
            provider_profile_revision,
            agent_limits_revision,
            logical_revision,
            aggregate_sequence,
            last_event_id,
            phase,
            counters,
            state_digest,
            payload_digest,
            payload,
        })
    }
}

fn read_positive_revision(reader: &mut CanonicalReader<'_>) -> Result<RevisionNumber, CodecError> {
    let offset = reader.offset();
    RevisionNumber::new(reader.read_u64()?)
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))
}
