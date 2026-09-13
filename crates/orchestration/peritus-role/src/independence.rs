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
    /// Exact stored distinct reviewers.
    pub closed spec fn spec_distinct_reviewers(&self) -> bool { self.distinct_reviewers }
    /// Exact stored independent from producer.
    pub closed spec fn spec_independent_from_producer(&self) -> bool { self.independent_from_producer }
    /// Exact stored distinct contexts.
    pub closed spec fn spec_distinct_contexts(&self) -> bool { self.distinct_contexts }
    /// Exact stored distinct model families.
    pub closed spec fn spec_distinct_model_families(&self) -> bool { self.distinct_model_families }
    /// Exact stored distinct providers.
    pub closed spec fn spec_distinct_providers(&self) -> bool { self.distinct_providers }
    /// Exact stored no shared ancestry.
    pub closed spec fn spec_no_shared_ancestry(&self) -> bool { self.no_shared_ancestry }
    /// Exact stored fresh context.
    pub closed spec fn spec_fresh_context(&self) -> bool { self.fresh_context }

    /// Copies every B2 requirement and adds C6's mandatory fresh-context rule.
    #[must_use]
    pub const fn from_contract(requirements: ReviewerIndependence) -> (result: Self)
        ensures
            result.spec_distinct_reviewers() == requirements.spec_distinct_reviewers(),
            result.spec_independent_from_producer() == requirements.spec_independent_from_producer(),
            result.spec_distinct_contexts() == requirements.spec_distinct_contexts(),
            result.spec_distinct_model_families() == requirements.spec_distinct_model_families(),
            result.spec_distinct_providers() == requirements.spec_distinct_providers(),
            result.spec_no_shared_ancestry() == requirements.spec_no_shared_ancestry(),
            result.spec_fresh_context(),
    {
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
    pub const fn distinct_reviewers(&self) -> (value: bool)
        ensures value == self.spec_distinct_reviewers(),
    { self.distinct_reviewers }
    /// Whether the producer is excluded.
    #[must_use]
    pub const fn independent_from_producer(&self) -> (value: bool)
        ensures value == self.spec_independent_from_producer(),
    { self.independent_from_producer }
    /// Whether contexts must be distinct.
    #[must_use]
    pub const fn distinct_contexts(&self) -> (value: bool)
        ensures value == self.spec_distinct_contexts(),
    { self.distinct_contexts }
    /// Whether model families must be distinct.
    #[must_use]
    pub const fn distinct_model_families(&self) -> (value: bool)
        ensures value == self.spec_distinct_model_families(),
    { self.distinct_model_families }
    /// Whether providers must be distinct.
    #[must_use]
    pub const fn distinct_providers(&self) -> (value: bool)
        ensures value == self.spec_distinct_providers(),
    { self.distinct_providers }
    /// Whether shared ancestry is forbidden.
    #[must_use]
    pub const fn no_shared_ancestry(&self) -> (value: bool)
        ensures value == self.spec_no_shared_ancestry(),
    { self.no_shared_ancestry }
    /// Whether every reviewer starts from a fresh model context.
    #[must_use]
    pub const fn fresh_context(&self) -> (value: bool)
        ensures value == self.spec_fresh_context(),
    { self.fresh_context }
}

} // verus!
