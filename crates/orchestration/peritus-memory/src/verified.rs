//! Executable predicates used by C6 verification roots and ordinary callers.

use crate::{MemoryCandidate, MemoryRecord, MemoryTombstone, RetrievalPlan};

#[doc = " Returns the invariant that selected memory is delimited derived evidence, never authority."]
#[must_use]
pub const fn memory_is_non_authority(candidate: &MemoryCandidate) -> bool {
    candidate.quoted_evidence()
}
#[doc = " Returns whether a new immutable record strictly advances revision and observation."]
#[must_use]
pub fn lifecycle_advanced(old: &MemoryRecord, new: &MemoryRecord) -> bool {
    old.id() == new.id()
        && old.revision().get() < new.revision().get()
        && old.latest_observation() < new.latest_observation()
}
#[doc = " Returns exactly when a deletion marker suppresses a replayed record revision."]
#[must_use]
pub const fn deletion_dominates(tombstone: &MemoryTombstone, record: &MemoryRecord) -> bool {
    tombstone.dominates(record)
}
#[doc = " Returns whether selected estimated tokens remain within the declared query budget."]
#[must_use]
pub const fn retrieval_is_bounded(plan: &RetrievalPlan) -> bool {
    plan.used_tokens() <= plan.token_budget()
}
