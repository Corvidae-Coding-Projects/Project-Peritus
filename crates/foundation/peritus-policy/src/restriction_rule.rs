//! Restriction-only deny and approval rule values.

use crate::{ApprovalRequirement, CapabilityScope, ScopeSelector, digest_order::compare_digest};
use core::cmp::Ordering;
use peritus_types::Sha256Digest;

#[derive(Debug, Eq, PartialEq)]
enum RestrictionRuleKind {
    Deny,
    RequireApproval(ApprovalRequirement),
}
#[doc = " Canonical restriction-only rule. It cannot grant authority."]
#[derive(Debug, Eq, PartialEq)]
pub struct RestrictionRule {
    digest: Sha256Digest,
    selector: ScopeSelector,
    kind: RestrictionRuleKind,
}
impl RestrictionRule {
    #[doc = " Creates an explicit deny rule."]
    #[must_use]
    pub const fn deny(digest: Sha256Digest, selector: ScopeSelector) -> Self {
        Self { digest, selector, kind: RestrictionRuleKind::Deny }
    }
    #[doc = " Creates an approval requirement rule."]
    #[must_use]
    pub const fn require_approval(
        digest: Sha256Digest,
        selector: ScopeSelector,
        requirement: ApprovalRequirement,
    ) -> Self {
        Self { digest, selector, kind: RestrictionRuleKind::RequireApproval(requirement) }
    }
    #[doc = " Returns the exact canonical correlation digest."]
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
    #[doc = " Returns the checked selector."]
    #[must_use]
    pub const fn selector(&self) -> &ScopeSelector {
        &self.selector
    }
    #[doc = " Returns whether this rule is an explicit denial."]
    #[must_use]
    pub const fn is_deny(&self) -> bool {
        matches!(self.kind, RestrictionRuleKind::Deny)
    }
    #[doc = " Returns the approval requirement when this is an approval rule."]
    #[must_use]
    pub const fn approval_requirement(&self) -> Option<&ApprovalRequirement> {
        match &self.kind {
            RestrictionRuleKind::Deny => None,
            RestrictionRuleKind::RequireApproval(requirement) => Some(requirement),
        }
    }
    pub(crate) const fn canonical_cmp(&self, other: &Self) -> Ordering {
        compare_digest(&self.digest, &other.digest)
    }
    pub(crate) fn matches_scope(&self, scope: &CapabilityScope) -> bool {
        self.selector.matches_identity(scope) && self.selector.matches_any_permission(scope)
    }
    pub(crate) fn matching_approval_requirement(
        &self,
        scope: &CapabilityScope,
    ) -> Option<&ApprovalRequirement> {
        if self.matches_scope(scope) { self.approval_requirement() } else { None }
    }
    pub(crate) fn rebind_revision(&self, revision: peritus_types::RevisionTuple) -> Self {
        let kind = match &self.kind {
            RestrictionRuleKind::Deny => RestrictionRuleKind::Deny,
            RestrictionRuleKind::RequireApproval(requirement) => {
                RestrictionRuleKind::RequireApproval(requirement.duplicate())
            }
        };

        Self { digest: self.digest, selector: self.selector.rebind_revision(revision), kind }
    }
}
