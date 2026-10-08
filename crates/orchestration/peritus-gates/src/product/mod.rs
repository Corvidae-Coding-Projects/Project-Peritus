//! Exact-target production gate planning and evidence aggregation.

mod commands;
mod plan;
mod report;
mod workspace_scope;

pub use plan::{
    AffectedProject, GateCommandSpec, GateNetworkPolicy, ProjectKind, TargetGatePlan,
};
pub use report::{GateExecutionRecord, TargetGateReport};
pub use workspace_scope::WorkspaceProductScope;
