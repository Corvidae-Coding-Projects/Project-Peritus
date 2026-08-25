//! Verified scoped derived-memory lifecycle and retrieval for Peritus.
//!
//! Memory is immutable, evidence-backed, non-authoritative data. This crate performs no I/O and
//! exposes no capability, acceptance, waiver, amendment, or promotion operation.

use vstd::prelude::*;

verus! {

mod claim;
mod confidence;
mod error;
mod evidence;
mod feedback;
mod identity;
mod index;
mod lifecycle;
mod record;
mod retrieval;
mod scope;
mod tombstone;
mod verified;

pub use claim::{
    ClaimType, ClaimTypeSet, MemoryMaterial, RetrievalFeature, RetrievalFeatures, SourceProvenance,
};
pub use confidence::{BasisPoints, Confidence, FeatureWeight};
pub use error::{MemoryError, MemoryErrorKind, MemoryField};
pub use evidence::{EvidenceSet, SourceEventSet};
pub use feedback::Feedback;
pub use identity::{FeatureKey, MemoryId, RepositoryId};
pub use index::{ClaimPosting, FeaturePosting, MemoryIndex, ScopePosting};
pub use lifecycle::{
    DeletionReason, MemoryState, Observation, QuarantineReason, StateSnapshot,
};
pub use record::{MemoryEvidence, MemoryRecord, MemoryTiming};
pub use retrieval::{
    CandidateExplanation, ExcludedMemory, ExclusionReason, FeedbackPolicy, MemoryCandidate,
    RankScore, RequiredFeatures, RetrievalLimits, RetrievalPlan, RetrievalPolicy, RetrievalQuery,
    RankingWeights, retrieve,
};
pub use scope::{MemoryScope, ScopeKind, ScopePolicy};
pub use tombstone::MemoryTombstone;
pub use verified::{
    deletion_dominates, lifecycle_advanced, memory_is_non_authority, retrieval_is_bounded,
};

} // verus!
