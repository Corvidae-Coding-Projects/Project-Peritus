//! Complete finite authority boundaries, default-deny grants, and immutable denials.

use crate::{
    AuthorityBoundary, CapabilityScope, RestrictionRule, ScopeSelector, UseLimit, ValidityWindow,
};
use peritus_types::{RevisionTuple, Sha256Digest};
use vstd::prelude::*;

mod construction;

verus! {

/// One checked grant beneath an authority boundary.
#[derive(Debug, Eq, PartialEq)]
pub struct CeilingGrant {
    digest: Sha256Digest,
    selector: ScopeSelector,
    validity: ValidityWindow,
    use_limit: UseLimit,
}

impl CeilingGrant {
    /// Returns the exact correlation digest bytes used by constructor specifications.
    pub closed spec fn spec_digest(&self) -> [u8; 32] { self.digest.spec_bytes() }

    /// Returns the exact checked selector value used by constructor specifications.
    pub closed spec fn spec_selector(&self) -> ScopeSelector { self.selector }

    /// Returns whether this grant preserves every constraint under a revision-only rebind.
    pub closed spec fn spec_is_revision_rebind_of(
        &self,
        original: &Self,
        revision: RevisionTuple,
    ) -> bool {
        self.digest.spec_bytes() == original.digest.spec_bytes()
            && self.selector.spec_is_revision_rebind_of(&original.selector, revision)
            && self.validity == original.validity
            && self.use_limit == original.use_limit
    }

    /// Returns exact whole-request selector matching used by constraint specifications.
    pub closed spec fn spec_matches_scope(&self, scope: &CapabilityScope) -> bool {
        self.selector.spec_matches_identity(scope)
            && self.selector.spec_matches_any_permission(scope)
    }

    /// Returns the exact finite grant validity used by evaluation specifications.
    pub closed spec fn spec_validity(&self) -> ValidityWindow { self.validity }

    /// Returns the exact finite or unlimited grant use bound used by specifications.
    pub closed spec fn spec_use_limit(&self) -> UseLimit { self.use_limit }

    /// Returns exact identity matching used by default-deny coverage specifications.
    pub closed spec fn spec_matches_identity(&self, scope: &CapabilityScope) -> bool {
        self.selector.spec_matches_identity(scope)
    }

    /// Returns exact permission membership used by default-deny coverage specifications.
    pub closed spec fn spec_contains_permission(&self, permission: &crate::Permission) -> bool {
        self.selector.spec_contains_permission(permission)
    }

    pub(crate) fn matches_identity(&self, scope: &CapabilityScope) -> (result: bool)
        ensures result == self.spec_matches_identity(scope),
    {
        self.selector.matches_identity(scope)
    }

    pub(crate) fn contains_permission(&self, permission: &crate::Permission) -> (result: bool)
        ensures result == self.spec_contains_permission(permission),
    {
        self.selector.contains_permission(permission)
    }

    pub(crate) fn matches_scope(&self, scope: &CapabilityScope) -> (result: bool)
        ensures result == self.spec_matches_scope(scope),
    {
        self.selector.matches_identity(scope) && self.selector.matches_any_permission(scope)
    }

    /// Creates a grant whose parent containment is checked by [`AuthorityCeiling::new`].
    #[must_use]
    pub const fn new(
        digest: Sha256Digest,
        selector: ScopeSelector,
        validity: ValidityWindow,
        use_limit: UseLimit,
    ) -> (grant: Self)
        ensures
            grant.spec_digest() == digest.spec_bytes(),
            grant.spec_selector() == selector,
            grant.spec_validity() == validity,
            grant.spec_use_limit() == use_limit,
    {
        Self { digest, selector, validity, use_limit }
    }

    /// Returns the exact correlation digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest { self.digest }

    /// Returns the checked selector.
    #[must_use]
    pub const fn selector(&self) -> &ScopeSelector { &self.selector }

    /// Returns the finite validity constraint.
    #[must_use]
    pub const fn validity(&self) -> (validity: ValidityWindow)
        ensures validity == self.spec_validity(),
    { self.validity }

    /// Returns the finite or unlimited use constraint.
    #[must_use]
    pub const fn use_limit(&self) -> (use_limit: UseLimit)
        ensures use_limit == self.spec_use_limit(),
    { self.use_limit }

    fn rebind_revision(&self, revision: RevisionTuple) -> (rebound: Self)
        ensures rebound.spec_is_revision_rebind_of(self, revision),
    {
        let rebound = Self {
            digest: self.digest,
            selector: self.selector.rebind_revision(revision),
            validity: self.validity,
            use_limit: self.use_limit,
        };
        reveal(CeilingGrant::spec_is_revision_rebind_of);
        rebound
    }
}

/// Protected upper bound, canonical grants, and immutable denials.
#[derive(Debug, Eq, PartialEq)]
pub struct AuthorityCeiling {
    boundary: AuthorityBoundary,
    grants: Vec<CeilingGrant>,
    immutable_denies: Vec<RestrictionRule>,
}

impl AuthorityCeiling {
    /// Returns whether this ceiling preserves its full protected authority under a revision rebind.
    pub closed spec fn spec_is_revision_rebind_of(
        &self,
        original: &Self,
        revision: RevisionTuple,
    ) -> bool {
        self.boundary.spec_is_revision_rebind_of(&original.boundary, revision)
            && self.spec_grants().len() == original.spec_grants().len()
            && forall |index: int| 0 <= index < self.spec_grants().len() ==>
                #[trigger] self.spec_grants()[index].spec_is_revision_rebind_of(
                    &original.spec_grants()[index],
                    revision,
                )
            && self.spec_immutable_denies().len()
                == original.spec_immutable_denies().len()
            && forall |index: int| 0 <= index < self.spec_immutable_denies().len() ==>
                #[trigger] self.spec_immutable_denies()[index].spec_is_revision_rebind_of(
                    &original.spec_immutable_denies()[index],
                    revision,
                )
    }

    pub(crate) proof fn revision_rebind_has_exact_revision(
        &self,
        original: &Self,
        revision: RevisionTuple,
    )
        requires self.spec_is_revision_rebind_of(original, revision),
        ensures self.spec_boundary_revision() == revision,
    {
        reveal(AuthorityCeiling::spec_is_revision_rebind_of);
        reveal(AuthorityCeiling::spec_boundary_revision);
        self.boundary.revision_rebind_has_exact_revision(&original.boundary, revision);
    }
    /// Returns the exact parent validity bound.
    pub closed spec fn spec_boundary_validity(&self) -> ValidityWindow {
        self.boundary.spec_validity()
    }

    /// Returns the exact protected boundary revision.
    pub closed spec fn spec_boundary_revision(&self) -> RevisionTuple {
        self.boundary.spec_revision()
    }

    /// Returns the exact parent logical-use bound.
    pub closed spec fn spec_boundary_use_limit(&self) -> UseLimit {
        self.boundary.spec_use_limit()
    }

    /// Returns exact complete parent-boundary containment.
    pub closed spec fn spec_contains_scope(&self, scope: &CapabilityScope) -> bool {
        self.boundary.spec_contains_scope(scope)
    }

    /// Returns the canonical grant sequence used by evaluation specifications.
    pub closed spec fn spec_grants(&self) -> Seq<CeilingGrant> { self.grants@ }

    /// Returns the immutable deny sequence used by evaluation specifications.
    pub closed spec fn spec_immutable_denies(&self) -> Seq<RestrictionRule> {
        self.immutable_denies@
    }

    /// Returns whether an immutable ceiling denial matches the request.
    pub open spec fn spec_has_immutable_deny(&self, scope: &CapabilityScope) -> bool {
        crate::model::deny_rule_matches_from(self.spec_immutable_denies(), scope, 0)
    }

    /// Returns exact whole-request default-deny grant coverage.
    pub open spec fn spec_has_full_coverage(&self, scope: &CapabilityScope) -> bool {
        crate::model::full_ceiling_coverage_from(
            scope.spec_permissions(),
            self.spec_grants(),
            scope,
            0,
        )
    }

    pub(crate) fn contains_scope(&self, scope: &CapabilityScope) -> (result: bool)
        ensures result == self.spec_contains_scope(scope),
    {
        self.boundary.contains_scope(scope)
    }
    /// Borrows canonical ceiling grants.
    #[must_use]
    pub const fn grants(&self) -> (grants: &[CeilingGrant])
        ensures grants@ == self.spec_grants(),
    { self.grants.as_slice() }

    /// Borrows canonical immutable deny rules.
    #[must_use]
    pub const fn immutable_denies(&self) -> (denies: &[RestrictionRule])
        ensures denies@ == self.spec_immutable_denies(),
    {
        self.immutable_denies.as_slice()
    }

    pub(crate) fn rebind_revision(&self, revision: RevisionTuple) -> (rebound: Self)
        ensures rebound.spec_is_revision_rebind_of(self, revision),
    {
        let mut grants: Vec<CeilingGrant> = Vec::new();
        let mut index = 0;
        while index < self.grants.len()
            invariant
                0 <= index <= self.grants.len(),
                grants@.len() == index,
                forall |prior: int| 0 <= prior < index ==>
                    #[trigger] grants@[prior].spec_is_revision_rebind_of(
                        &self.grants@[prior],
                        revision,
                    ),
            decreases self.grants.len() - index,
        {
            grants.push(self.grants[index].rebind_revision(revision));
            index += 1;
        }
        let mut immutable_denies: Vec<RestrictionRule> = Vec::new();
        index = 0;
        while index < self.immutable_denies.len()
            invariant
                0 <= index <= self.immutable_denies.len(),
                immutable_denies@.len() == index,
                forall |prior: int| 0 <= prior < index ==>
                    #[trigger] immutable_denies@[prior].spec_is_revision_rebind_of(
                        &self.immutable_denies@[prior],
                        revision,
                    ),
            decreases self.immutable_denies.len() - index,
        {
            immutable_denies.push(self.immutable_denies[index].rebind_revision(revision));
            index += 1;
        }
        let rebound = Self {
            boundary: self.boundary.rebind_revision(revision),
            grants,
            immutable_denies,
        };
        reveal(AuthorityCeiling::spec_is_revision_rebind_of);
        rebound
    }
}

} // verus!
