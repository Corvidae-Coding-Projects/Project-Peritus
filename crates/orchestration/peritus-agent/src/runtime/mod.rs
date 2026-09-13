//! Ordinary effect composition around the verified agent reducer.

mod budget;
mod context;
mod driver;
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
pub use driver::{
    AgentDriver, AgentDriverError, CommittedAgentStep, ProviderAdvance, RecoveryReport,
    TransitionIdentity,
};
pub use durability::{
    AGENT_STATE_NAMESPACE, AgentDurabilityError, AgentReplay, agent_aggregate_key, agent_state_key,
    commit_agent_transition, load_agent_replay,
};
pub use model::{ModelAdvance, ModelDriveError, ModelSession};
pub use tools::{
    RuntimeToolPhase, RuntimeToolSlot, ToolBatchCoordinator, ToolDispatchAdvance, ToolDriveError,
    ToolInvocationPlan,
};
