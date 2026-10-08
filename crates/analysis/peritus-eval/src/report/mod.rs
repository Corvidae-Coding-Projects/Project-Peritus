//! Deterministic analysis and immutable inert evaluation reports.

mod analysis;
mod canonical;
mod metric;
mod validation;

pub use analysis::{
    EvaluationAnalysis, EvaluationAnalysisBatch, EvaluationAnalysisCheckpoint, analyze_evaluation,
    analyze_evaluation_batch,
};
pub use metric::{
    ArmCorrectness, ArmResourceSummary, EvaluationReliability, MetricAvailability,
    MetricUnavailableReason, PairedEvidence, TaskPassAtK, TaskStability,
};
pub use validation::{EvaluationReport, ValidatedEvaluationReport};
