//! Immutable dataset manifests and candidate/evaluator isolation.

mod manifest;
mod partition;
mod task;

pub use manifest::DatasetManifest;
pub use partition::DatasetPartition;
pub use task::{CandidateTaskInput, DatasetTask, SealedEvaluatorInput};
