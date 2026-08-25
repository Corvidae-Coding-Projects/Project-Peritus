//! Canonical projection into the inert D0 B3 state DTO.

use crate::{
    ActivePhase, AgentBinding, AgentCommand, AgentCommandKind, AgentCounters, AgentEvent,
    AgentEventKind, AgentLimits, AgentPhase, AgentRejection, AgentTransition, AgentTurnState,
    TerminalKind,
};
use peritus_codec::{CodecError, CodecErrorKind, CodecLimits};
use peritus_protocol::{
    AgentCommandDto, AgentCommandKindDto, AgentCountersDto, AgentEventDto, AgentEventKindDto,
    AgentPhaseDto, AgentResumablePhaseDto, AgentStateDto,
};
use peritus_types::RevisionNumber;

impl From<ActivePhase> for AgentResumablePhaseDto {
    fn from(value: ActivePhase) -> Self {
        match value {
            ActivePhase::PreparingContext => Self::PreparingContext,
            ActivePhase::RequestingModel => Self::RequestingModel,
            ActivePhase::StreamingResponse => Self::StreamingResponse,
            ActivePhase::ProposedToolCalls => Self::ProposedToolCalls,
            ActivePhase::AwaitingAuthorization => Self::AwaitingAuthorization,
            ActivePhase::ExecutingTools => Self::ExecutingTools,
            ActivePhase::RecordingResults => Self::RecordingResults,
            ActivePhase::ProposedCompletion => Self::ProposedCompletion,
        }
    }
}

impl From<AgentCounters> for AgentCountersDto {
    fn from(value: AgentCounters) -> Self {
        Self::new(
            u64::from(value.tool_calls()),
            u64::from(value.provider_events()),
            u64::from(value.context_cycles()),
            value.output_bytes(),
            value.tool_result_bytes(),
            u64::from(value.peak_concurrent_tool_calls()),
            u64::from(value.transitions()),
        )
    }
}

impl AgentTurnState {
    /// Projects a trusted replayed state into canonical inert B3 family 42 data.
    ///
    /// # Errors
    ///
    /// Returns a codec error when the bounded reducer payload exceeds B3 limits.
    ///
    /// # Panics
    ///
    /// Panics only if an internally constructed paused state violates its retained-phase invariant.
    pub fn to_protocol_state(&self, limits: CodecLimits) -> Result<AgentStateDto, CodecError> {
        let binding = self.binding();
        let phase = match self.phase() {
            AgentPhase::Active(active) => AgentPhaseDto::Active(active.into()),
            AgentPhase::Paused => AgentPhaseDto::Paused(
                self.paused_from()
                    .expect("checked paused state retains its resumable phase")
                    .into(),
            ),
            AgentPhase::Cancelling => AgentPhaseDto::Cancelling,
            AgentPhase::Terminal(TerminalKind::Completed) => AgentPhaseDto::Completed,
            AgentPhase::Terminal(TerminalKind::Failed) => AgentPhaseDto::Failed,
            AgentPhase::Terminal(TerminalKind::Cancelled) => AgentPhaseDto::Cancelled,
        };
        let profile_revision = RevisionNumber::new(binding.provider_profile_revision().get())
            .expect("checked provider profile revision is positive");
        AgentStateDto::new(
            binding.turn_id(),
            binding.attempt_id(),
            binding.actor_id(),
            binding.role(),
            binding.session_id(),
            binding.environment_id(),
            binding.revision(),
            profile_revision,
            binding.limits_revision(),
            self.logical_revision(),
            self.sequence(),
            self.last_event_id(),
            phase,
            self.counters().into(),
            self.state_digest(),
            self.canonical_bytes(),
            limits,
        )
    }
}

impl AgentTransition {
    /// Projects one accepted transition into inert B3 command, event, and state records.
    /// `None` is valid only for the genesis transition.
    ///
    /// # Errors
    ///
    /// Returns a codec error for a mismatched command or excessive canonical payload.
    #[allow(
        clippy::suspicious_operation_groupings,
        reason = "nominal command_id/event_id are intentionally checked against differently named event accessors"
    )]
    pub fn to_protocol_records(
        &self,
        command: Option<&AgentCommand>,
        limits: CodecLimits,
    ) -> Result<(AgentCommandDto, AgentEventDto, AgentStateDto), CodecError> {
        let event = self.event();
        let state = self.state();
        let (command_kind, event_kind, payload) = match event.kind() {
            AgentEventKind::Started { .. } if command.is_none() => {
                (AgentCommandKindDto::StartTurn, AgentEventKindDto::TurnStarted, Vec::new())
            }
            AgentEventKind::CommandAccepted(kind) => {
                let supplied = command.ok_or_else(bridge_codec_error)?;
                let identity_matches = supplied.command_id() == event.command_id()
                    && supplied.event_id() == event.id();
                if !identity_matches || supplied.kind() != kind {
                    return Err(bridge_codec_error());
                }
                (
                    command_kind_dto(kind),
                    event_kind_dto(kind),
                    crate::canonical::encode_command_kind(kind),
                )
            }
            AgentEventKind::Started { .. } => return Err(bridge_codec_error()),
        };
        let prior_phase = event.prior_phase().unwrap_or_else(|| state.phase());
        let command_phase =
            protocol_phase(prior_phase, event.prior_resumable().or_else(|| state.paused_from()))?;
        let successor_phase = protocol_phase(state.phase(), state.paused_from())?;
        let command_dto = AgentCommandDto::new(
            event.command_id(),
            event.id(),
            state.binding().turn_id(),
            event.sequence().get().saturating_sub(1),
            event.previous_event_id(),
            state.binding().revision(),
            command_phase,
            command_kind,
            state.state_digest(),
            state.counters().into(),
            payload.clone(),
            limits,
        )?;
        let event_dto = AgentEventDto::new(
            event.id(),
            event.command_id(),
            event.sequence(),
            event.previous_event_id(),
            state.binding().turn_id(),
            state.binding().revision(),
            successor_phase,
            event_kind,
            state.state_digest(),
            state.counters().into(),
            payload,
            limits,
        )?;
        Ok((command_dto, event_dto, state.to_protocol_state(limits)?))
    }
}

impl AgentEvent {
    /// Recovers pure events from decoded B3 records and verifies each successor through the reducer.
    ///
    /// # Errors
    ///
    /// Returns a stable replay rejection for malformed payloads or any DTO/reducer disagreement.
    pub fn recover_protocol_events(
        records: &[AgentEventDto],
        binding: AgentBinding,
        limits: AgentLimits,
    ) -> Result<Vec<Self>, AgentRejection> {
        let first =
            records.first().ok_or_else(|| bridge_replay_error("protocol event stream is empty"))?;
        if first.kind() != AgentEventKindDto::TurnStarted || !first.payload().is_empty() {
            return Err(bridge_replay_error("protocol event stream has invalid genesis"));
        }
        let started = crate::start(binding, limits, first.command_id(), first.id())?;
        validate_event_dto(started.state(), started.event(), first)?;
        let (event, mut state) = started.into_parts();
        let mut events = vec![event];
        for record in &records[1..] {
            let kind = crate::canonical::decode_command_kind(record.payload())?;
            if event_kind_dto(&kind) != record.kind() {
                return Err(bridge_replay_error("protocol event kind disagrees with payload"));
            }
            let command = AgentCommand::new(
                record.command_id(),
                record.id(),
                state.logical_revision(),
                state.state_digest(),
                kind,
            );
            let transition = crate::reduce(&state, &command)?;
            validate_event_dto(transition.state(), transition.event(), record)?;
            let (event, successor) = transition.into_parts();
            events.push(event);
            state = successor;
        }
        Ok(events)
    }
}

fn validate_event_dto(
    state: &AgentTurnState,
    event: &AgentEvent,
    dto: &AgentEventDto,
) -> Result<(), AgentRejection> {
    let phase = protocol_phase(state.phase(), state.paused_from())
        .map_err(|_| bridge_replay_error("invalid replayed phase"))?;
    if dto.id() != event.id()
        || dto.command_id() != event.command_id()
        || dto.sequence() != event.sequence()
        || dto.previous_event_id() != event.previous_event_id()
        || dto.turn_id() != state.binding().turn_id()
        || dto.revision() != state.binding().revision()
        || dto.phase() != phase
        || dto.successor_state_digest() != state.state_digest()
        || dto.counters() != AgentCountersDto::from(state.counters())
    {
        Err(bridge_replay_error("protocol event does not match reduced successor"))
    } else {
        Ok(())
    }
}

fn protocol_phase(
    phase: AgentPhase,
    paused_from: Option<ActivePhase>,
) -> Result<AgentPhaseDto, CodecError> {
    match phase {
        AgentPhase::Active(active) => Ok(AgentPhaseDto::Active(active.into())),
        AgentPhase::Paused => paused_from
            .map(|value| AgentPhaseDto::Paused(value.into()))
            .ok_or_else(bridge_codec_error),
        AgentPhase::Cancelling => Ok(AgentPhaseDto::Cancelling),
        AgentPhase::Terminal(TerminalKind::Completed) => Ok(AgentPhaseDto::Completed),
        AgentPhase::Terminal(TerminalKind::Failed) => Ok(AgentPhaseDto::Failed),
        AgentPhase::Terminal(TerminalKind::Cancelled) => Ok(AgentPhaseDto::Cancelled),
    }
}

const fn command_kind_dto(kind: &AgentCommandKind) -> AgentCommandKindDto {
    match kind {
        AgentCommandKind::ContextPrepared(_) => AgentCommandKindDto::RecordContextPlan,
        AgentCommandKind::ModelRequestStarted { .. } => AgentCommandKindDto::StartModelAttempt,
        AgentCommandKind::ProviderEventObserved(_) => AgentCommandKindDto::ObserveProviderEvent,
        AgentCommandKind::ProviderRetryScheduled(_) => AgentCommandKindDto::ScheduleRetry,
        AgentCommandKind::ToolCallsProposed { .. } => AgentCommandKindDto::ProposeToolCalls,
        AgentCommandKind::CompletionProposed { .. } => AgentCommandKindDto::ProposeCompletion,
        AgentCommandKind::AuthorizationStarted => AgentCommandKindDto::RequestToolAuthorization,
        AgentCommandKind::ToolAuthorized { .. } | AgentCommandKind::ToolExecutionStarted => {
            AgentCommandKindDto::AuthorizeToolCall
        }
        AgentCommandKind::ToolDenied { .. } => AgentCommandKindDto::DenyToolCall,
        AgentCommandKind::ToolDispatched { .. } => AgentCommandKindDto::DispatchToolCall,
        AgentCommandKind::ToolActivated { .. }
        | AgentCommandKind::ToolProgressObserved { .. }
        | AgentCommandKind::ToolCompleted { .. } => AgentCommandKindDto::ObserveToolResult,
        AgentCommandKind::ResultRecordingStarted | AgentCommandKind::ResultsRecorded { .. } => {
            AgentCommandKindDto::RecordToolResults
        }
        AgentCommandKind::Paused => AgentCommandKindDto::PauseTurn,
        AgentCommandKind::Resumed { .. } => AgentCommandKindDto::ResumeTurn,
        AgentCommandKind::CancellationRequested | AgentCommandKind::CancellationFinished => {
            AgentCommandKindDto::CancelTurn
        }
        AgentCommandKind::Failed(_) => AgentCommandKindDto::FailTurn,
        AgentCommandKind::Exhausted(_) => AgentCommandKindDto::ExhaustLimits,
        AgentCommandKind::CompletionCommitted => AgentCommandKindDto::CompleteTurn,
    }
}

const fn event_kind_dto(kind: &AgentCommandKind) -> AgentEventKindDto {
    match kind {
        AgentCommandKind::ContextPrepared(_) => AgentEventKindDto::ContextPlanRecorded,
        AgentCommandKind::ModelRequestStarted { .. } => AgentEventKindDto::ModelAttemptStarted,
        AgentCommandKind::ProviderEventObserved(_) => AgentEventKindDto::ProviderEventObserved,
        AgentCommandKind::ProviderRetryScheduled(_) => AgentEventKindDto::RetryScheduled,
        AgentCommandKind::ToolCallsProposed { .. } => AgentEventKindDto::ToolCallsProposed,
        AgentCommandKind::CompletionProposed { .. } => AgentEventKindDto::CompletionProposed,
        AgentCommandKind::AuthorizationStarted => AgentEventKindDto::ToolAuthorizationRequested,
        AgentCommandKind::ToolAuthorized { .. } | AgentCommandKind::ToolExecutionStarted => {
            AgentEventKindDto::ToolCallAuthorized
        }
        AgentCommandKind::ToolDenied { .. } => AgentEventKindDto::ToolCallDenied,
        AgentCommandKind::ToolDispatched { .. } => AgentEventKindDto::ToolCallDispatched,
        AgentCommandKind::ToolActivated { .. }
        | AgentCommandKind::ToolProgressObserved { .. }
        | AgentCommandKind::ToolCompleted { .. } => AgentEventKindDto::ToolResultObserved,
        AgentCommandKind::ResultRecordingStarted | AgentCommandKind::ResultsRecorded { .. } => {
            AgentEventKindDto::ToolResultsRecorded
        }
        AgentCommandKind::Paused => AgentEventKindDto::TurnPaused,
        AgentCommandKind::Resumed { .. } => AgentEventKindDto::TurnResumed,
        AgentCommandKind::CancellationRequested => AgentEventKindDto::TurnCancellationStarted,
        AgentCommandKind::CancellationFinished => AgentEventKindDto::TurnCancelled,
        AgentCommandKind::Failed(_) => AgentEventKindDto::TurnFailed,
        AgentCommandKind::Exhausted(_) => AgentEventKindDto::LimitsExhausted,
        AgentCommandKind::CompletionCommitted => AgentEventKindDto::TurnCompleted,
    }
}

const fn bridge_codec_error() -> CodecError {
    CodecError::at(CodecErrorKind::InvalidDomainValue, 0)
}
const fn bridge_replay_error(detail: &'static str) -> AgentRejection {
    AgentRejection::new(
        crate::AgentErrorCode::ReplayMismatch,
        crate::AgentOperation::Replay,
        crate::AgentRecovery::RestartTurn,
        detail,
    )
}
