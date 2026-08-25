//! Cooperative durable composition of the pure reducer and ordinary effect adapters.

mod tool_steps;

use core::fmt;

use peritus_budget::{BudgetReceipt, UsageFinality};
use peritus_codec::{CodecError, CodecLimits, sha256};
use peritus_journal::SqliteJournal;
use peritus_model_protocol::{
    EventEnvelope, ModelRequest, ProtocolLimits, ReducerTransition, TerminalOutcome,
    decode_event_envelope,
};
use peritus_provider_core::{
    CancellationToken, ContinuationRestoreOutcome, ModelProvider, PersistedContinuation,
};
use peritus_types::{CommandId, EventId, EventSequence, Sha256Digest};

use crate::{
    AgentBinding, AgentCommand, AgentCommandKind, AgentEvent, AgentEventKind, AgentLimits,
    AgentPhase, AgentRejection, AgentTurnState, ModelCallId, ModelTerminalRecord,
    ProviderEventRecord, ProviderRetryClass, ToolOrdinal, ToolSlotPhase, reduce, replay, start,
};

use super::{
    AgentBudgetError, AgentBudgetPort, AgentBudgetReservation, AgentBudgetState,
    AgentDurabilityError, ModelAdvance, ModelDriveError, ModelSession, ToolBatchCoordinator,
    ToolDriveError, commit_agent_transition, load_agent_replay,
};

/// Caller-supplied deterministic identities for exactly one durable transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransitionIdentity {
    command_id: CommandId,
    event_id: EventId,
}

impl TransitionIdentity {
    /// Binds one command identity to the event identity it will produce.
    #[must_use]
    pub const fn new(command_id: CommandId, event_id: EventId) -> Self {
        Self { command_id, event_id }
    }

    /// Returns the command identity.
    #[must_use]
    pub const fn command_id(self) -> CommandId {
        self.command_id
    }

    /// Returns the resulting event identity.
    #[must_use]
    pub const fn event_id(self) -> EventId {
        self.event_id
    }
}

/// Receipt returned only after the journal committed a reducer transition and checkpoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommittedAgentStep {
    sequence: EventSequence,
    phase: AgentPhase,
    state_digest: Sha256Digest,
}

impl CommittedAgentStep {
    /// Returns the committed aggregate sequence.
    #[must_use]
    pub const fn sequence(self) -> EventSequence {
        self.sequence
    }

    /// Returns the committed successor phase.
    #[must_use]
    pub const fn phase(self) -> AgentPhase {
        self.phase
    }

    /// Returns the committed successor state digest.
    #[must_use]
    pub const fn state_digest(self) -> Sha256Digest {
        self.state_digest
    }
}

/// Result of one cooperative provider pull and durable acknowledgement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProviderAdvance {
    /// One normalized envelope was committed and then applied to the live C5 reducer.
    Envelope {
        /// Adapter-local sequence carried by the envelope.
        sequence: u64,
        /// C5 semantic transition after the durable acknowledgement.
        transition: ReducerTransition,
    },
    /// The provider stream has no more envelopes.
    Closed,
}

/// Effects that were logically in flight but cannot be reconstructed from process-local handles.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryReport {
    model_in_flight: bool,
    tool_ordinals: Vec<ToolOrdinal>,
}

impl RecoveryReport {
    /// Returns whether a provider attempt needs explicit C5 restoration, retry, or failure.
    #[must_use]
    pub const fn model_in_flight(&self) -> bool {
        self.model_in_flight
    }

    /// Returns dispatched/active tool slots that must be classified without redispatch.
    #[must_use]
    pub fn tool_ordinals(&self) -> &[ToolOrdinal] {
        &self.tool_ordinals
    }

    /// Returns whether restart found no uncertain effect ownership.
    #[must_use]
    pub const fn is_clean(&self) -> bool {
        !self.model_in_flight && self.tool_ordinals.is_empty()
    }
}

/// Redaction-safe failure from the cooperative D0 driver.
#[derive(Debug)]
#[non_exhaustive]
pub enum AgentDriverError {
    /// No durable aggregate exists for the requested turn.
    MissingAggregate,
    /// The replayed state disagrees with the atomically installed acceleration checkpoint.
    CheckpointMismatch,
    /// The requested runtime resource is not installed in this process.
    RuntimeResourceUnavailable,
    /// A previewed C5 transition differed after durable acknowledgement.
    RuntimeInvariant,
    /// The pure reducer rejected the transition without changing state.
    Rejected(AgentRejection),
    /// Canonical B3 projection failed.
    Codec(CodecError),
    /// C0 commit or replay loading failed.
    Durability(AgentDurabilityError),
    /// C5 provider driving failed.
    Model(ModelDriveError),
    /// C4 preparation, routing, execution, or observation failed.
    Tool(ToolDriveError),
    /// B1 reservation, usage, or settlement enforcement failed.
    Budget(AgentBudgetError),
}

impl fmt::Display for AgentDriverError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingAggregate => formatter.write_str("agent aggregate does not exist"),
            Self::CheckpointMismatch => {
                formatter.write_str("agent replay disagrees with its durable checkpoint")
            }
            Self::RuntimeResourceUnavailable => {
                formatter.write_str("required process-local agent resource is unavailable")
            }
            Self::RuntimeInvariant => {
                formatter.write_str("durable acknowledgement changed a previewed runtime result")
            }
            Self::Rejected(error) => fmt::Display::fmt(error, formatter),
            Self::Codec(error) => fmt::Display::fmt(error, formatter),
            Self::Durability(error) => fmt::Display::fmt(error, formatter),
            Self::Model(error) => fmt::Display::fmt(error, formatter),
            Self::Tool(error) => fmt::Display::fmt(error, formatter),
            Self::Budget(error) => fmt::Display::fmt(error, formatter),
        }
    }
}

impl std::error::Error for AgentDriverError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Rejected(error) => Some(error),
            Self::Codec(error) => Some(error),
            Self::Durability(error) => Some(error),
            Self::Model(error) => Some(error),
            Self::Tool(error) => Some(error),
            Self::Budget(error) => Some(error),
            Self::MissingAggregate
            | Self::CheckpointMismatch
            | Self::RuntimeResourceUnavailable
            | Self::RuntimeInvariant => None,
        }
    }
}

impl From<AgentRejection> for AgentDriverError {
    fn from(value: AgentRejection) -> Self {
        Self::Rejected(value)
    }
}

impl From<CodecError> for AgentDriverError {
    fn from(value: CodecError) -> Self {
        Self::Codec(value)
    }
}

impl From<AgentDurabilityError> for AgentDriverError {
    fn from(value: AgentDurabilityError) -> Self {
        Self::Durability(value)
    }
}

impl From<ModelDriveError> for AgentDriverError {
    fn from(value: ModelDriveError) -> Self {
        Self::Model(value)
    }
}

impl From<ToolDriveError> for AgentDriverError {
    fn from(value: ToolDriveError) -> Self {
        Self::Tool(value)
    }
}

impl From<AgentBudgetError> for AgentDriverError {
    fn from(value: AgentBudgetError) -> Self {
        Self::Budget(value)
    }
}

/// One durable turn plus the process-local provider/tool resources it currently owns.
///
/// Every public step is cooperative. A reducer transition is committed together with its current
/// checkpoint before `state` advances. Provider envelopes are previewed, durably observed, and
/// only then acknowledged to C5. Tool dispatch intent is committed before the sole C4 router may
/// invoke a dispatcher.
pub struct AgentDriver {
    state: AgentTurnState,
    codec_limits: CodecLimits,
    model: Option<ModelSession>,
    model_prefix: Vec<EventEnvelope>,
    tools: Option<ToolBatchCoordinator>,
}

#[allow(
    clippy::missing_errors_doc,
    reason = "the common checked driver boundary and per-method failure behavior are documented"
)]
impl AgentDriver {
    /// Creates and durably commits a new D0 aggregate genesis transition.
    ///
    /// # Errors
    ///
    /// Returns a checked binding/reducer, protocol projection, or journal error.
    pub fn start(
        journal: &mut SqliteJournal,
        binding: AgentBinding,
        limits: AgentLimits,
        identity: TransitionIdentity,
        codec_limits: CodecLimits,
    ) -> Result<Self, AgentDriverError> {
        let transition = start(binding, limits, identity.command_id, identity.event_id)?;
        let records = transition.to_protocol_records(None, codec_limits)?;
        commit_agent_transition(journal, &records.0, &records.1, &records.2)?;
        let (_, state) = transition.into_parts();
        Ok(Self { state, codec_limits, model: None, model_prefix: Vec::new(), tools: None })
    }

    /// Reconstructs one turn from its canonical event chain and validates its acceleration row.
    ///
    /// Process-local provider streams and C4 handles are intentionally not invented. Call
    /// [`Self::recovery_report`] and explicitly restore/classify outstanding effects.
    ///
    /// # Errors
    ///
    /// Rejects a missing aggregate, malformed chain, reducer disagreement, or checkpoint drift.
    pub fn restore(
        journal: &SqliteJournal,
        binding: AgentBinding,
        limits: AgentLimits,
        codec_limits: CodecLimits,
    ) -> Result<Self, AgentDriverError> {
        let durable = load_agent_replay(journal, binding.turn_id())?;
        if durable.events().is_empty() {
            return Err(AgentDriverError::MissingAggregate);
        }
        let events = AgentEvent::recover_protocol_events(durable.events(), binding, limits)?;
        let model_prefix = recover_model_prefix(&events)?;
        let state = replay(&events)?;
        let checkpoint = durable.checkpoint().ok_or(AgentDriverError::CheckpointMismatch)?;
        let digest_matches = checkpoint.state_digest() == state.state_digest();
        let payload_matches = checkpoint.payload() == state.canonical_bytes();
        if !digest_matches || !payload_matches {
            return Err(AgentDriverError::CheckpointMismatch);
        }
        Ok(Self { state, codec_limits, model: None, model_prefix, tools: None })
    }

    /// Borrows the last durably committed pure state.
    #[must_use]
    pub const fn state(&self) -> &AgentTurnState {
        &self.state
    }

    /// Borrows the process-local C5 session when this driver owns one.
    #[must_use]
    pub const fn model(&self) -> Option<&ModelSession> {
        self.model.as_ref()
    }

    /// Borrows the process-local C4 coordinator when this driver owns one.
    #[must_use]
    pub const fn tools(&self) -> Option<&ToolBatchCoordinator> {
        self.tools.as_ref()
    }

    /// Reports logically outstanding effects after restore without creating or repeating them.
    #[must_use]
    pub fn recovery_report(&self) -> RecoveryReport {
        let tool_ordinals = self
            .state
            .tools()
            .map(|batch| {
                batch
                    .slots()
                    .iter()
                    .filter(|slot| {
                        matches!(slot.phase(), ToolSlotPhase::Dispatched | ToolSlotPhase::Active)
                    })
                    .map(|slot| slot.proposal().ordinal())
                    .collect()
            })
            .unwrap_or_default();
        RecoveryReport {
            model_in_flight: self.state.model().in_flight() && self.model.is_none(),
            tool_ordinals,
        }
    }

    /// Applies and commits exactly one pure reducer command.
    ///
    /// The in-memory state remains unchanged if reduction, projection, or persistence fails.
    ///
    /// # Errors
    ///
    /// Returns a reducer, codec, or C0 commit failure.
    pub fn drive_once(
        &mut self,
        journal: &mut SqliteJournal,
        identity: TransitionIdentity,
        kind: AgentCommandKind,
    ) -> Result<CommittedAgentStep, AgentDriverError> {
        let clear_model_prefix = matches!(&kind, AgentCommandKind::ResultsRecorded { .. })
            || matches!(
                &kind,
                AgentCommandKind::ProviderRetryScheduled(record)
                    if matches!(record.class(), ProviderRetryClass::SafeNewRequest)
            );
        let command = AgentCommand::new(
            identity.command_id,
            identity.event_id,
            self.state.logical_revision(),
            self.state.state_digest(),
            kind,
        );
        let transition = reduce(&self.state, &command)?;
        let records = transition.to_protocol_records(Some(&command), self.codec_limits)?;
        commit_agent_transition(journal, &records.0, &records.1, &records.2)?;
        let (_, state) = transition.into_parts();
        let receipt = CommittedAgentStep {
            sequence: state.sequence(),
            phase: state.phase(),
            state_digest: state.state_digest(),
        };
        self.state = state;
        if clear_model_prefix {
            self.model_prefix.clear();
        }
        Ok(receipt)
    }

    /// Commits provider-attempt intent, then starts exactly one C5 request.
    ///
    /// A provider start error leaves the durable intent visible for retry/failure recovery.
    ///
    /// # Errors
    ///
    /// Rejects an installed session or propagates reducer, durability, provider, and protocol
    /// errors.
    #[allow(clippy::too_many_arguments, reason = "durable intent and C5 inputs remain explicit")]
    pub async fn start_model_once(
        &mut self,
        journal: &mut SqliteJournal,
        identity: TransitionIdentity,
        call_id: ModelCallId,
        budget: &mut AgentBudgetReservation,
        budget_port: &mut dyn AgentBudgetPort,
        activation_evidence: Sha256Digest,
        provider: &dyn ModelProvider,
        request: ModelRequest,
        limits: ProtocolLimits,
        cancellation: CancellationToken,
    ) -> Result<CommittedAgentStep, AgentDriverError> {
        if self.model.is_some() {
            return Err(AgentDriverError::RuntimeResourceUnavailable);
        }
        if budget.state() != AgentBudgetState::Held
            || budget.plan().revision() != self.state.binding().revision()
        {
            return Err(AgentBudgetError::InvalidPhase.into());
        }
        self.model_prefix.clear();
        let request_digest = request.fingerprint().map_err(ModelDriveError::from)?.digest();
        let receipt = self.drive_once(
            journal,
            identity,
            AgentCommandKind::ModelRequestStarted { call_id, request_digest },
        )?;
        let session = ModelSession::start(provider, request, limits, cancellation).await?;
        self.model = Some(session);
        budget.activate(budget_port, activation_evidence)?;
        Ok(receipt)
    }

    /// Pulls, validates, commits, and acknowledges at most one normalized provider envelope.
    ///
    /// # Errors
    ///
    /// Rejects a missing live session, malformed C5 event, pure-state disagreement, or durable
    /// commit failure. On commit failure the envelope remains pending and unacknowledged.
    pub async fn drive_model_once(
        &mut self,
        journal: &mut SqliteJournal,
        identity: TransitionIdentity,
    ) -> Result<ProviderAdvance, AgentDriverError> {
        let (sequence, bytes, envelope, preview) = {
            let model = self.model.as_mut().ok_or(AgentDriverError::RuntimeResourceUnavailable)?;
            match model.pull_one().await? {
                ModelAdvance::Closed => return Ok(ProviderAdvance::Closed),
                ModelAdvance::EnvelopePending { sequence, .. } => {
                    let bytes = model.encode_pending()?;
                    let envelope =
                        model.pending().cloned().ok_or(AgentDriverError::RuntimeInvariant)?;
                    let preview = model.preview_pending()?;
                    (sequence, bytes, envelope, preview)
                }
            }
        };
        let duplicate = matches!(preview, ReducerTransition::DuplicateIgnored);
        let cursor = if duplicate { self.state.model().cursor() } else { sequence };
        let output_bytes = if duplicate {
            0
        } else {
            u64::try_from(bytes.len()).map_err(|_| AgentDriverError::RuntimeInvariant)?
        };
        self.drive_once(
            journal,
            identity,
            AgentCommandKind::ProviderEventObserved(ProviderEventRecord::with_envelope(
                cursor,
                sha256(&bytes),
                output_bytes,
                duplicate,
                bytes,
            )?),
        )?;
        let observed = self
            .model
            .as_mut()
            .ok_or(AgentDriverError::RuntimeInvariant)?
            .accept_durable_pending()?;
        if observed != preview {
            return Err(AgentDriverError::RuntimeInvariant);
        }
        self.model_prefix.push(envelope);
        Ok(ProviderAdvance::Envelope { sequence, transition: observed })
    }

    /// Restores an exact provider continuation after a durable `ExactResume` retry transition.
    ///
    /// The normalized prefix is rebuilt from D0 event capsules before the provider is contacted.
    /// Unsupported restoration returns explicitly and performs no provider start. A restored
    /// attempt still requires a fresh B1 attempt/retry reservation and durable D0 start intent.
    ///
    /// # Errors
    ///
    /// Rejects missing exact-retry state, continuation/profile drift, invalid durable prefix,
    /// budget failure, reducer/durability failure, or provider restoration/start failure.
    #[allow(
        clippy::too_many_arguments,
        reason = "resume, budget, and durable intent stay explicit"
    )]
    pub async fn restore_model_once(
        &mut self,
        journal: &mut SqliteJournal,
        identity: TransitionIdentity,
        call_id: ModelCallId,
        budget: &mut AgentBudgetReservation,
        budget_port: &mut dyn AgentBudgetPort,
        activation_evidence: Sha256Digest,
        provider: &dyn ModelProvider,
        request: ModelRequest,
        limits: ProtocolLimits,
        cancellation: CancellationToken,
    ) -> Result<ContinuationRestoreOutcome, AgentDriverError> {
        if self.model.is_some()
            || self.state.phase() != AgentPhase::Active(crate::ActivePhase::RequestingModel)
            || !self.state.model().retry_pending()
            || !self.state.model().resume_exact()
            || self.model_prefix.is_empty()
        {
            return Err(ModelDriveError::InvalidContinuation.into());
        }
        if budget.state() != AgentBudgetState::Held
            || budget.plan().revision() != self.state.binding().revision()
        {
            return Err(AgentBudgetError::InvalidPhase.into());
        }
        let continuation = request
            .options()
            .continuation()
            .cloned()
            .ok_or(ModelDriveError::InvalidContinuation)?;
        let persisted = PersistedContinuation::new(
            self.state.binding().provider_profile_id(),
            self.state.binding().provider_profile_revision().get(),
            continuation.clone(),
        )
        .map_err(ModelDriveError::from)?;
        let outcome =
            provider.restore_continuation(&persisted).await.map_err(ModelDriveError::from)?;
        let ContinuationRestoreOutcome::Restored(restored) = &outcome else {
            return Ok(outcome);
        };
        if restored != &continuation {
            return Err(AgentDriverError::RuntimeInvariant);
        }
        let request_digest = request.fingerprint().map_err(ModelDriveError::from)?.digest();
        self.drive_once(
            journal,
            identity,
            AgentCommandKind::ModelRequestStarted { call_id, request_digest },
        )?;
        let session =
            ModelSession::resume(provider, request, limits, cancellation, &self.model_prefix)
                .await?;
        self.model = Some(session);
        budget.activate(budget_port, activation_evidence)?;
        Ok(outcome)
    }

    /// Derives the pure terminal record from the fully reduced C5 response.
    ///
    /// The supplied reservation can report settled only after its B1/C0 port accepted exact or
    /// conservative terminal accounting. D0 cannot claim that authority itself.
    ///
    /// # Errors
    ///
    /// Rejects a missing/unterminated live response or missing durable stream digest.
    pub fn model_terminal_record(
        &self,
        budget: &AgentBudgetReservation,
    ) -> Result<ModelTerminalRecord, AgentDriverError> {
        let terminal = self
            .model
            .as_ref()
            .and_then(ModelSession::terminal)
            .ok_or(AgentDriverError::RuntimeResourceUnavailable)?;
        let normal_terminal = matches!(
            terminal,
            TerminalOutcome::Succeeded { .. } | TerminalOutcome::RequiresAction { .. }
        );
        let response_digest =
            self.state.model().stream_digest().ok_or(AgentDriverError::RuntimeInvariant)?;
        Ok(ModelTerminalRecord::new(
            response_digest,
            normal_terminal,
            !normal_terminal,
            budget.is_settled(),
        ))
    }

    /// Commits the owned C5 session's cumulative usage through the active B1 reservation.
    ///
    /// The caller supplies accountable active time and a digest of the durable observation used by
    /// the B1/C0 port. A final observation closes the reservation before completion can progress.
    ///
    /// # Errors
    ///
    /// Rejects a missing session, invalid budget lifecycle, incomplete final provider accounting,
    /// arithmetic overflow, or a B1/C0 commit failure.
    pub fn observe_model_budget_once(
        &self,
        budget: &mut AgentBudgetReservation,
        budget_port: &mut dyn AgentBudgetPort,
        evidence_digest: Sha256Digest,
        active_effect_milliseconds: u64,
        finality: UsageFinality,
    ) -> Result<BudgetReceipt, AgentDriverError> {
        let usage = self
            .model
            .as_ref()
            .ok_or(AgentDriverError::RuntimeResourceUnavailable)?
            .usage_high_water();
        budget
            .observe_model(
                budget_port,
                evidence_digest,
                usage,
                active_effect_milliseconds,
                finality,
            )
            .map_err(Into::into)
    }

    /// Cancels the process-local provider stream immediately and idempotently.
    pub fn cancel_model(&self) {
        if let Some(model) = &self.model {
            model.cancel();
        }
    }

    /// Drops process-local effect owners after their logical terminal observations are committed.
    pub fn clear_runtime_resources(&mut self) {
        self.model = None;
        self.tools = None;
    }
}

impl fmt::Debug for AgentDriver {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentDriver")
            .field("turn_id", &self.state.binding().turn_id())
            .field("phase", &self.state.phase())
            .field("sequence", &self.state.sequence())
            .field("model_owned", &self.model.is_some())
            .field("model_prefix_events", &self.model_prefix.len())
            .field("tool_batch_owned", &self.tools.is_some())
            .finish_non_exhaustive()
    }
}

fn recover_model_prefix(events: &[AgentEvent]) -> Result<Vec<EventEnvelope>, AgentDriverError> {
    let mut prefix = Vec::new();
    let mut preserve_for_exact_resume = false;
    for event in events {
        let AgentEventKind::CommandAccepted(kind) = event.kind() else { continue };
        match kind {
            AgentCommandKind::ModelRequestStarted { .. } => {
                if !preserve_for_exact_resume {
                    prefix.clear();
                }
                preserve_for_exact_resume = false;
            }
            AgentCommandKind::ProviderEventObserved(record) => {
                if !record.encoded_envelope().is_empty() {
                    prefix.push(
                        decode_event_envelope(
                            record.encoded_envelope(),
                            ProtocolLimits::PRODUCTION,
                        )
                        .map_err(ModelDriveError::from)?,
                    );
                }
            }
            AgentCommandKind::ProviderRetryScheduled(record) => {
                preserve_for_exact_resume =
                    matches!(record.class(), ProviderRetryClass::ExactResume { .. });
                if !preserve_for_exact_resume {
                    prefix.clear();
                }
            }
            AgentCommandKind::ResultsRecorded { .. } => prefix.clear(),
            _ => {}
        }
    }
    Ok(prefix)
}
