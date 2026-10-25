//! Complete finite authority boundaries, default-deny grants, and immutable denials.

use crate::{
    AuthorityBoundary, CapabilityScope, RestrictionRule, ScopeSelector, UseLimit, ValidityWindow,
};
use peritus_types::{RevisionTuple, Sha256Digest};

mod construction;

#[doc = " One checked grant beneath an authority boundary."]
#[derive(Debug, Eq, PartialEq)]
pub struct CeilingGrant {
    digest: Sha256Digest,
    selector: ScopeSelector,
    validity: ValidityWindow,
    use_limit: UseLimit,
}
impl CeilingGrant {
    pub(crate) fn matches_identity(&self, scope: &CapabilityScope) -> bool {
        self.selector.matches_identity(scope)
    }
    pub(crate) fn contains_permission(&self, permission: &crate::Permission) -> bool {
        self.selector.contains_permission(permission)
    }
    pub(crate) fn matches_scope(&self, scope: &CapabilityScope) -> bool {
        self.selector.matches_identity(scope) && self.selector.matches_any_permission(scope)
    }
    #[doc = " Creates a grant whose parent containment is checked by [`AuthorityCeiling::new`]."]
    #[must_use]
    pub const fn new(
        digest: Sha256Digest,
        selector: ScopeSelector,
        validity: ValidityWindow,
        use_limit: UseLimit,
    ) -> Self {
        Self { digest, selector, validity, use_limit }
    }
    #[doc = " Returns the exact correlation digest."]
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
    #[doc = " Returns the checked selector."]
    #[must_use]
    pub const fn selector(&self) -> &ScopeSelector {
        &self.selector
    }
    #[doc = " Returns the finite validity constraint."]
    #[must_use]
    pub const fn validity(&self) -> ValidityWindow {
        self.validity
    }
    #[doc = " Returns the finite or unlimited use constraint."]
    #[must_use]
    pub const fn use_limit(&self) -> UseLimit {
        self.use_limit
    }
    fn rebind_revision(&self, revision: RevisionTuple) -> Self {
        Self {
            digest: self.digest,
            selector: self.selector.rebind_revision(revision),
            validity: self.validity,
            use_limit: self.use_limit,
        }
    }
}
#[doc = " Protected upper bound, canonical grants, and immutable denials."]
#[derive(Debug, Eq, PartialEq)]
pub struct AuthorityCeiling {
    boundary: AuthorityBoundary,
    grants: Vec<CeilingGrant>,
    immutable_denies: Vec<RestrictionRule>,
}
impl AuthorityCeiling {
    pub(crate) fn contains_scope(&self, scope: &CapabilityScope) -> bool {
        self.boundary.contains_scope(scope)
    }
    #[doc = " Borrows canonical ceiling grants."]
    #[must_use]
    pub const fn grants(&self) -> &[CeilingGrant] {
        self.grants.as_slice()
    }
    #[doc = " Borrows canonical immutable deny rules."]
    #[must_use]
    pub const fn immutable_denies(&self) -> &[RestrictionRule] {
        self.immutable_denies.as_slice()
    }
    pub(crate) fn rebind_revision(&self, revision: RevisionTuple) -> Self {
        let mut grants: Vec<CeilingGrant> = Vec::new();
        let mut index = 0;
        while index < self.grants.len() {
            grants.push(self.grants[index].rebind_revision(revision));
            index += 1;
        }
        let mut immutable_denies: Vec<RestrictionRule> = Vec::new();
        index = 0;
        while index < self.immutable_denies.len() {
            immutable_denies.push(self.immutable_denies[index].rebind_revision(revision));
            index += 1;
        }

        Self { boundary: self.boundary.rebind_revision(revision), grants, immutable_denies }
    }
}
