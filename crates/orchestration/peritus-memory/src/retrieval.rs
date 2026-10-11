//! Deterministic filter, rank, and budget retrieval planning.

mod filter;
mod output;
mod plan;
mod ranking;
mod types;
pub use output::{
    CandidateExplanation, ExcludedMemory, ExclusionReason, MemoryCandidate, RankScore,
    RetrievalPlan,
};
pub use plan::retrieve;
pub use types::MAX_RETRIEVAL_INPUTS;
pub use types::{
    FeedbackPolicy, RankingWeights, RequiredFeatures, RetrievalLimits, RetrievalPolicy,
    RetrievalQuery,
};
