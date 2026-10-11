//! Reviewer quorum, category, severity, and independence policy.

use crate::{CanonicalCollection, LimitKind, ReviewCategory, SpecError};

#[doc = " Ordered finding severity used by contract blocker thresholds."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum FindingSeverity {
    #[doc = " Informational or stylistic advice."]
    Advisory,
    #[doc = " A low-impact correctness or maintainability concern."]
    Low,
    #[doc = " A material concern that should normally be fixed."]
    Medium,
    #[doc = " A serious correctness or production-readiness concern."]
    High,
    #[doc = " A release-stopping critical concern."]
    Critical,
}
#[doc = " Independent facts required of the review quorum."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[allow(clippy::struct_excessive_bools, reason = "each named independence fact is contract data")]
pub struct ReviewerIndependence {
    distinct_reviewers: bool,
    independent_from_producer: bool,
    distinct_contexts: bool,
    distinct_model_families: bool,
    distinct_providers: bool,
    no_shared_ancestry: bool,
}
impl ReviewerIndependence {
    #[doc = " Creates an explicit conjunction of independence requirements."]
    #[must_use]
    #[allow(
        clippy::fn_params_excessive_bools,
        reason = "callers must declare every independence fact"
    )]
    pub const fn new(
        distinct_reviewers: bool,
        independent_from_producer: bool,
        distinct_contexts: bool,
        distinct_model_families: bool,
        distinct_providers: bool,
        no_shared_ancestry: bool,
    ) -> Self {
        Self {
            distinct_reviewers,
            independent_from_producer,
            distinct_contexts,
            distinct_model_families,
            distinct_providers,
            no_shared_ancestry,
        }
    }
    #[doc = " Whether one identity may count more than once in the quorum."]
    #[must_use]
    pub const fn requires_distinct_reviewers(&self) -> bool {
        self.distinct_reviewers
    }
    #[doc = " Whether producing actors are excluded from the review quorum."]
    #[must_use]
    pub const fn requires_independence_from_producer(&self) -> bool {
        self.independent_from_producer
    }
    #[doc = " Whether each counted review must use a distinct context."]
    #[must_use]
    pub const fn requires_distinct_contexts(&self) -> bool {
        self.distinct_contexts
    }
    #[doc = " Whether each counted review must use a distinct model family."]
    #[must_use]
    pub const fn requires_distinct_model_families(&self) -> bool {
        self.distinct_model_families
    }
    #[doc = " Whether each counted review must use a distinct provider."]
    #[must_use]
    pub const fn requires_distinct_providers(&self) -> bool {
        self.distinct_providers
    }
    #[doc = " Whether reviews with shared causal ancestry are excluded."]
    #[must_use]
    pub const fn requires_no_shared_ancestry(&self) -> bool {
        self.no_shared_ancestry
    }
}
#[doc = " Checked reviewer policy attached to an acceptance contract."]
#[derive(Debug, Eq, PartialEq)]
pub struct ReviewPolicy {
    required_categories: Vec<ReviewCategory>,
    reviewer_quorum: u16,
    independence: ReviewerIndependence,
    blocking_severity: FindingSeverity,
}
impl ReviewPolicy {
    #[doc = " Validates a nonempty, strictly ordered category set and nonzero reviewer quorum."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a typed empty, duplicate, ordering, or zero-quorum error."]
    pub fn new(
        required_categories: Vec<ReviewCategory>,
        reviewer_quorum: u16,
        independence: ReviewerIndependence,
        blocking_severity: FindingSeverity,
    ) -> Result<Self, SpecError> {
        if required_categories.is_empty() {
            return Err(SpecError::EmptyCollection(CanonicalCollection::ReviewCategories));
        }
        let mut index = 0;
        while index < required_categories.len() {
            if index > 0 {
                if required_categories[index - 1] == required_categories[index] {
                    return Err(SpecError::DuplicateCanonicalValue(
                        CanonicalCollection::ReviewCategories,
                    ));
                }
                if required_categories[index - 1] > required_categories[index] {
                    return Err(SpecError::NonCanonicalOrder(
                        CanonicalCollection::ReviewCategories,
                    ));
                }
            }
            index += 1;
        }
        if reviewer_quorum == 0 {
            return Err(SpecError::ZeroLimit(LimitKind::ReviewerQuorum));
        }
        Ok(Self { required_categories, reviewer_quorum, independence, blocking_severity })
    }
    #[doc = " Returns required categories in canonical order."]
    #[must_use]
    pub const fn required_categories(&self) -> &[ReviewCategory] {
        self.required_categories.as_slice()
    }
    #[doc = " Returns the number of reviews required to form a quorum."]
    #[must_use]
    pub const fn reviewer_quorum(&self) -> u16 {
        self.reviewer_quorum
    }
    #[doc = " Returns the required reviewer independence facts."]
    #[must_use]
    pub const fn independence(&self) -> ReviewerIndependence {
        self.independence
    }
    #[doc = " Returns the lowest finding severity treated as a blocker."]
    #[must_use]
    pub const fn blocking_severity(&self) -> FindingSeverity {
        self.blocking_severity
    }
}
