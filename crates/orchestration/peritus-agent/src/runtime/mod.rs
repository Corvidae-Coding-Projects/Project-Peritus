//! Ordinary effect composition around the verified agent reducer.

mod budget;
mod context;
#[cfg(feature = "protocol-bridge")]
mod driver;
#[cfg(feature = "protocol-bridge")]
mod durability;
mod model;
mod tools;

pub use budget::{
    AgentBudgetError, AgentBudgetPlan, AgentBudgetPort, AgentBudgetPortError,
    AgentBudgetReservation, AgentBudgetState,
};
pub use context::{
    ContextDriveError, ContextPreparation, MemorySelection, prepare_context, render_messages,
};
#[cfg(feature = "protocol-bridge")]
pub use driver::{
    AgentDriver, AgentDriverError, CommittedAgentStep, ProviderAdvance, RecoveryReport,
    TransitionIdentity,
};
#[cfg(feature = "protocol-bridge")]
pub use durability::{
    AGENT_STATE_NAMESPACE, AgentDurabilityError, AgentReplay, agent_aggregate_key, agent_state_key,
    commit_agent_transition, load_agent_replay,
};
pub use model::{ModelAdvance, ModelDriveError, ModelSession};
pub use tools::{
    RuntimeToolPhase, RuntimeToolSlot, ToolBatchCoordinator, ToolDispatchAdvance, ToolDriveError,
    ToolInvocationPlan,
};
