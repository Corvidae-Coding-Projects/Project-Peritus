//! Published E3 evaluation capture with exact campaign-arm identity.

mod analysis;
mod evidence;

pub(crate) use analysis::reason_tag;
pub use analysis::{EvaluationAnalysisSnapshot, EvaluationMetric, TaskPassAtKSnapshot};
pub use evidence::PublishedEvaluationEvidence;
