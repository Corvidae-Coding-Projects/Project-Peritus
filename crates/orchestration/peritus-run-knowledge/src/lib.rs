//! Verified digest-bound run knowledge and role-delta planning for Peritus.
//!
//! The crate owns pure freshness and reuse decisions. Filesystem observation, hashing, persistence,
//! prompt rendering, and provider effects remain outside this boundary.

use vstd::prelude::*;

verus! {

mod binding;
mod change;
mod delta;
mod error;
mod identity;
mod kind;
mod limits;
mod plan;
mod section;
mod snapshot;
mod source;
pub mod verified;

pub use binding::KnowledgeBinding;
pub use change::{CurrentKnowledgeState, InvalidationRequest, KnowledgeChange};
pub use delta::{DeltaAccounting, DeltaDelivery, DeltaEntry, DeltaPacket, plan_delta_packet};
pub use error::{KnowledgeError, KnowledgeErrorKind};
pub use identity::{KnowledgeSectionId, KnowledgeSourceId};
pub use kind::{KnowledgeAuthority, KnowledgeSectionKind};
pub use limits::KnowledgeLimits;
pub use plan::{
    InvalidationPlan, InvalidationReason, PlannedKnowledge, ReuseAccounting, ReuseDecision,
    plan_invalidation,
};
pub use section::KnowledgeSection;
pub use snapshot::RunKnowledgeSnapshot;
pub use source::SourceDigest;
pub use verified::{ReusePremiseStatus, ReusePremises};
pub use peritus_run_settlement::CandidateIdentity;

} // verus!
