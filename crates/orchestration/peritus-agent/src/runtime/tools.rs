//! C4-only preparation, authority handoff, bounded execution, control, and result ordering.

use std::collections::BTreeSet;

use peritus_model_protocol::{
    CanonicalJson, CompletedToolCall, JsonBounds, ProtocolError as ModelProtocolError,
    ProtocolLimits as ModelProtocolLimits, ToolResult as ModelToolResult,
};
use peritus_policy::{AuthorityInstant, OperationClass};
use peritus_tool_protocol::{
    BoundedJson, CallLimits, CancellationReason, IdempotencyKey, JsonLimits, PreparedToolCall,
    ResultStatus, SemanticVersion, SideEffectClass, ToolCall, ToolControl, ToolResult,
};
use peritus_tool_router::{
    DispatchOutcome, ExposedTools, InvocationHandle, RecoveryOutcome, ReplayDisposition,
    RouterError, ToolAuthorizationRequest, ToolDispatcher, ToolRouter,
};
use peritus_types::{ActionId, CapabilityName, RevisionTuple};

use crate::{
    AgentRejection, ModelCallId, ToolIdempotency, ToolOrdinal, ToolProposal, ToolResultRecord,
    ToolResultStatus, ToolSideEffect, ToolVersion,
};

/// One complete mapping from an inert C5 call to an unprivileged C4 call envelope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolInvocationPlan {
    model_call: CompletedToolCall,
    call: ToolCall,
}

impl ToolInvocationPlan {
    /// Maps a fully reduced C5 tool call into bounded C4 protocol data without granting authority.
    ///
    /// # Errors
    ///
    /// Rejects names outside the C4 capability grammar and JSON that cannot satisfy C4's stricter
    /// integer-only canonical grammar or configured limits.
    #[allow(clippy::too_many_arguments, reason = "the unprivileged call binding is explicit")]
    pub fn from_model(
        model_call: CompletedToolCall,
        action_id: ActionId,
        version: SemanticVersion,
        limits: CallLimits,
        revision: RevisionTuple,
        deadline: AuthorityInstant,
        idempotency_key: IdempotencyKey,
        json_limits: JsonLimits,
    ) -> Result<Self, ToolDriveError> {
        let name = CapabilityName::new(model_call.name().as_str().to_owned())
            .map_err(|_| ToolDriveError::InvalidModelToolName)?;
        let arguments = core::str::from_utf8(model_call.arguments().canonical_bytes())
            .map_err(|_| ToolDriveError::InvalidModelArguments)
            .and_then(|value| {
                BoundedJson::parse(value, json_limits).map_err(ToolDriveError::ToolProtocol)
            })?;
        let call = ToolCall::new(
            action_id,
            name,
            version,
            arguments,
            limits,
            revision,
            deadline,
            idempotency_key,
        );
        Ok(Self { model_call, call })
    }

    /// Borrows the complete inert model proposal.
    #[must_use]
    pub const fn model_call(&self) -> &CompletedToolCall {
        &self.model_call
    }

    /// Borrows the unprivileged C4 call envelope.
    #[must_use]
    pub const fn call(&self) -> &ToolCall {
        &self.call
    }
}

/// Runtime state for one proposal slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeToolPhase {
    /// C4 preparation and current exposure checks succeeded without an effect.
    Prepared,
    /// The caller is assembling independent committed authority.
    AwaitingAuthorization,
    /// C4 owns a controllable active invocation.
    Active,
    /// A closed C4 terminal result was observed.
    Terminal,
    /// Independent authority was denied without dispatch.
    Denied,
    /// An effect may have occurred but success/retry cannot safely be inferred.
    Indeterminate,
}

/// One stable proposal-order slot and its C4 runtime observations.
#[derive(Clone, Debug)]
pub struct RuntimeToolSlot {
    ordinal: usize,
    model_call: CompletedToolCall,
    prepared: PreparedToolCall,
    phase: RuntimeToolPhase,
    handle: Option<InvocationHandle>,
    result: Option<ToolResult>,
}

impl RuntimeToolSlot {
    /// Returns the original model proposal ordinal.
    #[must_use]
    pub const fn ordinal(&self) -> usize {
        self.ordinal
    }
    /// Borrows the inert C5 proposal.
    #[must_use]
    pub const fn model_call(&self) -> &CompletedToolCall {
        &self.model_call
    }
    /// Borrows the exact C4 prepared call.
    #[must_use]
    pub const fn prepared(&self) -> &PreparedToolCall {
        &self.prepared
    }
    /// Returns the current runtime observation class.
    #[must_use]
    pub const fn phase(&self) -> RuntimeToolPhase {
        self.phase
    }
    /// Returns the live C4 handle when this process still owns it.
    #[must_use]
    pub const fn handle(&self) -> Option<InvocationHandle> {
        self.handle
    }
    /// Borrows a closed terminal result when observed.
    #[must_use]
    pub const fn result(&self) -> Option<&ToolResult> {
        self.result.as_ref()
    }

    /// Projects exact C4 preparation facts into the pure D0 inert proposal record.
    ///
    /// # Errors
    ///
    /// Rejects only an impossible zero model-call digest or invalid semantic major version.
    pub fn agent_proposal(&self) -> Result<ToolProposal, ToolDriveError> {
        let mut call_bytes = Vec::with_capacity(64);
        call_bytes.extend_from_slice(b"peritus.agent.model-tool-call.v1\0");
        call_bytes.extend_from_slice(self.model_call.id().expose_for_wire().as_bytes());
        let model_call_id = ModelCallId::new(peritus_codec::sha256(&call_bytes))?;
        let descriptor = self.prepared.descriptor();
        let version = ToolVersion::new(descriptor.version().major(), descriptor.version().minor())?;
        Ok(ToolProposal::new(
            ToolOrdinal::new(
                u16::try_from(self.ordinal).map_err(|_| ToolDriveError::InvalidBound)?,
            ),
            model_call_id,
            self.prepared.call().action_id(),
            self.prepared.call().name().clone(),
            version,
            self.prepared.arguments_digest(),
            self.prepared.prepared_digest(),
            self.prepared.replay_identity().digest(),
            self.prepared.call().revision(),
            self.prepared.call().deadline(),
            agent_side_effect(descriptor.side_effect()),
            agent_idempotency(descriptor.side_effect(), descriptor.idempotency()),
        ))
    }

    /// Projects a concrete C4 terminal envelope into the pure D0 result record.
    ///
    /// # Errors
    ///
    /// Rejects a slot without a concrete result or noncanonical evidence identities.
    pub fn agent_result(
        &self,
        evidence: Vec<peritus_types::EvidenceId>,
    ) -> Result<ToolResultRecord, ToolDriveError> {
        let result = self.result.as_ref().ok_or(ToolDriveError::ResultsIncomplete)?;
        ToolResultRecord::new(
            agent_result_status(result.status()),
            peritus_codec::sha256(&result.canonical_bytes()),
            u64::try_from(result.model_rendering().as_str().len())
                .map_err(|_| ToolDriveError::InvalidBound)?,
            evidence,
        )
        .map_err(Into::into)
    }
}

/// Immediate result of one independently authorized dispatch step.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolDispatchAdvance {
    /// The dispatcher or exact replay returned a terminal envelope.
    Terminal,
    /// C4 owns an active controllable invocation.
    Active,
    /// C4 reported a prior disposition that cannot be returned as success.
    Prior(ReplayDisposition),
}

/// Redaction-safe runtime tool composition failure.
#[derive(Debug)]
#[non_exhaustive]
pub enum ToolDriveError {
    /// The batch or fan-out bound was zero or exceeded.
    InvalidBound,
    /// Model tool name cannot be represented by the C4 capability grammar.
    InvalidModelToolName,
    /// Model arguments cannot be represented by C4's JSON grammar.
    InvalidModelArguments,
    /// A prepared descriptor is not in the current independently computed exposure.
    ToolNotExposed,
    /// Duplicate action identities appeared in one proposal batch.
    DuplicateAction,
    /// Slot index or requested state transition was invalid.
    InvalidSlotState,
    /// Bounded fan-out or mutation serialization prevents another dispatch now.
    DispatchCapacity,
    /// Not every slot has a concrete terminal result in proposal order.
    ResultsIncomplete,
    /// C4 protocol construction failed.
    ToolProtocol(peritus_tool_protocol::ProtocolError),
    /// C4 router preparation, authorization, dispatch, or control failed.
    Router(RouterError),
    /// C5 result rendering failed.
    ModelProtocol(ModelProtocolError),
    /// The pure D0 record rejected a C4 projection.
    Agent(AgentRejection),
}

impl core::fmt::Display for ToolDriveError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidBound => formatter.write_str("tool batch or fan-out bound is invalid"),
            Self::InvalidModelToolName => formatter.write_str("model tool name is not a C4 name"),
            Self::InvalidModelArguments => {
                formatter.write_str("model tool arguments are not valid C4 JSON")
            }
            Self::ToolNotExposed => formatter.write_str("tool is not in the current exposure"),
            Self::DuplicateAction => formatter.write_str("tool action identity is duplicated"),
            Self::InvalidSlotState => formatter.write_str("tool slot state transition is invalid"),
            Self::DispatchCapacity => {
                formatter.write_str("tool dispatch fan-out or serialization bound is reached")
            }
            Self::ResultsIncomplete => {
                formatter.write_str("tool results are not complete in every proposal slot")
            }
            Self::ToolProtocol(error) => core::fmt::Display::fmt(error, formatter),
            Self::Router(error) => core::fmt::Display::fmt(error, formatter),
            Self::ModelProtocol(error) => core::fmt::Display::fmt(error, formatter),
            Self::Agent(error) => core::fmt::Display::fmt(error, formatter),
        }
    }
}

impl std::error::Error for ToolDriveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ToolProtocol(error) => Some(error),
            Self::Router(error) => Some(error),
            Self::ModelProtocol(error) => Some(error),
            Self::Agent(error) => Some(error),
            Self::InvalidBound
            | Self::InvalidModelToolName
            | Self::InvalidModelArguments
            | Self::ToolNotExposed
            | Self::DuplicateAction
            | Self::InvalidSlotState
            | Self::DispatchCapacity
            | Self::ResultsIncomplete => None,
        }
    }
}

impl From<RouterError> for ToolDriveError {
    fn from(value: RouterError) -> Self {
        Self::Router(value)
    }
}

impl From<AgentRejection> for ToolDriveError {
    fn from(value: AgentRejection) -> Self {
        Self::Agent(value)
    }
}

/// Bounded proposal-order coordinator around the sole C4 router effect boundary.
#[derive(Debug)]
pub struct ToolBatchCoordinator {
    slots: Vec<RuntimeToolSlot>,
    max_parallel: usize,
}

impl ToolBatchCoordinator {
    /// Prepares and exposure-checks a complete nonempty model proposal batch without effects.
    ///
    /// # Errors
    ///
    /// Rejects invalid bounds, duplicate actions, proposal/call mismatch, C4 preparation failure,
    /// or descriptors hidden from the current role/capability exposure.
    pub fn prepare(
        router: &ToolRouter,
        exposed: &ExposedTools,
        plans: Vec<ToolInvocationPlan>,
        max_calls: usize,
        max_parallel: usize,
    ) -> Result<Self, ToolDriveError> {
        if plans.is_empty()
            || max_calls == 0
            || max_parallel == 0
            || plans.len() > max_calls
            || max_parallel > max_calls
        {
            return Err(ToolDriveError::InvalidBound);
        }
        let mut actions = BTreeSet::new();
        let mut slots = Vec::with_capacity(plans.len());
        for (ordinal, plan) in plans.into_iter().enumerate() {
            if plan.model_call.name().as_str() != plan.call.name().as_str()
                || plan.model_call.arguments().canonical_bytes()
                    != plan.call.arguments().canonical_bytes()
            {
                return Err(ToolDriveError::InvalidModelArguments);
            }
            if !actions.insert(plan.call.action_id()) {
                return Err(ToolDriveError::DuplicateAction);
            }
            let prepared = router.prepare(plan.call)?;
            if !exposed.contains(prepared.descriptor()) {
                return Err(ToolDriveError::ToolNotExposed);
            }
            slots.push(RuntimeToolSlot {
                ordinal,
                model_call: plan.model_call,
                prepared,
                phase: RuntimeToolPhase::Prepared,
                handle: None,
                result: None,
            });
        }
        Ok(Self { slots, max_parallel })
    }

    /// Borrows slots in original model proposal order.
    #[must_use]
    pub fn slots(&self) -> &[RuntimeToolSlot] {
        &self.slots
    }

    /// Returns the configured maximum simultaneous active calls.
    #[must_use]
    pub const fn max_parallel(&self) -> usize {
        self.max_parallel
    }

    /// Marks that a slot is waiting for independent B0/B1/C0 authority.
    ///
    /// # Errors
    ///
    /// Rejects an unknown slot or a slot that has moved past effect-free preparation.
    pub fn request_authorization(&mut self, ordinal: usize) -> Result<(), ToolDriveError> {
        let slot = self.slot_mut(ordinal)?;
        if slot.phase != RuntimeToolPhase::Prepared {
            return Err(ToolDriveError::InvalidSlotState);
        }
        slot.phase = RuntimeToolPhase::AwaitingAuthorization;
        Ok(())
    }

    /// Records independent denial without invoking C4 dispatch.
    ///
    /// # Errors
    ///
    /// Rejects an unknown slot or a slot outside authorization wait.
    pub fn deny(&mut self, ordinal: usize) -> Result<(), ToolDriveError> {
        let slot = self.slot_mut(ordinal)?;
        if slot.phase != RuntimeToolPhase::AwaitingAuthorization {
            return Err(ToolDriveError::InvalidSlotState);
        }
        slot.phase = RuntimeToolPhase::Denied;
        Ok(())
    }

    /// Dispatches one slot using a complete independently assembled C4 authority request.
    ///
    /// # Errors
    ///
    /// Rejects invalid slot state, bounded-fan-out/mutation conflicts, incomplete authority,
    /// replay conflicts, dispatcher mismatch, or malformed observations.
    pub fn dispatch(
        &mut self,
        ordinal: usize,
        router: &mut ToolRouter,
        authorization: &ToolAuthorizationRequest<'_>,
        dispatcher: &mut dyn ToolDispatcher,
    ) -> Result<ToolDispatchAdvance, ToolDriveError> {
        self.ensure_dispatch_capacity(ordinal)?;
        let slot = self.slot_mut(ordinal)?;
        let outcome = router.dispatch(slot.prepared.clone(), authorization, dispatcher)?;
        match outcome {
            DispatchOutcome::Completed(result) | DispatchOutcome::Replayed(result) => {
                slot.result = Some(result);
                slot.phase = RuntimeToolPhase::Terminal;
                Ok(ToolDispatchAdvance::Terminal)
            }
            DispatchOutcome::Active(handle) => {
                slot.handle = Some(handle);
                slot.phase = RuntimeToolPhase::Active;
                Ok(ToolDispatchAdvance::Active)
            }
            DispatchOutcome::PriorOutcome(disposition) => {
                slot.phase = RuntimeToolPhase::Indeterminate;
                Ok(ToolDispatchAdvance::Prior(disposition))
            }
        }
    }

    /// Polls one active invocation without blocking the agent loop.
    ///
    /// # Errors
    ///
    /// Rejects missing ownership or a malformed C4 observation.
    pub fn poll(
        &mut self,
        ordinal: usize,
        router: &mut ToolRouter,
        observed_at: AuthorityInstant,
    ) -> Result<(), ToolDriveError> {
        let handle = self.active_handle(ordinal)?;
        let update = router.poll(handle, observed_at)?;
        self.accept_update(ordinal, update.terminal().cloned())
    }

    /// Sends one descriptor-supported control to an active invocation.
    ///
    /// # Errors
    ///
    /// Rejects missing ownership, unsupported control, or malformed C4 observations.
    pub fn control(
        &mut self,
        ordinal: usize,
        router: &mut ToolRouter,
        control: ToolControl,
        observed_at: AuthorityInstant,
    ) -> Result<(), ToolDriveError> {
        let handle = self.active_handle(ordinal)?;
        let update = router.control(handle, control, observed_at)?;
        self.accept_update(ordinal, update.terminal().cloned())
    }

    /// Requests cancellation while retaining ownership until a terminal observation.
    ///
    /// # Errors
    ///
    /// Rejects missing ownership or malformed C4 cancellation observations.
    pub fn cancel(
        &mut self,
        ordinal: usize,
        router: &mut ToolRouter,
        reason: CancellationReason,
        observed_at: AuthorityInstant,
    ) -> Result<(), ToolDriveError> {
        let handle = self.active_handle(ordinal)?;
        let update = router.cancel(handle, reason, observed_at)?;
        self.accept_update(ordinal, update.terminal().cloned())
    }

    /// Asks C4 to recover one still-owned active handle.
    ///
    /// # Errors
    ///
    /// Rejects missing ownership or malformed C4 recovery observations.
    pub fn recover(
        &mut self,
        ordinal: usize,
        router: &mut ToolRouter,
        observed_at: AuthorityInstant,
    ) -> Result<(), ToolDriveError> {
        let handle = self.active_handle(ordinal)?;
        match router.recover(handle, observed_at)? {
            RecoveryOutcome::Active(_) => Ok(()),
            RecoveryOutcome::Completed(result) => self.accept_update(ordinal, Some(result)),
            RecoveryOutcome::Indeterminate(_) => {
                let slot = self.slot_mut(ordinal)?;
                slot.handle = None;
                slot.phase = RuntimeToolPhase::Indeterminate;
                Ok(())
            }
        }
    }

    /// Classifies every process-local active handle lost across restart as indeterminate.
    ///
    /// This consumes no dispatcher and deliberately provides no redispatch operation.
    pub fn classify_active_lost_after_restart(&mut self) -> usize {
        let mut classified = 0;
        for slot in &mut self.slots {
            if slot.phase == RuntimeToolPhase::Active {
                slot.handle = None;
                slot.phase = RuntimeToolPhase::Indeterminate;
                classified += 1;
            }
        }
        classified
    }

    /// Returns true when every slot has a closed non-active classification.
    #[must_use]
    pub fn is_settled(&self) -> bool {
        self.slots.iter().all(|slot| {
            matches!(
                slot.phase,
                RuntimeToolPhase::Terminal
                    | RuntimeToolPhase::Denied
                    | RuntimeToolPhase::Indeterminate
            )
        })
    }

    /// Borrows all concrete terminal results in stable model proposal order.
    ///
    /// # Errors
    ///
    /// Rejects any nonterminal, denied, or indeterminate slot because those require an explicit
    /// synthetic D0 failure record rather than an invented C4 result.
    pub fn ordered_results(&self) -> Result<Vec<&ToolResult>, ToolDriveError> {
        self.slots
            .iter()
            .map(|slot| slot.result.as_ref().ok_or(ToolDriveError::ResultsIncomplete))
            .collect()
    }

    /// Produces bounded C5 tool-result messages in original proposal order.
    ///
    /// # Errors
    ///
    /// Rejects incomplete slots or results outside the selected C5 JSON bounds.
    pub fn ordered_model_results(
        &self,
        limits: ModelProtocolLimits,
    ) -> Result<Vec<ModelToolResult>, ToolDriveError> {
        self.slots
            .iter()
            .map(|slot| {
                let result = slot.result.as_ref().ok_or(ToolDriveError::ResultsIncomplete)?;
                let structured = result
                    .structured()
                    .map(|value| {
                        serde_json::from_slice::<serde_json::Value>(value.canonical_bytes())
                            .map_err(|_| ToolDriveError::InvalidModelArguments)
                    })
                    .transpose()?;
                let mut object = serde_json::Map::new();
                object.insert(
                    "status".to_owned(),
                    serde_json::Value::String(result_status(result.status()).to_owned()),
                );
                object.insert(
                    "output".to_owned(),
                    serde_json::Value::String(result.model_rendering().as_str().to_owned()),
                );
                object
                    .insert("structured".to_owned(), structured.unwrap_or(serde_json::Value::Null));
                object.insert(
                    "truncation".to_owned(),
                    serde_json::Value::String(truncation(result.truncation().model).to_owned()),
                );
                let value = serde_json::Value::Object(object);
                let canonical = CanonicalJson::parse(&value.to_string(), JsonBounds::value(limits))
                    .map_err(ToolDriveError::ModelProtocol)?;
                Ok(ModelToolResult::new(
                    slot.model_call.id().clone(),
                    canonical,
                    result.status() != ResultStatus::Succeeded,
                ))
            })
            .collect()
    }

    fn ensure_dispatch_capacity(&self, ordinal: usize) -> Result<(), ToolDriveError> {
        let slot = self.slots.get(ordinal).ok_or(ToolDriveError::InvalidSlotState)?;
        if slot.ordinal != ordinal || slot.phase != RuntimeToolPhase::AwaitingAuthorization {
            return Err(ToolDriveError::InvalidSlotState);
        }
        let active: Vec<&RuntimeToolSlot> = self
            .slots
            .iter()
            .filter(|candidate| candidate.phase == RuntimeToolPhase::Active)
            .collect();
        if active.len() >= self.max_parallel
            || (!parallel_safe(slot) && !active.is_empty())
            || active.iter().any(|candidate| !parallel_safe(candidate))
        {
            return Err(ToolDriveError::DispatchCapacity);
        }
        Ok(())
    }

    fn active_handle(&self, ordinal: usize) -> Result<InvocationHandle, ToolDriveError> {
        let slot = self.slots.get(ordinal).ok_or(ToolDriveError::InvalidSlotState)?;
        if slot.ordinal != ordinal || slot.phase != RuntimeToolPhase::Active {
            return Err(ToolDriveError::InvalidSlotState);
        }
        slot.handle.ok_or(ToolDriveError::InvalidSlotState)
    }

    fn accept_update(
        &mut self,
        ordinal: usize,
        terminal: Option<ToolResult>,
    ) -> Result<(), ToolDriveError> {
        let slot = self.slot_mut(ordinal)?;
        if slot.phase != RuntimeToolPhase::Active {
            return Err(ToolDriveError::InvalidSlotState);
        }
        if let Some(result) = terminal {
            slot.handle = None;
            slot.result = Some(result);
            slot.phase = RuntimeToolPhase::Terminal;
        }
        Ok(())
    }

    fn slot_mut(&mut self, ordinal: usize) -> Result<&mut RuntimeToolSlot, ToolDriveError> {
        let slot = self.slots.get_mut(ordinal).ok_or(ToolDriveError::InvalidSlotState)?;
        if slot.ordinal != ordinal {
            return Err(ToolDriveError::InvalidSlotState);
        }
        Ok(slot)
    }
}

fn parallel_safe(slot: &RuntimeToolSlot) -> bool {
    let descriptor = slot.prepared.descriptor();
    matches!(descriptor.side_effect(), SideEffectClass::None | SideEffectClass::Process)
        && matches!(
            descriptor.operation().operation_class(),
            OperationClass::Inspection | OperationClass::Execution
        )
}

const fn agent_side_effect(value: SideEffectClass) -> ToolSideEffect {
    match value {
        SideEffectClass::None => ToolSideEffect::None,
        SideEffectClass::Workspace => ToolSideEffect::Workspace,
        SideEffectClass::Process => ToolSideEffect::Process,
        SideEffectClass::External => ToolSideEffect::External,
    }
}

const fn agent_idempotency(
    side_effect: SideEffectClass,
    value: peritus_tool_protocol::IdempotencySemantics,
) -> ToolIdempotency {
    match (side_effect, value) {
        (SideEffectClass::None, peritus_tool_protocol::IdempotencySemantics::ReplayTerminal) => {
            ToolIdempotency::Idempotent
        }
        (_, peritus_tool_protocol::IdempotencySemantics::ReplayTerminal) => {
            ToolIdempotency::ReplayTerminalOnly
        }
        (_, peritus_tool_protocol::IdempotencySemantics::ReportPriorOutcome) => {
            ToolIdempotency::NonIdempotent
        }
    }
}

const fn agent_result_status(value: ResultStatus) -> ToolResultStatus {
    match value {
        ResultStatus::Succeeded => ToolResultStatus::Succeeded,
        ResultStatus::Failed | ResultStatus::TimedOut => ToolResultStatus::Failed,
        ResultStatus::Cancelled => ToolResultStatus::Cancelled,
        ResultStatus::Indeterminate => ToolResultStatus::Indeterminate,
    }
}

const fn result_status(value: ResultStatus) -> &'static str {
    match value {
        ResultStatus::Succeeded => "succeeded",
        ResultStatus::Failed => "failed",
        ResultStatus::Cancelled => "cancelled",
        ResultStatus::TimedOut => "timed-out",
        ResultStatus::Indeterminate => "indeterminate",
    }
}

const fn truncation(value: peritus_tool_protocol::Truncation) -> &'static str {
    match value {
        peritus_tool_protocol::Truncation::Complete => "complete",
        peritus_tool_protocol::Truncation::TailDropped => "tail-dropped",
        peritus_tool_protocol::Truncation::HeadDropped => "head-dropped",
        peritus_tool_protocol::Truncation::Windowed => "windowed",
        peritus_tool_protocol::Truncation::Indeterminate => "indeterminate",
    }
}
