//! Projection of immutable B2 reviewer-independence requirements.

use peritus_spec::ReviewerIndependence;
use vstd::prelude::*;

verus! {

/// Exact review-independence facts requested from the future D2 review engine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(clippy::struct_excessive_bools, reason = "each named fact is immutable contract data")]
pub struct ReviewIndependenceView {
    distinct_reviewers: bool,
    independent_from_producer: bool,
    distinct_contexts: bool,
    distinct_model_families: bool,
    distinct_providers: bool,
    no_shared_ancestry: bool,
    fresh_context: bool,
}

impl ReviewIndependenceView {
    /// Copies every B2 requirement and adds C6's mandatory fresh-context rule.
    #[must_use]
    pub const fn from_contract(requirements: ReviewerIndependence) -> Self {
        Self {
            distinct_reviewers: requirements.requires_distinct_reviewers(),
            independent_from_producer: requirements.requires_independence_from_producer(),
            distinct_contexts: requirements.requires_distinct_contexts(),
            distinct_model_families: requirements.requires_distinct_model_families(),
            distinct_providers: requirements.requires_distinct_providers(),
            no_shared_ancestry: requirements.requires_no_shared_ancestry(),
            fresh_context: true,
        }
    }

    /// Whether identities must be distinct.
    #[must_use]
    pub const fn distinct_reviewers(&self) -> bool { self.distinct_reviewers }
    /// Whether the producer is excluded.
    #[must_use]
    pub const fn independent_from_producer(&self) -> bool { self.independent_from_producer }
    /// Whether contexts must be distinct.
    #[must_use]
    pub const fn distinct_contexts(&self) -> bool { self.distinct_contexts }
    /// Whether model families must be distinct.
    #[must_use]
    pub const fn distinct_model_families(&self) -> bool { self.distinct_model_families }
    /// Whether providers must be distinct.
    #[must_use]
    pub const fn distinct_providers(&self) -> bool { self.distinct_providers }
    /// Whether shared ancestry is forbidden.
    #[must_use]
    pub const fn no_shared_ancestry(&self) -> bool { self.no_shared_ancestry }
    /// Whether every reviewer starts from a fresh model context.
    #[must_use]
    pub const fn fresh_context(&self) -> bool { self.fresh_context }
}

} // verus!
