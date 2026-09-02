//! Exact-target production gate planning and evidence aggregation.

mod commands;
mod plan;
mod report;

pub use plan::{
    AffectedProject, GateCommandSpec, PRODUCT_MAX_SOURCE_LINES, ProjectKind, TargetGatePlan,
};
pub use report::{GateExecutionRecord, TargetGateReport};
