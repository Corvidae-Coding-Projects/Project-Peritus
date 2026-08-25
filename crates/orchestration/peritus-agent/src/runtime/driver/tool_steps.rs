//! Cooperative C4 preparation, authorization, execution, control, and recovery steps.

use peritus_codec::sha256;
use peritus_journal::SqliteJournal;
use peritus_tool_protocol::{CancellationReason, ToolControl};
use peritus_tool_router::{ToolAuthorizationRequest, ToolDispatcher, ToolRouter};
use peritus_types::{EvidenceId, Sha256Digest};

use crate::{
    ActivePhase, AgentCommandKind, AgentPhase, AgentTurnState, ToolOrdinal, ToolResultRecord,
    ToolResultStatus, ToolSlotPhase,
};

use super::{AgentDriver, AgentDriverError, CommittedAgentStep, TransitionIdentity};
use crate::runtime::{
    RuntimeToolPhase, RuntimeToolSlot, ToolBatchCoordinator, ToolDispatchAdvance,
};

#[allow(
    clippy::missing_errors_doc,
    reason = "the common checked driver boundary and per-method failure behavior are documented"
)]
impl AgentDriver {
    /// Commits inert tool proposals derived from a prepared/exposure-checked C4 batch.
    pub fn propose_tools_once(
        &mut self,
        journal: &mut SqliteJournal,
        identity: TransitionIdentity,
        coordinator: ToolBatchCoordinator,
        budget: &crate::runtime::AgentBudgetReservation,
    ) -> Result<CommittedAgentStep, AgentDriverError> {
        if self.tools.is_some() {
            return Err(AgentDriverError::RuntimeResourceUnavailable);
        }
        let terminal = self.model_terminal_record(budget)?;
        let proposals = coordinator
            .slots()
            .iter()
            .map(RuntimeToolSlot::agent_proposal)
            .collect::<Result<Vec<_>, _>>()?;
        let receipt = self.drive_once(
            journal,
            identity,
            AgentCommandKind::ToolCallsProposed { terminal, proposals },
        )?;
        self.model = None;
        self.tools = Some(coordinator);
        Ok(receipt)
    }

    /// Reattaches an effect-free prepared C4 batch to an already durable proposal phase.
    pub fn attach_prepared_tools(
        &mut self,
        coordinator: ToolBatchCoordinator,
    ) -> Result<(), AgentDriverError> {
        if self.tools.is_some()
            || self.state.phase() != AgentPhase::Active(ActivePhase::ProposedToolCalls)
        {
            return Err(AgentDriverError::RuntimeResourceUnavailable);
        }
        let durable = self.state.tools().ok_or(AgentDriverError::RuntimeInvariant)?;
        if durable.slots().len() != coordinator.slots().len() {
            return Err(AgentDriverError::RuntimeInvariant);
        }
        for (pure, runtime) in durable.slots().iter().zip(coordinator.slots()) {
            if pure.proposal() != &runtime.agent_proposal()? {
                return Err(AgentDriverError::RuntimeInvariant);
            }
        }
        self.tools = Some(coordinator);
        Ok(())
    }

    /// Commits the batch authorization phase, then marks each inert slot as awaiting it.
    pub fn request_tool_authorization_once(
        &mut self,
        journal: &mut SqliteJournal,
        identity: TransitionIdentity,
    ) -> Result<CommittedAgentStep, AgentDriverError> {
        let tools = self.tools.as_ref().ok_or(AgentDriverError::RuntimeResourceUnavailable)?;
        if tools.slots().iter().any(|slot| slot.phase() != RuntimeToolPhase::Prepared) {
            return Err(AgentDriverError::RuntimeInvariant);
        }
        let receipt = self.drive_once(journal, identity, AgentCommandKind::AuthorizationStarted)?;
        let tools = self.tools.as_mut().ok_or(AgentDriverError::RuntimeInvariant)?;
        for ordinal in 0..tools.slots().len() {
            tools.request_authorization(ordinal)?;
        }
        Ok(receipt)
    }

    /// Records independent authorization evidence without dispatching an effect.
    pub fn authorize_tool_once(
        &mut self,
        journal: &mut SqliteJournal,
        identity: TransitionIdentity,
        ordinal: ToolOrdinal,
        authority_digest: Sha256Digest,
    ) -> Result<CommittedAgentStep, AgentDriverError> {
        self.drive_once(
            journal,
            identity,
            AgentCommandKind::ToolAuthorized { ordinal, authority_digest },
        )
    }

    /// Records an independently denied call; no dispatcher is invoked.
    pub fn deny_tool_once(
        &mut self,
        journal: &mut SqliteJournal,
        identity: TransitionIdentity,
        ordinal: ToolOrdinal,
        result: ToolResultRecord,
    ) -> Result<CommittedAgentStep, AgentDriverError> {
        let index = usize::from(ordinal.get());
        let tools = self.tools.as_ref().ok_or(AgentDriverError::RuntimeResourceUnavailable)?;
        if tools.slots().get(index).map(RuntimeToolSlot::phase)
            != Some(RuntimeToolPhase::AwaitingAuthorization)
        {
            return Err(AgentDriverError::RuntimeInvariant);
        }
        let receipt =
            self.drive_once(journal, identity, AgentCommandKind::ToolDenied { ordinal, result })?;
        self.tools.as_mut().ok_or(AgentDriverError::RuntimeInvariant)?.deny(index)?;
        Ok(receipt)
    }

    /// Commits the transition from authorization into tool execution.
    pub fn begin_tool_execution_once(
        &mut self,
        journal: &mut SqliteJournal,
        identity: TransitionIdentity,
    ) -> Result<CommittedAgentStep, AgentDriverError> {
        self.drive_once(journal, identity, AgentCommandKind::ToolExecutionStarted)
    }

    /// Commits dispatch intent, then invokes the sole C4 router effect boundary once.
    #[allow(clippy::too_many_arguments, reason = "complete C4 authority remains explicit")]
    pub fn dispatch_tool_once(
        &mut self,
        journal: &mut SqliteJournal,
        identity: TransitionIdentity,
        ordinal: ToolOrdinal,
        router: &mut ToolRouter,
        authorization: &ToolAuthorizationRequest<'_>,
        dispatcher: &mut dyn ToolDispatcher,
    ) -> Result<ToolDispatchAdvance, AgentDriverError> {
        self.drive_once(journal, identity, AgentCommandKind::ToolDispatched { ordinal })?;
        self.tools
            .as_mut()
            .ok_or(AgentDriverError::RuntimeResourceUnavailable)?
            .dispatch(usize::from(ordinal.get()), router, authorization, dispatcher)
            .map_err(Into::into)
    }

    /// Records the current C4 slot observation after dispatch, control, polling, or cancellation.
    pub fn record_tool_observation_once(
        &mut self,
        journal: &mut SqliteJournal,
        identity: TransitionIdentity,
        ordinal: ToolOrdinal,
        evidence: Vec<EvidenceId>,
    ) -> Result<Option<CommittedAgentStep>, AgentDriverError> {
        let index = usize::from(ordinal.get());
        let phase = self
            .tools
            .as_ref()
            .and_then(|tools| tools.slots().get(index))
            .map(RuntimeToolSlot::phase)
            .ok_or(AgentDriverError::RuntimeResourceUnavailable)?;
        let kind = match phase {
            RuntimeToolPhase::Active
                if self
                    .state
                    .tools()
                    .and_then(|batch| batch.slots().get(index))
                    .is_some_and(|slot| slot.phase() == ToolSlotPhase::Dispatched) =>
            {
                AgentCommandKind::ToolActivated { ordinal }
            }
            RuntimeToolPhase::Active => return Ok(None),
            RuntimeToolPhase::Terminal => {
                let result = self
                    .tools
                    .as_ref()
                    .and_then(|tools| tools.slots().get(index))
                    .ok_or(AgentDriverError::RuntimeInvariant)?
                    .agent_result(evidence)?;
                AgentCommandKind::ToolCompleted { ordinal, result }
            }
            RuntimeToolPhase::Indeterminate => AgentCommandKind::ToolCompleted {
                ordinal,
                result: indeterminate_result(&self.state, ordinal)?,
            },
            RuntimeToolPhase::Prepared
            | RuntimeToolPhase::AwaitingAuthorization
            | RuntimeToolPhase::Denied => return Err(AgentDriverError::RuntimeInvariant),
        };
        self.drive_once(journal, identity, kind).map(Some)
    }

    /// Polls one active C4 invocation once and returns its new runtime phase.
    pub fn poll_tool_once(
        &mut self,
        ordinal: ToolOrdinal,
        router: &mut ToolRouter,
        observed_at: peritus_policy::AuthorityInstant,
    ) -> Result<RuntimeToolPhase, AgentDriverError> {
        let index = usize::from(ordinal.get());
        let tools = self.tools.as_mut().ok_or(AgentDriverError::RuntimeResourceUnavailable)?;
        tools.poll(index, router, observed_at)?;
        tools
            .slots()
            .get(index)
            .map(RuntimeToolSlot::phase)
            .ok_or(AgentDriverError::RuntimeInvariant)
    }

    /// Sends one supported control to a live C4 invocation without blocking the loop.
    pub fn control_tool_once(
        &mut self,
        ordinal: ToolOrdinal,
        router: &mut ToolRouter,
        control: ToolControl,
        observed_at: peritus_policy::AuthorityInstant,
    ) -> Result<RuntimeToolPhase, AgentDriverError> {
        let index = usize::from(ordinal.get());
        let tools = self.tools.as_mut().ok_or(AgentDriverError::RuntimeResourceUnavailable)?;
        tools.control(index, router, control, observed_at)?;
        tools
            .slots()
            .get(index)
            .map(RuntimeToolSlot::phase)
            .ok_or(AgentDriverError::RuntimeInvariant)
    }

    /// Requests cancellation of one live C4 invocation without blocking the loop.
    pub fn cancel_tool_once(
        &mut self,
        ordinal: ToolOrdinal,
        router: &mut ToolRouter,
        reason: CancellationReason,
        observed_at: peritus_policy::AuthorityInstant,
    ) -> Result<RuntimeToolPhase, AgentDriverError> {
        let index = usize::from(ordinal.get());
        let tools = self.tools.as_mut().ok_or(AgentDriverError::RuntimeResourceUnavailable)?;
        tools.cancel(index, router, reason, observed_at)?;
        tools
            .slots()
            .get(index)
            .map(RuntimeToolSlot::phase)
            .ok_or(AgentDriverError::RuntimeInvariant)
    }

    /// Classifies one dispatched/active tool lost across restart as indeterminate without C4 work.
    pub fn classify_lost_tool_once(
        &mut self,
        journal: &mut SqliteJournal,
        identity: TransitionIdentity,
        ordinal: ToolOrdinal,
    ) -> Result<CommittedAgentStep, AgentDriverError> {
        let phase = self
            .state
            .tools()
            .and_then(|batch| batch.slots().get(usize::from(ordinal.get())))
            .map(crate::ToolSlot::phase)
            .ok_or(AgentDriverError::RuntimeResourceUnavailable)?;
        if !matches!(phase, ToolSlotPhase::Dispatched | ToolSlotPhase::Active) {
            return Err(AgentDriverError::RuntimeInvariant);
        }
        let result = indeterminate_result(&self.state, ordinal)?;
        self.drive_once(journal, identity, AgentCommandKind::ToolCompleted { ordinal, result })
    }
}

fn indeterminate_result(
    state: &AgentTurnState,
    ordinal: ToolOrdinal,
) -> Result<ToolResultRecord, AgentDriverError> {
    let slot = state
        .tools()
        .and_then(|batch| batch.slots().get(usize::from(ordinal.get())))
        .ok_or(AgentDriverError::RuntimeResourceUnavailable)?;
    let mut bytes = b"peritus.agent.indeterminate-tool.v1\0".to_vec();
    bytes.extend_from_slice(slot.proposal().action_id().as_bytes());
    bytes.extend_from_slice(state.state_digest().as_bytes());
    ToolResultRecord::new(ToolResultStatus::Indeterminate, sha256(&bytes), 0, Vec::new())
        .map_err(Into::into)
}
