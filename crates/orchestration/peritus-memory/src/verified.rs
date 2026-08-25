//! Executable predicates used by C6 verification roots and ordinary callers.

use crate::{MemoryCandidate, MemoryRecord, MemoryTombstone, RetrievalPlan};
use vstd::prelude::*;

verus! {

/// Returns the invariant that selected memory is delimited derived evidence, never authority.
#[must_use]
pub const fn memory_is_non_authority(candidate: &MemoryCandidate) -> (result: bool)
    ensures result == candidate.spec_is_quoted_evidence(),
{
    candidate.quoted_evidence()
}

/// Returns whether a new immutable record strictly advances revision and observation.
#[must_use]
pub fn lifecycle_advanced(old: &MemoryRecord, new: &MemoryRecord) -> (result: bool)
    ensures
        result ==> old.spec_revision_value() < new.spec_revision_value(),
{
    old.id() == new.id()
        && old.revision().get() < new.revision().get()
        && old.latest_observation() < new.latest_observation()
}

/// Returns exactly when a deletion marker suppresses a replayed record revision.
#[must_use]
pub const fn deletion_dominates(
    tombstone: &MemoryTombstone,
    record: &MemoryRecord,
) -> (result: bool)
    ensures
        result == tombstone.spec_dominates(record),
{
    tombstone.dominates(record)
}

/// Returns whether selected estimated tokens remain within the declared query budget.
#[must_use]
pub const fn retrieval_is_bounded(plan: &RetrievalPlan) -> (result: bool)
    ensures result == plan.spec_is_bounded(),
{
    plan.used_tokens() <= plan.token_budget()
}

} // verus!
