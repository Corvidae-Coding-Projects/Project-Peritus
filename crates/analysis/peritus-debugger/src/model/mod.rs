//! Provider-neutral, tool-free optional model analysis.

mod plan;
mod proposal;
mod runner;

pub use plan::{
    MODEL_PROPOSAL_SCHEMA, ModelAnalysisPlan, messages_from_render_plan, model_proposal_schema,
};
pub use proposal::{ModelFinding, ModelRecommendation, ValidatedModelProposal};
pub use runner::{ModelRunFailure, ModelRunSuccess, run_model_analysis};
pub(crate) use runner::{ModelPriorUsage, run_model_analysis_with_usage};
