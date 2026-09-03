//! Executable refinement predicates for reuse and authority invariants.

use crate::KnowledgeAuthority;
use vstd::prelude::*;

verus! {

/// Result of checking one independent reuse premise.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReusePremiseStatus {
    /// The premise is current and satisfied.
    Satisfied,
    /// The premise is missing, stale, or false.
    Failed,
}

impl ReusePremiseStatus {
    /// Converts one checked predicate into a typed premise status.
    #[must_use]
    pub const fn from_satisfied(satisfied: bool) -> Self {
        if satisfied { Self::Satisfied } else { Self::Failed }
    }
}

/// Independent currentness premises required before prior knowledge may be reused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReusePremises {
    /// Run and workspace identities are unchanged.
    pub same_lineage: ReusePremiseStatus,
    /// The retained view belongs to the target role.
    pub role_matches: ReusePremiseStatus,
    /// Every bound source digest is current.
    pub sources_current: ReusePremiseStatus,
    /// The retained observation was not created in the future.
    pub sequence_not_future: ReusePremiseStatus,
    /// Conversation and candidate revisions satisfy the section kind.
    pub revision_current: ReusePremiseStatus,
    /// Every section dependency is still current.
    pub dependencies_current: ReusePremiseStatus,
}

impl ReusePremises {
    /// Collects the six independent fail-closed reuse premises.
    #[must_use]
    pub const fn new(
        same_lineage: ReusePremiseStatus,
        role_matches: ReusePremiseStatus,
        sources_current: ReusePremiseStatus,
        sequence_not_future: ReusePremiseStatus,
        revision_current: ReusePremiseStatus,
        dependencies_current: ReusePremiseStatus,
    ) -> Self {
        Self {
            same_lineage,
            role_matches,
            sources_current,
            sequence_not_future,
            revision_current,
            dependencies_current,
        }
    }
}

/// Mathematical conjunction required before any prior observation may be reused.
pub open spec fn reuse_allowed_spec(premises: ReusePremises) -> bool {
    premises.same_lineage == ReusePremiseStatus::Satisfied
        && premises.role_matches == ReusePremiseStatus::Satisfied
        && premises.sources_current == ReusePremiseStatus::Satisfied
        && premises.sequence_not_future == ReusePremiseStatus::Satisfied
        && premises.revision_current == ReusePremiseStatus::Satisfied
        && premises.dependencies_current == ReusePremiseStatus::Satisfied
}

/// Executable reuse gate used by the pure planner.
#[must_use]
pub const fn reuse_allowed(premises: ReusePremises) -> (allowed: bool)
    ensures allowed == reuse_allowed_spec(premises),
{
    matches!(premises.same_lineage, ReusePremiseStatus::Satisfied)
        && matches!(premises.role_matches, ReusePremiseStatus::Satisfied)
        && matches!(premises.sources_current, ReusePremiseStatus::Satisfied)
        && matches!(premises.sequence_not_future, ReusePremiseStatus::Satisfied)
        && matches!(premises.revision_current, ReusePremiseStatus::Satisfied)
        && matches!(premises.dependencies_current, ReusePremiseStatus::Satisfied)
}

/// Reuse exposes every currentness premise rather than trusting a summary or cache hit.
pub proof fn reuse_implies_current_observation(premises: ReusePremises)
    requires reuse_allowed_spec(premises),
    ensures
        premises.same_lineage == ReusePremiseStatus::Satisfied,
        premises.role_matches == ReusePremiseStatus::Satisfied,
        premises.sources_current == ReusePremiseStatus::Satisfied,
        premises.sequence_not_future == ReusePremiseStatus::Satisfied,
        premises.revision_current == ReusePremiseStatus::Satisfied,
        premises.dependencies_current == ReusePremiseStatus::Satisfied,
{}

/// Mathematical monotonic invalidation accumulator.
pub open spec fn invalidation_accumulates_spec(
    already_invalidated: bool,
    new_reason: bool,
) -> bool {
    already_invalidated || new_reason
}

/// Adds a reason without ever restoring an invalidated section.
#[must_use]
pub const fn invalidation_accumulates(
    already_invalidated: bool,
    new_reason: bool,
) -> (invalidated: bool)
    ensures invalidated == invalidation_accumulates_spec(already_invalidated, new_reason),
{
    already_invalidated || new_reason
}

/// Adding later invalidation causes cannot make stale knowledge reusable again.
pub proof fn invalidation_is_monotonic(already_invalidated: bool, new_reason: bool)
    ensures
        already_invalidated ==> invalidation_accumulates_spec(already_invalidated, new_reason),
{}

/// Mathematical authority gate for evidence-bearing selections.
pub open spec fn authoritative_evidence_allowed_spec(authority: KnowledgeAuthority) -> bool {
    authority == KnowledgeAuthority::Authoritative
}

/// Whether one section authority may satisfy typed evidence.
#[must_use]
pub const fn authoritative_evidence_allowed(
    authority: KnowledgeAuthority,
) -> (allowed: bool)
    ensures allowed == authoritative_evidence_allowed_spec(authority),
{
    matches!(authority, KnowledgeAuthority::Authoritative)
}

/// Navigation-only summaries can never satisfy authoritative evidence requirements.
pub proof fn navigation_cannot_satisfy_authoritative_evidence()
    ensures !authoritative_evidence_allowed_spec(KnowledgeAuthority::NavigationOnly),
{}

} // verus!
