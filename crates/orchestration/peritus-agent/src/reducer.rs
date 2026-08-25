//! Pure command reduction and exact event replay.

use crate::{
    ActivePhase, AgentBinding, AgentCommand, AgentCommandKind, AgentCounters, AgentErrorCode,
    AgentEvent, AgentEventKind, AgentFailureKind, AgentLimits, AgentOperation, AgentPhase,
    AgentRecovery, AgentRejection, AgentTurnState, TerminalKind, ToolResultStatus, ToolSlotPhase,
};
use peritus_types::{CommandId, EventId, EventSequence, RevisionNumber, Sha256Digest};
use std::collections::BTreeSet;

/// One accepted event and its deterministic successor state. This does not claim durability.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentTransition {
    event: AgentEvent,
    state: AgentTurnState,
}

impl AgentTransition {
    #[must_use]
    pub const fn event(&self) -> &AgentEvent {
        &self.event
    }
    #[must_use]
    pub const fn state(&self) -> &AgentTurnState {
        &self.state
    }
    #[must_use]
    pub fn into_parts(self) -> (AgentEvent, AgentTurnState) {
        (self.event, self.state)
    }
}

/// Starts a turn in `PreparingContext` and emits its genesis event.
///
/// # Errors
///
/// Returns a typed limit rejection if even the genesis transition is not permitted.
pub fn start(
    binding: AgentBinding,
    limits: AgentLimits,
    command_id: CommandId,
    event_id: EventId,
) -> Result<AgentTransition, AgentRejection> {
    let mut counters = AgentCounters::default();
    counters.transition(limits)?;
    let mut state = AgentTurnState {
        binding: binding.clone(),
        limits,
        counters,
        phase: AgentPhase::Active(ActivePhase::PreparingContext),
        paused_from: None,
        logical_revision: RevisionNumber::first(),
        sequence: EventSequence::first(),
        last_event_id: event_id,
        state_digest: Sha256Digest::new([0; 32]),
        context: None,
        model: crate::ModelState::default(),
        tools: None,
        tool_transcript_digest: None,
        completion: None,
        failure: None,
        unresolved_indeterminate: false,
    };
    state.state_digest = crate::canonical::state_digest(&state);
    let event = AgentEvent::new(
        event_id,
        command_id,
        EventSequence::first(),
        None,
        None,
        None,
        None,
        RevisionNumber::first(),
        Sha256Digest::new([0; 32]),
        state.state_digest,
        AgentEventKind::Started { binding, limits },
    );
    Ok(AgentTransition { event, state })
}

/// Applies exactly one causally fenced command without I/O or hidden effects.
///
/// # Errors
///
/// Returns a stable rejection for a stale fence, illegal phase, invalid payload, or crossed limit.
pub fn reduce(
    state: &AgentTurnState,
    command: &AgentCommand,
) -> Result<AgentTransition, AgentRejection> {
    if state.phase.is_terminal() {
        return Err(reject(
            state,
            AgentErrorCode::IllegalPhase,
            AgentRecovery::Terminal,
            "terminal state rejects all commands",
        ));
    }
    if command.expected_revision() != state.logical_revision()
        || command.expected_state_digest() != state.state_digest()
        || command.event_id() == state.last_event_id()
    {
        return Err(reject(
            state,
            AgentErrorCode::CausalMismatch,
            AgentRecovery::RetrySameCommand,
            "command causal fence does not match aggregate state",
        ));
    }
    let sequence = state.sequence.checked_next().map_err(|_| overflow(state))?;
    let revision = state.logical_revision.checked_next().map_err(|_| overflow(state))?;
    let mut successor = state.clone();
    let transition_limit_exhaustion = matches!(command.kind(), AgentCommandKind::Exhausted(failure)
        if matches!(failure.kind(), AgentFailureKind::Exhausted(crate::AgentLimitDimension::Transitions)))
        && successor.counters.transitions() == successor.limits.max_transitions();
    if !transition_limit_exhaustion {
        successor
            .counters
            .transition(successor.limits)
            .map_err(|error| error.at(state.binding.turn_id(), state.phase))?;
    }
    apply_kind(&mut successor, command.kind())?;
    successor.sequence = sequence;
    successor.logical_revision = revision;
    successor.last_event_id = command.event_id();
    successor.state_digest = crate::canonical::state_digest(&successor);
    let event = AgentEvent::new(
        command.event_id(),
        command.command_id(),
        sequence,
        Some(state.last_event_id),
        Some(state.phase),
        state.paused_from,
        Some(state.logical_revision),
        revision,
        state.state_digest,
        successor.state_digest,
        AgentEventKind::CommandAccepted(command.kind().clone()),
    );
    Ok(AgentTransition { event, state: successor })
}

/// Reconstructs exactly the same state from genesis and a canonical event stream.
///
/// # Errors
///
/// Returns `ReplayMismatch` when event order, fences, payload legality, or state digests disagree.
pub fn replay(events: &[AgentEvent]) -> Result<AgentTurnState, AgentRejection> {
    let first = events.first().ok_or_else(|| replay_error("event stream has no genesis"))?;
    let (binding, limits) = match first.kind() {
        AgentEventKind::Started { binding, limits } => (binding.clone(), *limits),
        AgentEventKind::CommandAccepted(_) => {
            return Err(replay_error("first event is not genesis"));
        }
    };
    let started = start(binding, limits, first.command_id(), first.id())?;
    if started.event != *first {
        return Err(replay_error("genesis event fence does not match"));
    }
    let mut state = started.state;
    let mut ids = BTreeSet::from([first.id()]);
    for event in &events[1..] {
        if !ids.insert(event.id()) {
            return Err(replay_error("event identifier is duplicated"));
        }
        let kind = match event.kind() {
            AgentEventKind::CommandAccepted(kind) => kind,
            AgentEventKind::Started { .. } => {
                return Err(replay_error("event stream contains a second genesis"));
            }
        };
        let expected_sequence =
            state.sequence.checked_next().map_err(|_| replay_error("event sequence overflow"))?;
        let expected_revision = state
            .logical_revision
            .checked_next()
            .map_err(|_| replay_error("state revision overflow"))?;
        if event.sequence() != expected_sequence
            || event.prior_revision() != Some(state.logical_revision)
            || event.previous_event_id() != Some(state.last_event_id)
            || event.prior_phase() != Some(state.phase)
            || event.prior_resumable() != state.paused_from
            || event.successor_revision() != expected_revision
            || event.prior_state_digest() != state.state_digest
        {
            return Err(replay_error("event predecessor fence or order does not match"));
        }
        let mut successor = state.clone();
        let transition_limit_exhaustion = matches!(kind, AgentCommandKind::Exhausted(failure)
            if matches!(failure.kind(), AgentFailureKind::Exhausted(crate::AgentLimitDimension::Transitions)))
            && successor.counters.transitions() == successor.limits.max_transitions();
        if !transition_limit_exhaustion {
            successor
                .counters
                .transition(successor.limits)
                .map_err(|_| replay_error("replayed transition exceeds limits"))?;
        }
        apply_kind(&mut successor, kind)
            .map_err(|_| replay_error("event payload is illegal for replayed state"))?;
        successor.sequence = expected_sequence;
        successor.logical_revision = expected_revision;
        successor.last_event_id = event.id();
        successor.state_digest = crate::canonical::state_digest(&successor);
        if successor.state_digest != event.successor_state_digest() {
            return Err(replay_error("event successor digest does not match replayed state"));
        }
        state = successor;
    }
    Ok(state)
}

#[allow(
    clippy::too_many_lines,
    reason = "one exhaustive match keeps the closed command vocabulary auditable"
)]
fn apply_kind(state: &mut AgentTurnState, kind: &AgentCommandKind) -> Result<(), AgentRejection> {
    match kind {
        AgentCommandKind::ContextPrepared(context) => {
            require(state, AgentPhase::Active(ActivePhase::PreparingContext))?;
            state.counters.context_cycle(state.limits).map_err(|error| scoped(state, error))?;
            state.context = Some(*context);
            state.phase = AgentPhase::Active(ActivePhase::RequestingModel);
        }
        AgentCommandKind::ModelRequestStarted { call_id, request_digest } => {
            require(state, AgentPhase::Active(ActivePhase::RequestingModel))?;
            if state.model.retry_pending() && state.model.request_digest() != Some(*request_digest)
            {
                return Err(reject(
                    state,
                    AgentErrorCode::CausalMismatch,
                    AgentRecovery::ResumeProvider,
                    "retry request digest does not match the scheduled request",
                ));
            }
            crate::state::set_model_started(state, *call_id, *request_digest);
            state.phase = AgentPhase::Active(ActivePhase::StreamingResponse);
        }
        AgentCommandKind::ProviderEventObserved(record) => observe_provider(state, record)?,
        AgentCommandKind::ProviderRetryScheduled(record) => schedule_retry(state, *record)?,
        AgentCommandKind::ToolCallsProposed { terminal, proposals } => {
            require(state, AgentPhase::Active(ActivePhase::StreamingResponse))?;
            validate_terminal(state, *terminal)?;
            let batch = crate::ToolBatch::new(
                proposals.clone(),
                state.binding.revision(),
                state.limits.max_tool_calls(),
            )?;
            let count = u16::try_from(proposals.len())
                .map_err(|_| invalid(state, "tool count cannot be represented"))?;
            state.counters.add_tools(count, state.limits).map_err(|error| scoped(state, error))?;
            crate::state::set_model_terminal(state, *terminal);
            state.tools = Some(batch);
            state.phase = AgentPhase::Active(ActivePhase::ProposedToolCalls);
        }
        AgentCommandKind::CompletionProposed { terminal, proposal } => {
            propose_completion(state, *terminal, proposal.clone())?;
        }
        AgentCommandKind::AuthorizationStarted => {
            require(state, AgentPhase::Active(ActivePhase::ProposedToolCalls))?;
            crate::tools::set_awaiting(tools_mut(state)?);
            state.phase = AgentPhase::Active(ActivePhase::AwaitingAuthorization);
        }
        AgentCommandKind::ToolAuthorized { ordinal, authority_digest } => {
            require(state, AgentPhase::Active(ActivePhase::AwaitingAuthorization))?;
            require_slot(
                state,
                tools(state)?.slot_mut_for_read(*ordinal)?,
                ToolSlotPhase::AwaitingAuthorization,
            )?;
            let slot = tools_mut(state)?.slot_mut(*ordinal)?;
            crate::tools::authorize(slot, *authority_digest);
        }
        AgentCommandKind::ToolDenied { ordinal, result } => {
            deny_tool(state, *ordinal, result.clone())?;
        }
        AgentCommandKind::ToolExecutionStarted => begin_execution(state)?,
        AgentCommandKind::ToolDispatched { ordinal } => dispatch_tool(state, *ordinal)?,
        AgentCommandKind::ToolActivated { ordinal } => {
            require(state, AgentPhase::Active(ActivePhase::ExecutingTools))?;
            require_slot(
                state,
                tools(state)?.slot_mut_for_read(*ordinal)?,
                ToolSlotPhase::Dispatched,
            )?;
            let slot = tools_mut(state)?.slot_mut(*ordinal)?;
            crate::tools::activate(slot);
        }
        AgentCommandKind::ToolProgressObserved { ordinal, sequence, progress_digest } => {
            progress_tool(state, *ordinal, *sequence, *progress_digest)?;
        }
        AgentCommandKind::ToolCompleted { ordinal, result } => {
            complete_tool(state, *ordinal, result.clone())?;
        }
        AgentCommandKind::ResultRecordingStarted => {
            require(state, AgentPhase::Active(ActivePhase::ExecutingTools))?;
            let batch = tools(state)?;
            if !batch.all_terminal() || state.counters.active_tool_calls() != 0 {
                return Err(invalid(state, "tool batch is not terminal"));
            }
            state.phase = AgentPhase::Active(ActivePhase::RecordingResults);
        }
        AgentCommandKind::ResultsRecorded { transcript_digest } => {
            record_results(state, *transcript_digest)?;
        }
        AgentCommandKind::Paused => pause(state)?,
        AgentCommandKind::Resumed { recovery_checked } => resume(state, *recovery_checked)?,
        AgentCommandKind::CancellationRequested => {
            if state.phase.is_terminal() || state.phase == AgentPhase::Cancelling {
                return Err(illegal(state));
            }
            state.paused_from = None;
            state.phase = AgentPhase::Cancelling;
        }
        AgentCommandKind::CancellationFinished => {
            require(state, AgentPhase::Cancelling)?;
            state.phase = AgentPhase::Terminal(TerminalKind::Cancelled);
        }
        AgentCommandKind::Failed(failure) => fail(state, failure.clone(), false)?,
        AgentCommandKind::Exhausted(failure) => fail(state, failure.clone(), true)?,
        AgentCommandKind::CompletionCommitted => {
            require(state, AgentPhase::Active(ActivePhase::ProposedCompletion))?;
            if state.completion.is_none() {
                return Err(ineligible(state, "completion proposal is absent"));
            }
            state.phase = AgentPhase::Terminal(TerminalKind::Completed);
        }
    }
    Ok(())
}

fn observe_provider(
    state: &mut AgentTurnState,
    record: &crate::ProviderEventRecord,
) -> Result<(), AgentRejection> {
    require(state, AgentPhase::Active(ActivePhase::StreamingResponse))?;
    let ordered = if record.duplicate() {
        record.cursor() > 0 && record.cursor() <= state.model.cursor()
    } else {
        state.model.cursor().checked_add(1) == Some(record.cursor())
    };
    if !ordered {
        return Err(AgentRejection::new(
            AgentErrorCode::InvalidProgress,
            AgentOperation::Reduce,
            AgentRecovery::ResumeProvider,
            "provider cursor is not canonical",
        )
        .at(state.binding.turn_id(), state.phase));
    }
    state
        .counters
        .provider_event(record.output_bytes(), state.limits)
        .map_err(|error| scoped(state, error))?;
    crate::state::observe_provider(state, record);
    Ok(())
}

fn schedule_retry(
    state: &mut AgentTurnState,
    record: crate::ProviderRetryRecord,
) -> Result<(), AgentRejection> {
    require(state, AgentPhase::Active(ActivePhase::StreamingResponse))?;
    if !state.model.in_flight() {
        return Err(reject(
            state,
            AgentErrorCode::InvalidCommand,
            AgentRecovery::ResumeProvider,
            "provider retry requires an in-flight model call",
        ));
    }
    if let crate::ProviderRetryClass::ExactResume { cursor } = record.class()
        && (cursor == 0 || cursor != state.model.cursor())
    {
        return Err(reject(
            state,
            AgentErrorCode::InvalidProgress,
            AgentRecovery::ResumeProvider,
            "exact provider resume cursor does not match the reduced stream",
        ));
    }
    crate::state::schedule_retry(state, record);
    state.phase = AgentPhase::Active(ActivePhase::RequestingModel);
    Ok(())
}

fn propose_completion(
    state: &mut AgentTurnState,
    terminal: crate::ModelTerminalRecord,
    proposal: crate::CompletionProposal,
) -> Result<(), AgentRejection> {
    require(state, AgentPhase::Active(ActivePhase::StreamingResponse))?;
    validate_terminal(state, terminal)?;
    if proposal.revision() != state.binding.revision() {
        return Err(ineligible(state, "completion revision is stale"));
    }
    if state.unresolved_indeterminate
        && (proposal.uncertainties().is_empty()
            || matches!(
                proposal.requested(),
                crate::CompletionRequest::RunGates | crate::CompletionRequest::RequestReview
            ))
    {
        return Err(ineligible(
            state,
            "indeterminate tool work must remain explicit in a non-success proposal",
        ));
    }
    let context = state.context.ok_or_else(|| ineligible(state, "completion context is absent"))?;
    if proposal.transcripts().context() != context.render_digest()
        || proposal.transcripts().model() != terminal.response_digest()
        || state
            .tool_transcript_digest
            .is_some_and(|digest| digest != proposal.transcripts().tools())
    {
        return Err(ineligible(state, "completion transcript binding does not match"));
    }
    crate::state::set_model_terminal(state, terminal);
    state.completion = Some(proposal);
    state.phase = AgentPhase::Active(ActivePhase::ProposedCompletion);
    Ok(())
}

const fn validate_terminal(
    state: &AgentTurnState,
    terminal: crate::ModelTerminalRecord,
) -> Result<(), AgentRejection> {
    if !terminal.normal_terminal() || terminal.incomplete_items() || !terminal.usage_settled() {
        Err(ineligible(state, "model terminal facts do not permit progress"))
    } else {
        Ok(())
    }
}

fn deny_tool(
    state: &mut AgentTurnState,
    ordinal: crate::ToolOrdinal,
    result: crate::ToolResultRecord,
) -> Result<(), AgentRejection> {
    require(state, AgentPhase::Active(ActivePhase::AwaitingAuthorization))?;
    if result.status() != ToolResultStatus::Denied {
        return Err(invalid(state, "authorization denial requires a denied result"));
    }
    require_slot(
        state,
        tools(state)?.slot_mut_for_read(ordinal)?,
        ToolSlotPhase::AwaitingAuthorization,
    )?;
    let slot = tools_mut(state)?.slot_mut(ordinal)?;
    crate::tools::terminal(slot, result);
    Ok(())
}

fn begin_execution(state: &mut AgentTurnState) -> Result<(), AgentRejection> {
    require(state, AgentPhase::Active(ActivePhase::AwaitingAuthorization))?;
    if tools(state)?
        .slots()
        .iter()
        .any(|slot| !matches!(slot.phase(), ToolSlotPhase::Authorized | ToolSlotPhase::Terminal))
    {
        return Err(invalid(state, "authorization decisions are incomplete"));
    }
    state.phase = AgentPhase::Active(ActivePhase::ExecutingTools);
    Ok(())
}

fn dispatch_tool(
    state: &mut AgentTurnState,
    ordinal: crate::ToolOrdinal,
) -> Result<(), AgentRejection> {
    require(state, AgentPhase::Active(ActivePhase::ExecutingTools))?;
    if tools(state)?.slot_mut_for_read(ordinal)? != ToolSlotPhase::Authorized {
        return Err(invalid(state, "tool is not authorized for dispatch"));
    }
    state.counters.start_tool(state.limits).map_err(|error| scoped(state, error))?;
    crate::tools::dispatch(tools_mut(state)?.slot_mut(ordinal)?);
    Ok(())
}

fn progress_tool(
    state: &mut AgentTurnState,
    ordinal: crate::ToolOrdinal,
    sequence: u32,
    progress_digest: Sha256Digest,
) -> Result<(), AgentRejection> {
    require(state, AgentPhase::Active(ActivePhase::ExecutingTools))?;
    require_slot(state, tools(state)?.slot_mut_for_read(ordinal)?, ToolSlotPhase::Active)?;
    let invalid_progress = AgentRejection::new(
        AgentErrorCode::InvalidProgress,
        AgentOperation::Reduce,
        AgentRecovery::ReconcileTool,
        "tool progress sequence is not canonical",
    )
    .at(state.binding.turn_id(), state.phase);
    let slot = tools_mut(state)?.slot_mut(ordinal)?;
    if slot.next_progress_sequence() != sequence {
        return Err(invalid_progress);
    }
    crate::tools::progress(slot, progress_digest);
    Ok(())
}

fn complete_tool(
    state: &mut AgentTurnState,
    ordinal: crate::ToolOrdinal,
    result: crate::ToolResultRecord,
) -> Result<(), AgentRejection> {
    require(state, AgentPhase::Active(ActivePhase::ExecutingTools))?;
    let phase = tools(state)?.slot_mut_for_read(ordinal)?;
    if !matches!(phase, ToolSlotPhase::Dispatched | ToolSlotPhase::Active) {
        return Err(invalid(state, "tool has not been dispatched"));
    }
    state
        .counters
        .finish_tool(result.model_visible_bytes(), state.limits)
        .map_err(|error| scoped(state, error))?;
    crate::tools::terminal(tools_mut(state)?.slot_mut(ordinal)?, result);
    Ok(())
}

fn record_results(state: &mut AgentTurnState, digest: Sha256Digest) -> Result<(), AgentRejection> {
    require(state, AgentPhase::Active(ActivePhase::RecordingResults))?;
    state.unresolved_indeterminate |= tools(state)?.has_indeterminate();
    state.tool_transcript_digest = Some(digest);
    state.tools = None;
    state.model = crate::ModelState::default();
    state.phase = AgentPhase::Active(ActivePhase::PreparingContext);
    Ok(())
}

const fn pause(state: &mut AgentTurnState) -> Result<(), AgentRejection> {
    let AgentPhase::Active(active) = state.phase else {
        return Err(illegal(state));
    };
    state.paused_from = Some(active);
    state.phase = AgentPhase::Paused;
    Ok(())
}

fn resume(state: &mut AgentTurnState, checked: bool) -> Result<(), AgentRejection> {
    require(state, AgentPhase::Paused)?;
    if !checked {
        return Err(AgentRejection::new(
            AgentErrorCode::InvalidCommand,
            AgentOperation::Reduce,
            AgentRecovery::RestartTurn,
            "resume requires completed recovery checks",
        )
        .at(state.binding.turn_id(), state.phase));
    }
    let active = state
        .paused_from
        .take()
        .ok_or_else(|| invalid(state, "paused state has no resumable phase"))?;
    state.phase = AgentPhase::Active(active);
    Ok(())
}

fn fail(
    state: &mut AgentTurnState,
    failure: crate::AgentFailure,
    exhausted: bool,
) -> Result<(), AgentRejection> {
    if exhausted != matches!(failure.kind(), AgentFailureKind::Exhausted(_)) {
        return Err(invalid(state, "failure command does not match failure kind"));
    }
    state.failure = Some(failure);
    state.completion = None;
    state.phase = AgentPhase::Terminal(TerminalKind::Failed);
    Ok(())
}

fn tools(state: &AgentTurnState) -> Result<&crate::ToolBatch, AgentRejection> {
    state.tools.as_ref().ok_or_else(|| invalid(state, "tool batch is absent"))
}
fn tools_mut(state: &mut AgentTurnState) -> Result<&mut crate::ToolBatch, AgentRejection> {
    let error = invalid(state, "tool batch is absent");
    state.tools.as_mut().ok_or(error)
}
fn require(state: &AgentTurnState, expected: AgentPhase) -> Result<(), AgentRejection> {
    if state.phase == expected { Ok(()) } else { Err(illegal(state)) }
}
fn require_slot(
    state: &AgentTurnState,
    actual: ToolSlotPhase,
    expected: ToolSlotPhase,
) -> Result<(), AgentRejection> {
    if actual == expected { Ok(()) } else { Err(invalid(state, "tool slot phase is illegal")) }
}
const fn scoped(state: &AgentTurnState, error: AgentRejection) -> AgentRejection {
    error.at(state.binding.turn_id(), state.phase)
}
const fn reject(
    state: &AgentTurnState,
    code: AgentErrorCode,
    recovery: AgentRecovery,
    detail: &'static str,
) -> AgentRejection {
    AgentRejection::new(code, AgentOperation::Reduce, recovery, detail)
        .at(state.binding.turn_id(), state.phase)
}
const fn illegal(state: &AgentTurnState) -> AgentRejection {
    reject(
        state,
        AgentErrorCode::IllegalPhase,
        AgentRecovery::CorrectRequest,
        "command is illegal in the current phase",
    )
}
const fn invalid(state: &AgentTurnState, detail: &'static str) -> AgentRejection {
    reject(state, AgentErrorCode::InvalidCommand, AgentRecovery::CorrectRequest, detail)
}
const fn ineligible(state: &AgentTurnState, detail: &'static str) -> AgentRejection {
    reject(state, AgentErrorCode::CompletionIneligible, AgentRecovery::CorrectRequest, detail)
}
const fn overflow(state: &AgentTurnState) -> AgentRejection {
    reject(
        state,
        AgentErrorCode::ArithmeticOverflow,
        AgentRecovery::Exhausted,
        "aggregate sequence overflow",
    )
}
const fn replay_error(detail: &'static str) -> AgentRejection {
    AgentRejection::new(
        AgentErrorCode::ReplayMismatch,
        AgentOperation::Replay,
        AgentRecovery::RestartTurn,
        detail,
    )
}
