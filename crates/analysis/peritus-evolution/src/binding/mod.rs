//! Checked bridges from immutable owning-slice values into F0 evidence.

mod diagnosis;
mod evaluation;
mod policy;
mod production;
mod review;

pub use diagnosis::{DiagnosisCitation, PublishedDebuggerEvidence};
pub(crate) use evaluation::reason_tag;
pub use evaluation::{
    EvaluationAnalysisSnapshot, EvaluationMetric, PublishedEvaluationEvidence, TaskPassAtKSnapshot,
};
pub use policy::{Objective, PromotionPolicy, PromotionPolicyBinding, PromotionThresholds};
pub use production::{InstalledSnapshotBinding, ProductionHarnessBinding};
pub use review::PromotionReviewEvidence;
