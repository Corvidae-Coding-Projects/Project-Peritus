//! Deterministic filter, rank, and budget retrieval planning.

use vstd::prelude::*;

verus! {

mod filter;
mod output;
mod plan;
mod ranking;
mod types;

pub use plan::retrieve;
pub use output::{
    CandidateExplanation, ExcludedMemory, ExclusionReason, MemoryCandidate, RankScore,
    RetrievalPlan,
};
pub use types::{
    FeedbackPolicy, RankingWeights, RequiredFeatures, RetrievalLimits, RetrievalPolicy,
    RetrievalQuery,
};
pub use types::MAX_RETRIEVAL_INPUTS;

} // verus!
