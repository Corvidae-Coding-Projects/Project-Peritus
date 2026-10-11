//! Executable refinement predicates for reuse and authority invariants.

use crate::KnowledgeAuthority;

#[doc = " Result of checking one independent reuse premise."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReusePremiseStatus {
    #[doc = " The premise is current and satisfied."]
    Satisfied,
    #[doc = " The premise is missing, stale, or false."]
    Failed,
}
impl ReusePremiseStatus {
    #[doc = " Converts one checked predicate into a typed premise status."]
    #[must_use]
    pub const fn from_satisfied(satisfied: bool) -> Self {
        if satisfied { Self::Satisfied } else { Self::Failed }
    }
}
#[doc = " Independent currentness premises required before prior knowledge may be reused."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReusePremises {
    #[doc = " Run and workspace identities are unchanged."]
    pub same_lineage: ReusePremiseStatus,
    #[doc = " The retained view belongs to the target role."]
    pub role_matches: ReusePremiseStatus,
    #[doc = " Every bound source digest is current."]
    pub sources_current: ReusePremiseStatus,
    #[doc = " The retained observation was not created in the future."]
    pub sequence_not_future: ReusePremiseStatus,
    #[doc = " Conversation and candidate revisions satisfy the section kind."]
    pub revision_current: ReusePremiseStatus,
    #[doc = " Every section dependency is still current."]
    pub dependencies_current: ReusePremiseStatus,
}
impl ReusePremises {
    #[doc = " Collects the six independent fail-closed reuse premises."]
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
#[doc = " Executable reuse gate used by the pure planner."]
#[must_use]
pub const fn reuse_allowed(premises: ReusePremises) -> bool {
    matches!(premises.same_lineage, ReusePremiseStatus::Satisfied)
        && matches!(premises.role_matches, ReusePremiseStatus::Satisfied)
        && matches!(premises.sources_current, ReusePremiseStatus::Satisfied)
        && matches!(premises.sequence_not_future, ReusePremiseStatus::Satisfied)
        && matches!(premises.revision_current, ReusePremiseStatus::Satisfied)
        && matches!(premises.dependencies_current, ReusePremiseStatus::Satisfied)
}
#[doc = " Adds a reason without ever restoring an invalidated section."]
#[must_use]
pub const fn invalidation_accumulates(already_invalidated: bool, new_reason: bool) -> bool {
    already_invalidated || new_reason
}
#[doc = " Whether one section authority may satisfy typed evidence."]
#[must_use]
pub const fn authoritative_evidence_allowed(authority: KnowledgeAuthority) -> bool {
    matches!(authority, KnowledgeAuthority::Authoritative)
}
