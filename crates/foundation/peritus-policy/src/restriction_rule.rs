//! Restriction-only deny and approval rule values.

use crate::{
    digest_order::compare_digest, ApprovalRequirement, CapabilityScope, ScopeSelector,
};
use core::cmp::Ordering;
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

#[derive(Debug, Eq, PartialEq)]
enum RestrictionRuleKind {
    Deny,
    RequireApproval(ApprovalRequirement),
}

/// Canonical restriction-only rule. It cannot grant authority.
#[derive(Debug, Eq, PartialEq)]
pub struct RestrictionRule {
    digest: Sha256Digest,
    selector: ScopeSelector,
    kind: RestrictionRuleKind,
}

impl RestrictionRule {
    pub(crate) proof fn kind_is_total(&self)
        ensures self.spec_is_deny() == self.spec_approval_requirement().is_none(),
    {
        reveal(RestrictionRule::spec_is_deny);
        reveal(RestrictionRule::spec_approval_requirement);
    }

    /// Returns the exact correlation digest bytes used by amendment specifications.
    pub closed spec fn spec_digest(&self) -> [u8; 32] { self.digest.spec_bytes() }

    /// Returns the exact checked selector used by constructor specifications.
    pub closed spec fn spec_selector(&self) -> ScopeSelector { self.selector }

    /// Returns the exact canonical digest ordering used by checked rule collections.
    pub open spec fn spec_canonical_cmp(&self, other: &Self) -> Ordering {
        peritus_types::canonical_byte_order_from(self.spec_digest()@, other.spec_digest()@, 0)
    }

    /// Returns whether this rule is an exact revision-only rebind of another rule.
    pub closed spec fn spec_is_revision_rebind_of(
        &self,
        original: &Self,
        revision: peritus_types::RevisionTuple,
    ) -> bool {
        self.spec_digest() == original.spec_digest()
            && self.selector.spec_is_revision_rebind_of(&original.selector, revision)
            && self.spec_is_deny() == original.spec_is_deny()
            && match (self.spec_approval_requirement(), original.spec_approval_requirement()) {
                (Some(left), Some(right)) => left.spec_same_as(&right),
                (None, None) => true,
                _ => false,
            }
    }
    /// Returns whether this is an explicit denial in policy specifications.
    pub closed spec fn spec_is_deny(&self) -> bool {
        matches!(self.kind, RestrictionRuleKind::Deny)
    }

    /// Returns exact whole-selector matching used by policy specifications.
    pub closed spec fn spec_matches_scope(&self, scope: &CapabilityScope) -> bool {
        self.selector.spec_matches_identity(scope)
            && self.selector.spec_matches_any_permission(scope)
    }

    /// Returns the exact approval payload, if this is an approval restriction.
    pub closed spec fn spec_approval_requirement(&self) -> Option<ApprovalRequirement> {
        match self.kind {
            RestrictionRuleKind::Deny => None,
            RestrictionRuleKind::RequireApproval(requirement) => Some(requirement),
        }
    }

    /// Creates an explicit deny rule.
    #[must_use]
    pub const fn deny(digest: Sha256Digest, selector: ScopeSelector) -> Self {
        Self { digest, selector, kind: RestrictionRuleKind::Deny }
    }

    /// Creates an approval requirement rule.
    #[must_use]
    pub const fn require_approval(
        digest: Sha256Digest,
        selector: ScopeSelector,
        requirement: ApprovalRequirement,
    ) -> Self {
        Self {
            digest,
            selector,
            kind: RestrictionRuleKind::RequireApproval(requirement),
        }
    }

    /// Returns the exact canonical correlation digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest { self.digest }

    /// Returns the checked selector.
    #[must_use]
    pub const fn selector(&self) -> (selector: &ScopeSelector)
        ensures *selector == self.spec_selector(),
    { &self.selector }

    /// Returns whether this rule is an explicit denial.
    #[must_use]
    pub const fn is_deny(&self) -> (result: bool)
        ensures result == self.spec_is_deny(),
    { matches!(self.kind, RestrictionRuleKind::Deny) }

    /// Returns the approval requirement when this is an approval rule.
    #[must_use]
    pub const fn approval_requirement(&self) -> (requirement: Option<&ApprovalRequirement>)
        ensures
            match requirement {
                Some(value) => self.spec_approval_requirement() == Some(*value),
                None => self.spec_approval_requirement().is_none(),
            },
    {
        match &self.kind {
            RestrictionRuleKind::Deny => None,
            RestrictionRuleKind::RequireApproval(requirement) => Some(requirement),
        }
    }

    pub(crate) const fn canonical_cmp(&self, other: &Self) -> (result: Ordering)
        ensures result == self.spec_canonical_cmp(other),
    {
        compare_digest(&self.digest, &other.digest)
    }

    pub(crate) fn matches_scope(&self, scope: &CapabilityScope) -> (result: bool)
        ensures result == self.spec_matches_scope(scope),
    {
        self.selector.matches_identity(scope) && self.selector.matches_any_permission(scope)
    }

    pub(crate) fn matching_approval_requirement(
        &self,
        scope: &CapabilityScope,
    ) -> (result: Option<&ApprovalRequirement>)
        ensures
            match result {
                Some(requirement) => {
                    self.spec_matches_scope(scope)
                        && self.spec_approval_requirement() == Some(*requirement)
                }
                None => {
                    !self.spec_matches_scope(scope)
                        || self.spec_approval_requirement().is_none()
                }
            },
    {
        if self.matches_scope(scope) {
            self.approval_requirement()
        } else {
            None
        }
    }

    pub(crate) fn rebind_revision(
        &self,
        revision: peritus_types::RevisionTuple,
    ) -> (rebound: Self)
        ensures rebound.spec_is_revision_rebind_of(self, revision),
    {
        let kind = match &self.kind {
            RestrictionRuleKind::Deny => RestrictionRuleKind::Deny,
            RestrictionRuleKind::RequireApproval(requirement) => {
                RestrictionRuleKind::RequireApproval(requirement.duplicate())
            }
        };
        let rebound = Self {
            digest: self.digest,
            selector: self.selector.rebind_revision(revision),
            kind,
        };
        reveal(RestrictionRule::spec_is_revision_rebind_of);
        rebound
    }
}

} // verus!
