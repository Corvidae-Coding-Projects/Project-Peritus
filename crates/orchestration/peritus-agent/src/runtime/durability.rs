//! Atomic B3/C0 persistence and restart loading for one D0 aggregate.

use core::fmt;

use peritus_codec::{CodecError, CodecLimits, decode_message, encode_message, sha256};
use peritus_evidence::revision_digest;
use peritus_journal::{
    AggregateId, AggregateKey, AggregateKind, AppendRequest, CommittedBatch, EventDraft,
    ExactFrame, HeadExpectation, JournalError, SqliteJournal, StateInstall,
};
use peritus_protocol::{
    AgentCommandDto, AgentCommandKindDto, AgentEventDto, AgentEventKindDto, AgentStateDto,
};
use peritus_types::TurnId;

/// Journal-owned namespace for current D0 aggregate checkpoints.
pub const AGENT_STATE_NAMESPACE: u16 = 0xD001;
const STATE_KEY_DOMAIN: &[u8] = b"peritus.agent.state.v1\0";

/// Replay material loaded from one checked C0 D0 aggregate.
pub struct AgentReplay {
    events: Vec<AgentEventDto>,
    checkpoint: Option<AgentStateDto>,
}

impl AgentReplay {
    /// Borrows canonical events in contiguous aggregate order.
    #[must_use]
    pub fn events(&self) -> &[AgentEventDto] {
        &self.events
    }
    /// Borrows the current atomically installed checkpoint when the aggregate exists.
    #[must_use]
    pub const fn checkpoint(&self) -> Option<&AgentStateDto> {
        self.checkpoint.as_ref()
    }
}

impl fmt::Debug for AgentReplay {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentReplay")
            .field("events", &self.events.len())
            .field(
                "checkpoint_sequence",
                &self.checkpoint.as_ref().map(AgentStateDto::aggregate_sequence),
            )
            .finish_non_exhaustive()
    }
}

/// Typed failure from D0 canonical persistence or replay loading.
#[derive(Debug)]
#[non_exhaustive]
pub enum AgentDurabilityError {
    /// Command, event, checkpoint, head, and state bindings do not describe one transition.
    BindingMismatch,
    /// C0 contains an agent head without state, state without a head, or a stale checkpoint.
    InconsistentDurability,
    /// B3 canonical encoding or decoding failed.
    Codec(CodecError),
    /// C0 planning, storage, or integrity validation failed.
    Journal(JournalError),
}

impl fmt::Display for AgentDurabilityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BindingMismatch => formatter
                .write_str("agent command, event, checkpoint, or aggregate head binding differs"),
            Self::InconsistentDurability => {
                formatter.write_str("agent journal head and checkpoint are inconsistent")
            }
            Self::Codec(error) => fmt::Display::fmt(error, formatter),
            Self::Journal(error) => fmt::Display::fmt(error, formatter),
        }
    }
}

impl std::error::Error for AgentDurabilityError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Codec(error) => Some(error),
            Self::Journal(error) => Some(error),
            Self::BindingMismatch | Self::InconsistentDurability => None,
        }
    }
}

impl From<CodecError> for AgentDurabilityError {
    fn from(value: CodecError) -> Self {
        Self::Codec(value)
    }
}

impl From<JournalError> for AgentDurabilityError {
    fn from(value: JournalError) -> Self {
        Self::Journal(value)
    }
}

/// Derives the dedicated C0 agent aggregate identity from the exact B0 turn identity.
///
/// # Errors
///
/// Rejects the reserved zero identity (which is also rejected by `TurnId`).
pub fn agent_aggregate_key(turn_id: TurnId) -> Result<AggregateKey, AgentDurabilityError> {
    let id = AggregateId::new(*turn_id.as_bytes())?;
    Ok(AggregateKey::new(AggregateKind::Agent, id))
}

/// Derives the stable journal-owned checkpoint key for one turn.
#[must_use]
pub fn agent_state_key(turn_id: TurnId) -> Vec<u8> {
    let mut key = Vec::with_capacity(STATE_KEY_DOMAIN.len() + 16);
    key.extend_from_slice(STATE_KEY_DOMAIN);
    key.extend_from_slice(turn_id.as_bytes());
    key
}

/// Atomically appends one accepted D0 event and replaces its complete current checkpoint.
///
/// The caller retains the reducer transition until this returns a C0 committed receipt. This
/// function validates all cross-record bindings before planning, then relies on C0 head and state
/// compare-and-swap in one `SQLite` transaction.
///
/// # Errors
///
/// Rejects any command/event/state mismatch, stale head/state expectation, canonical codec error,
/// or C0 storage/integrity failure.
pub fn commit_agent_transition(
    journal: &mut SqliteJournal,
    command: &AgentCommandDto,
    event: &AgentEventDto,
    state: &AgentStateDto,
) -> Result<CommittedBatch, AgentDurabilityError> {
    validate_binding(command, event, state)?;
    let aggregate = agent_aggregate_key(command.turn_id())?;
    let state_key = agent_state_key(command.turn_id());
    let head = journal.head(aggregate)?;
    let current_state = journal.state_record(AGENT_STATE_NAMESPACE, &state_key)?;
    if head.is_some() != current_state.is_some() {
        return Err(AgentDurabilityError::InconsistentDurability);
    }
    match head {
        None => {
            if command.expected_sequence() != 0 || command.expected_previous_event_id().is_some() {
                return Err(AgentDurabilityError::BindingMismatch);
            }
        }
        Some(observed) => {
            if observed.sequence().get() != command.expected_sequence()
                || Some(observed.event_id()) != command.expected_previous_event_id()
            {
                return Err(AgentDurabilityError::BindingMismatch);
            }
        }
    }
    let command_bytes = encode_message(command, CodecLimits::PRODUCTION)?;
    let event_bytes = encode_message(event, CodecLimits::PRODUCTION)?;
    let state_bytes = encode_message(state, CodecLimits::PRODUCTION)?;
    let frame = ExactFrame::new(event_bytes)?;
    let draft = EventDraft::new(
        aggregate,
        event.sequence(),
        event.id(),
        event.previous_event_id(),
        frame,
        revision_digest(&event.revision()),
        Vec::new(),
    )?;
    let state_install = StateInstall::new(
        AGENT_STATE_NAMESPACE,
        state_key,
        current_state.as_ref().map(peritus_journal::DurableStateRecord::revision),
        state.logical_revision().get(),
        state_bytes,
    )?;
    let expectation = head.map_or(HeadExpectation::Absent(aggregate), HeadExpectation::Present);
    let request = AppendRequest::new(
        journal.store_id(),
        command.command_id(),
        sha256(&command_bytes),
        vec![expectation],
        vec![draft],
        vec![state_install],
        Vec::new(),
        None,
        None,
        Vec::new(),
    );
    journal.append(request.plan()?).map_err(Into::into)
}

/// Loads and validates the complete canonical D0 chain plus current checkpoint for restart.
///
/// # Errors
///
/// Rejects corrupt C0 chains, non-D0 frames, wrong turn/revision bindings, gaps, or a checkpoint
/// that does not exactly reach the durable aggregate head.
pub fn load_agent_replay(
    journal: &SqliteJournal,
    turn_id: TurnId,
) -> Result<AgentReplay, AgentDurabilityError> {
    let aggregate = agent_aggregate_key(turn_id)?;
    let state_key = agent_state_key(turn_id);
    let records = journal.records_for_aggregate(aggregate)?;
    let state_record = journal.state_record(AGENT_STATE_NAMESPACE, &state_key)?;
    if records.is_empty() != state_record.is_none() {
        return Err(AgentDurabilityError::InconsistentDurability);
    }
    let mut events = Vec::with_capacity(records.len());
    for record in records {
        let event = decode_message::<AgentEventDto>(record.frame_bytes(), CodecLimits::PRODUCTION)?;
        if event.turn_id() != turn_id
            || event.sequence() != record.sequence()
            || event.id() != record.event_id()
            || event.command_id() != record.command_id()
            || event.previous_event_id() != record.previous_event_id()
            || revision_digest(&event.revision()) != record.revision_digest()
        {
            return Err(AgentDurabilityError::BindingMismatch);
        }
        events.push(event);
    }
    let checkpoint = state_record
        .map(|record| {
            decode_message::<AgentStateDto>(record.bytes(), CodecLimits::PRODUCTION)
                .map_err(AgentDurabilityError::from)
        })
        .transpose()?;
    if let Some(state) = &checkpoint {
        let Some(last) = events.last() else {
            return Err(AgentDurabilityError::InconsistentDurability);
        };
        if state.turn_id() != turn_id
            || state.aggregate_sequence() != last.sequence()
            || state.last_event_id() != last.id()
            || state.revision() != last.revision()
            || state.phase() != last.phase()
            || state.counters() != last.counters()
            || state.state_digest() != last.successor_state_digest()
        {
            return Err(AgentDurabilityError::InconsistentDurability);
        }
    }
    Ok(AgentReplay { events, checkpoint })
}

fn validate_binding(
    command: &AgentCommandDto,
    event: &AgentEventDto,
    state: &AgentStateDto,
) -> Result<(), AgentDurabilityError> {
    let next_sequence = command.expected_sequence().checked_add(1);
    let mismatches = [
        command.event_id() != event.id(),
        command.command_id() != event.command_id(),
        command.turn_id() != event.turn_id(),
        command.turn_id() != state.turn_id(),
        command.expected_previous_event_id() != event.previous_event_id(),
        next_sequence != Some(event.sequence().get()),
        command.revision() != event.revision(),
        command.revision() != state.revision(),
        command.successor_state_digest() != event.successor_state_digest(),
        command.successor_state_digest() != state.state_digest(),
        command.counters() != event.counters(),
        command.counters() != state.counters(),
        event.sequence() != state.aggregate_sequence(),
        event.id() != state.last_event_id(),
        event.phase() != state.phase(),
        !event_matches_command(command.kind(), event.kind()),
    ];
    if mismatches.into_iter().any(core::convert::identity) {
        return Err(AgentDurabilityError::BindingMismatch);
    }
    Ok(())
}

const fn event_matches_command(command: AgentCommandKindDto, event: AgentEventKindDto) -> bool {
    use AgentCommandKindDto as C;
    use AgentEventKindDto as E;
    matches!(
        (command, event),
        (C::StartTurn, E::TurnStarted)
            | (C::RecordContextPlan, E::ContextPlanRecorded)
            | (C::RecordModelRequest, E::ModelRequestRecorded)
            | (C::StartModelAttempt, E::ModelAttemptStarted)
            | (C::ObserveProviderEvent, E::ProviderEventObserved)
            | (C::FinishModelResponse, E::ModelResponseFinished)
            | (C::ProposeToolCalls, E::ToolCallsProposed)
            | (C::PrepareToolCall, E::ToolCallPrepared)
            | (C::RequestToolAuthorization, E::ToolAuthorizationRequested)
            | (C::AuthorizeToolCall, E::ToolCallAuthorized)
            | (C::DenyToolCall, E::ToolCallDenied)
            | (C::DispatchToolCall, E::ToolCallDispatched)
            | (C::ObserveToolResult, E::ToolResultObserved)
            | (C::RecordToolResults, E::ToolResultsRecorded)
            | (C::ScheduleRetry, E::RetryScheduled)
            | (C::ProposeCompletion, E::CompletionProposed)
            | (C::CompleteTurn, E::TurnCompleted)
            | (C::PauseTurn, E::TurnPaused)
            | (C::ResumeTurn, E::TurnResumed)
            | (C::CancelTurn, E::TurnCancellationStarted | E::TurnCancelled)
            | (C::FailTurn, E::TurnFailed)
            | (C::ClassifyRecovery, E::EffectRecoveryClassified)
            | (C::ExhaustLimits, E::LimitsExhausted)
    )
}
