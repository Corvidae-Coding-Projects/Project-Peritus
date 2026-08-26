//! Deterministic analysis and immutable inert evaluation reports.

mod analysis;
mod canonical;
mod metric;
mod validation;

pub use analysis::{EvaluationAnalysis, analyze_evaluation};
pub use metric::{
    ArmCorrectness, ArmResourceSummary, EvaluationReliability, MetricAvailability,
    MetricUnavailableReason, PairedEvidence, TaskPassAtK, TaskStability,
};
pub use validation::{EvaluationReport, ValidatedEvaluationReport};
