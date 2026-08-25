//! Pure deterministic D0 agent-turn state machine.
#![allow(
    missing_docs,
    reason = "closed domain variants and self-describing accessors are documented at their owning types"
)]

mod canonical;
mod command;
mod completion;
mod error;
mod event;
mod identity;
mod limits;
mod phase;
mod reducer;
#[cfg(not(verus_only))]
mod runtime;
mod state;
mod tools;
mod verified;

#[cfg(feature = "protocol-bridge")]
mod protocol_bridge;

pub use command::{
    AgentCommand, AgentCommandKind, ContextRecord, ModelTerminalRecord, ProviderEventRecord,
    ProviderRetryClass, ProviderRetryRecord,
};
pub use completion::{CompletionProposal, CompletionRequest, EvidenceReference, TranscriptDigests};
pub use error::{AgentErrorCode, AgentOperation, AgentRecovery, AgentRejection};
pub use event::{AgentEvent, AgentEventKind};
pub use identity::{AgentBinding, ModelCallId, ProfileRevision, SafeText, ToolOrdinal};
pub use limits::{AgentCounters, AgentLimitDimension, AgentLimits};
pub use phase::{ActivePhase, AgentPhase, TerminalKind};
pub use reducer::{AgentTransition, reduce, replay, start};
#[cfg(not(verus_only))]
pub use runtime::{
    AGENT_STATE_NAMESPACE, AgentBudgetError, AgentBudgetPlan, AgentBudgetPort,
    AgentBudgetPortError, AgentBudgetReservation, AgentBudgetState, AgentDriver, AgentDriverError,
    AgentDurabilityError, AgentReplay, CommittedAgentStep, ContextDriveError, ContextPreparation,
    MemorySelection, ModelAdvance, ModelDriveError, ModelSession, ProviderAdvance, RecoveryReport,
    RuntimeToolPhase, RuntimeToolSlot, ToolBatchCoordinator, ToolDispatchAdvance, ToolDriveError,
    ToolInvocationPlan, TransitionIdentity, agent_aggregate_key, agent_state_key,
    commit_agent_transition, load_agent_replay, prepare_context, render_messages,
};
pub use state::{AgentFailure, AgentFailureKind, AgentTurnState, ModelState};
pub use tools::{
    ToolBatch, ToolIdempotency, ToolProposal, ToolResultRecord, ToolResultStatus, ToolSideEffect,
    ToolSlot, ToolSlotPhase, ToolVersion,
};
pub use verified::{
    completion_eligible, counter_within_limit, phase_transition_valid, proposal_has_no_effect,
    tool_result_order_valid,
};
