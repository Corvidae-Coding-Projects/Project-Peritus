//! Latest durable D0 inner-turn observations over canonical agent-event records.

use crate::encoding::{put_digest, put_key, put_u16, put_u64};
use crate::lifecycle::{invalid_frame, invariant, schema};
use crate::{FoldContext, Projection, ProjectionError, ProjectionSchema, ProjectionState};
use peritus_codec::{CodecLimits, decode_message, sha256};
use peritus_journal::{AggregateKey, AggregateKind};
use peritus_protocol::{
    AgentCountersDto, AgentEventDto, AgentEventKindDto, AgentPhaseDto, AgentResumablePhaseDto,
};
use peritus_types::Sha256Digest;
use std::collections::BTreeMap;

const FAMILY: u16 = 41;

/// Latest replay-checked observation for one durable D0 turn.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AgentEntry {
    last_position: u64,
    sequence: u64,
    event_kind: u16,
    phase: AgentPhaseDto,
    successor_state_digest: Sha256Digest,
    frame_digest: Sha256Digest,
    revision_digest: Sha256Digest,
    counters: AgentCountersDto,
}

impl AgentEntry {
    /// Returns the last journal position applied to this turn.
    #[must_use]
    pub const fn last_position(self) -> u64 {
        self.last_position
    }
    /// Returns the positive D0 aggregate sequence.
    #[must_use]
    pub const fn sequence(self) -> u64 {
        self.sequence
    }
    /// Returns the stable agent-event discriminant.
    #[must_use]
    pub const fn event_kind(self) -> u16 {
        self.event_kind
    }
    /// Returns the detailed successor phase.
    #[must_use]
    pub const fn phase(self) -> AgentPhaseDto {
        self.phase
    }
    /// Returns the reducer-computed successor state digest claim.
    #[must_use]
    pub const fn successor_state_digest(self) -> Sha256Digest {
        self.successor_state_digest
    }
    /// Returns the latest D0-local bounded counters.
    #[must_use]
    pub const fn counters(self) -> AgentCountersDto {
        self.counters
    }
}

/// Deterministic latest-state catalog for Agent aggregates.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AgentState {
    entries: BTreeMap<AggregateKey, AgentEntry>,
}

impl AgentState {
    /// Returns the number of observed D0 turn aggregates.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    /// Returns whether no D0 turn aggregate was observed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    /// Looks up the latest checked observation for one exact aggregate.
    #[must_use]
    pub fn get(&self, key: AggregateKey) -> Option<AgentEntry> {
        self.entries.get(&key).copied()
    }
}

impl ProjectionState for AgentState {
    fn encode(&self) -> Vec<u8> {
        let mut bytes = b"peritus-agent-projection-v1\0".to_vec();
        put_u64(&mut bytes, self.entries.len() as u64);
        for (key, entry) in &self.entries {
            put_key(&mut bytes, *key);
            put_u64(&mut bytes, entry.last_position);
            put_u64(&mut bytes, entry.sequence);
            put_u16(&mut bytes, entry.event_kind);
            put_phase(&mut bytes, entry.phase);
            put_digest(&mut bytes, entry.successor_state_digest);
            put_digest(&mut bytes, entry.frame_digest);
            put_digest(&mut bytes, entry.revision_digest);
            put_counters(&mut bytes, entry.counters);
        }
        bytes
    }

    fn validate(&self) -> Result<(), ProjectionError> {
        if self.entries.iter().any(|(key, entry)| {
            key.kind() != AggregateKind::Agent
                || entry.last_position == 0
                || entry.sequence == 0
                || entry.event_kind == 0
        }) {
            return Err(invariant("invalid agent projection entry"));
        }
        Ok(())
    }

    fn invariant_digest(&self) -> Sha256Digest {
        let mut bytes = b"peritus-agent-projection-invariants-v1\0".to_vec();
        bytes.extend_from_slice(&self.encode());
        sha256(&bytes)
    }
}

/// Version-one projection of the latest durable observation for every D0 turn.
#[derive(Clone, Debug)]
pub struct AgentProjection {
    schema: ProjectionSchema,
}

impl AgentProjection {
    /// Creates the frozen version-one D0 agent projection schema.
    ///
    /// # Errors
    ///
    /// Returns an identity error only if the built-in schema constants are invalid.
    pub fn new() -> Result<Self, ProjectionError> {
        schema(
            "agent-turns",
            b"agent-event:v1;turn-derived-aggregate;latest-phase;successor-digest;counters",
        )
        .map(|schema| Self { schema })
    }
}

impl Projection for AgentProjection {
    type State = AgentState;

    fn schema(&self) -> &ProjectionSchema {
        &self.schema
    }
    fn genesis(&self) -> Self::State {
        AgentState::default()
    }

    fn fold(&self, state: &mut Self::State, input: FoldContext<'_>) -> Result<(), ProjectionError> {
        if input.family() != FAMILY {
            return Ok(());
        }
        let record = input.record();
        if record.aggregate().kind() != AggregateKind::Agent {
            return Err(invariant("agent-event frame belongs to a non-agent aggregate"));
        }
        let event = decode_message::<AgentEventDto>(input.frame_bytes(), CodecLimits::PRODUCTION)
            .map_err(|_| invalid_frame("decode agent event"))?;
        let record_event_id = record.event_id();
        let envelope_matches = event.id() == record_event_id
            && event.command_id() == record.command_id()
            && event.sequence() == record.sequence()
            && event.previous_event_id() == record.previous_event_id()
            && event.turn_id().as_bytes() == record.aggregate().id().as_bytes();
        if !envelope_matches {
            return Err(invariant("agent-event payload disagrees with journal envelope"));
        }
        state.entries.insert(
            record.aggregate(),
            AgentEntry {
                last_position: record.global_position(),
                sequence: record.sequence().get(),
                event_kind: event_tag(event.kind()),
                phase: event.phase(),
                successor_state_digest: event.successor_state_digest(),
                frame_digest: record.frame_digest(),
                revision_digest: record.revision_digest(),
                counters: event.counters(),
            },
        );
        Ok(())
    }
}

fn put_phase(bytes: &mut Vec<u8>, phase: AgentPhaseDto) {
    match phase {
        AgentPhaseDto::Active(value) => {
            put_u16(bytes, resumable_tag(value));
            put_u16(bytes, 0);
        }
        AgentPhaseDto::Paused(value) => {
            put_u16(bytes, 9);
            put_u16(bytes, resumable_tag(value));
        }
        AgentPhaseDto::Cancelling => {
            put_u16(bytes, 10);
            put_u16(bytes, 0);
        }
        AgentPhaseDto::Completed => {
            put_u16(bytes, 11);
            put_u16(bytes, 0);
        }
        AgentPhaseDto::Failed => {
            put_u16(bytes, 12);
            put_u16(bytes, 0);
        }
        AgentPhaseDto::Cancelled => {
            put_u16(bytes, 13);
            put_u16(bytes, 0);
        }
    }
}

fn put_counters(bytes: &mut Vec<u8>, value: AgentCountersDto) {
    for counter in [
        value.tool_calls(),
        value.provider_events(),
        value.context_cycles(),
        value.output_bytes(),
        value.tool_result_bytes(),
        value.concurrent_calls_high_water(),
        value.transitions(),
    ] {
        put_u64(bytes, counter);
    }
}

const fn resumable_tag(phase: AgentResumablePhaseDto) -> u16 {
    match phase {
        AgentResumablePhaseDto::PreparingContext => 1,
        AgentResumablePhaseDto::RequestingModel => 2,
        AgentResumablePhaseDto::StreamingResponse => 3,
        AgentResumablePhaseDto::ProposedToolCalls => 4,
        AgentResumablePhaseDto::AwaitingAuthorization => 5,
        AgentResumablePhaseDto::ExecutingTools => 6,
        AgentResumablePhaseDto::RecordingResults => 7,
        AgentResumablePhaseDto::ProposedCompletion => 8,
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
