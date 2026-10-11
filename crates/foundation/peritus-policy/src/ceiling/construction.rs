//! Exact ordered validation for protected authority ceilings.

use super::{AuthorityCeiling, CeilingGrant};
use crate::{
    AuthorityBoundary, CanonicalCollection, PolicyError, RestrictionRule,
    digest_order::compare_digest,
};

use core::cmp::Ordering;

impl AuthorityCeiling {
    #[doc = " Returns the complete parent authority boundary."]
    #[must_use]
    pub const fn boundary(&self) -> &AuthorityBoundary {
        &self.boundary
    }
    #[doc = " Validates canonical grants and immutable deny-only rules beneath one boundary."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns the exact first canonical, containment, or rule-kind failure."]
    pub fn new(
        boundary: AuthorityBoundary,
        grants: Vec<CeilingGrant>,
        immutable_denies: Vec<RestrictionRule>,
    ) -> Result<Self, PolicyError> {
        let mut index = 0;
        while index < grants.len() {
            if let Some(dimension) = grants[index].selector.first_boundary_mismatch(&boundary) {
                return Err(PolicyError::selector_outside_boundary(dimension));
            }
            if index > 0 {
                match compare_digest(&grants[index - 1].digest, &grants[index].digest) {
                    Ordering::Less => {}
                    Ordering::Equal => {
                        return Err(PolicyError::duplicate_canonical_value(
                            CanonicalCollection::Grants,
                        ));
                    }
                    Ordering::Greater => {
                        return Err(PolicyError::non_canonical_order(CanonicalCollection::Grants));
                    }
                }
            }
            index += 1;
        }
        index = 0;
        while index < immutable_denies.len() {
            if !immutable_denies[index].is_deny() {
                return Err(PolicyError::invalid_rule_kind());
            }
            if let Some(dimension) =
                immutable_denies[index].selector().first_boundary_mismatch(&boundary)
            {
                return Err(PolicyError::selector_outside_boundary(dimension));
            }
            if index > 0 {
                match immutable_denies[index - 1].canonical_cmp(&immutable_denies[index]) {
                    Ordering::Less => {}
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
        Ok(ceiling)
    }
}
