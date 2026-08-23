//! Exact ordered validation for protected authority ceilings.

use super::{AuthorityCeiling, CeilingGrant};
use crate::{
    digest_order::compare_digest, AuthorityBoundary, CanonicalCollection, PolicyError,
    RestrictionRule,
};
#[cfg(verus_only)]
use crate::{PolicyErrorKind, ScopeDimension};
use core::cmp::Ordering;
use vstd::prelude::*;

verus! {

/// Exact typed failure triple used by ceiling-construction specifications.
#[cfg(verus_only)]
pub type CeilingValidationError = (
    PolicyErrorKind,
    Option<CanonicalCollection>,
    Option<ScopeDimension>,
);

/// Returns the exact first grant-containment or canonical-order failure.
pub closed spec fn first_grant_validation_error(
    grants: Seq<CeilingGrant>,
    boundary: &AuthorityBoundary,
    index: nat,
) -> Option<CeilingValidationError>
    decreases grants.len() - index,
{
    if index >= grants.len() {
        None
    } else if grants[index as int].selector.spec_first_boundary_mismatch(boundary) is Some {
        Some((
            PolicyErrorKind::SelectorOutsideBoundary,
            None,
            grants[index as int].selector.spec_first_boundary_mismatch(boundary),
        ))
    } else if index > 0 {
        match peritus_types::canonical_byte_order_from(
            grants[index as int - 1].spec_digest()@,
            grants[index as int].spec_digest()@,
            0,
        ) {
            Ordering::Equal => Some((
                PolicyErrorKind::DuplicateCanonicalValue,
                Some(CanonicalCollection::Grants),
                None,
            )),
            Ordering::Greater => Some((
                PolicyErrorKind::NonCanonicalOrder,
                Some(CanonicalCollection::Grants),
                None,
            )),
            Ordering::Less => first_grant_validation_error(grants, boundary, index + 1),
        }
    } else {
        first_grant_validation_error(grants, boundary, index + 1)
    }
}

/// Returns the exact first immutable-denial kind, containment, or order failure.
pub closed spec fn first_deny_validation_error(
    denies: Seq<RestrictionRule>,
    boundary: &AuthorityBoundary,
    index: nat,
) -> Option<CeilingValidationError>
    decreases denies.len() - index,
{
    if index >= denies.len() {
        None
    } else if !denies[index as int].spec_is_deny() {
        Some((PolicyErrorKind::InvalidRuleKind, None, None))
    } else if denies[index as int].spec_selector().spec_first_boundary_mismatch(boundary) is Some {
        Some((
            PolicyErrorKind::SelectorOutsideBoundary,
            None,
            denies[index as int].spec_selector().spec_first_boundary_mismatch(boundary),
        ))
    } else if index > 0 {
        match denies[index as int - 1].spec_canonical_cmp(&denies[index as int]) {
            Ordering::Equal => Some((
                PolicyErrorKind::DuplicateCanonicalValue,
                Some(CanonicalCollection::RestrictionRules),
                None,
            )),
            Ordering::Greater => Some((
                PolicyErrorKind::NonCanonicalOrder,
                Some(CanonicalCollection::RestrictionRules),
                None,
            )),
            Ordering::Less => first_deny_validation_error(denies, boundary, index + 1),
        }
    } else {
        first_deny_validation_error(denies, boundary, index + 1)
    }
}

/// Returns the exact first failure in executable ceiling-validation order.
pub closed spec fn authority_ceiling_validation_error(
    boundary: &AuthorityBoundary,
    grants: Seq<CeilingGrant>,
    denies: Seq<RestrictionRule>,
) -> Option<CeilingValidationError> {
    if first_grant_validation_error(grants, boundary, 0) is Some {
        first_grant_validation_error(grants, boundary, 0)
    } else {
        first_deny_validation_error(denies, boundary, 0)
    }
}

impl AuthorityCeiling {
    /// Returns the exact boundary value used by constructor specifications.
    pub closed spec fn spec_boundary_value(&self) -> AuthorityBoundary { self.boundary }

    /// Returns the complete parent authority boundary.
    #[must_use]
    pub const fn boundary(&self) -> (boundary: &AuthorityBoundary)
        ensures
            *boundary == self.spec_boundary_value(),
            boundary.spec_revision() == self.spec_boundary_revision(),
            boundary.spec_validity() == self.spec_boundary_validity(),
            boundary.spec_use_limit() == self.spec_boundary_use_limit(),
    { &self.boundary }

    /// Validates canonical grants and immutable deny-only rules beneath one boundary.
    ///
    /// # Errors
    ///
    /// Returns the exact first canonical, containment, or rule-kind failure.
    pub fn new(
        boundary: AuthorityBoundary,
        grants: Vec<CeilingGrant>,
        immutable_denies: Vec<RestrictionRule>,
    ) -> (result: Result<Self, PolicyError>)
        ensures
            match result {
                Ok(ceiling) => {
                    authority_ceiling_validation_error(
                        &boundary,
                        grants@,
                        immutable_denies@,
                    ).is_none()
                        && ceiling.spec_boundary_value() == boundary
                        && ceiling.spec_grants() == grants@
                        && ceiling.spec_immutable_denies() == immutable_denies@
                }
                Err(error) => {
                    authority_ceiling_validation_error(
                        &boundary,
                        grants@,
                        immutable_denies@,
                    ) == Some((
                        error.spec_kind(),
                        error.spec_collection(),
                        error.spec_dimension(),
                    ))
                }
            },
    {
        let mut index = 0;
        while index < grants.len()
            invariant
                0 <= index <= grants.len(),
                first_grant_validation_error(grants@, &boundary, 0)
                    == first_grant_validation_error(grants@, &boundary, index as nat),
            decreases grants.len() - index,
        {
            if let Some(dimension) = grants[index].selector.first_boundary_mismatch(&boundary) {
                return Err(PolicyError::selector_outside_boundary(dimension));
            }
            if index > 0 {
                match compare_digest(&grants[index - 1].digest, &grants[index].digest) {
                    Ordering::Less => {},
                    Ordering::Equal => {
                        return Err(PolicyError::duplicate_canonical_value(CanonicalCollection::Grants));
                    }
                    Ordering::Greater => {
                        return Err(PolicyError::non_canonical_order(CanonicalCollection::Grants));
                    }
                }
            }
            index += 1;
        }
        index = 0;
        while index < immutable_denies.len()
            invariant
                0 <= index <= immutable_denies.len(),
                first_grant_validation_error(grants@, &boundary, 0).is_none(),
                first_deny_validation_error(immutable_denies@, &boundary, 0)
                    == first_deny_validation_error(immutable_denies@, &boundary, index as nat),
            decreases immutable_denies.len() - index,
        {
            if !immutable_denies[index].is_deny() {
                return Err(PolicyError::invalid_rule_kind());
            }
            if let Some(dimension) = immutable_denies[index]
                .selector()
                .first_boundary_mismatch(&boundary)
            {
                return Err(PolicyError::selector_outside_boundary(dimension));
            }
            if index > 0 {
                match immutable_denies[index - 1].canonical_cmp(&immutable_denies[index]) {
                    Ordering::Less => {},
                    Ordering::Equal => {
                        return Err(PolicyError::duplicate_canonical_value(
                            CanonicalCollection::RestrictionRules,
                        ));
                    }
                    Ordering::Greater => {
                        return Err(PolicyError::non_canonical_order(
                            CanonicalCollection::RestrictionRules,
                        ));
                    }
                }
            }
            index += 1;
        }
        let ceiling = Self { boundary, grants, immutable_denies };
        reveal(AuthorityCeiling::spec_boundary_value);
        reveal(AuthorityCeiling::spec_grants);
        reveal(AuthorityCeiling::spec_immutable_denies);
        Ok(ceiling)
    }
}

} // verus!
