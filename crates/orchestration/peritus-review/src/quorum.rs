//! Independent named review-quorum dimensions.

use std::collections::BTreeSet;

use peritus_spec::ReviewCategory;

use crate::{ReviewBinding, ReviewCycle, ReviewCyclePhase};

/// Every independently evaluated quorum condition.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum QuorumDimension {
    /// Sufficient complete current submissions exist.
    SubmittedReviewCount,
    /// Every contract-required category is covered.
    RequiredCategoryCoverage,
    /// Counted actor identities are distinct when required.
    DistinctReviewerIdentities,
    /// Every counted reviewer is producer-independent when required.
    ProducerIndependence,
    /// Counted context identities are distinct when required.
    DistinctContexts,
    /// Counted model families are distinct when required.
    DistinctModelFamilies,
    /// Counted providers are distinct when required.
    DistinctProviders,
    /// Counted reviewers share no ancestry when required.
    NoSharedAncestry,
    /// Every counted assignment carries a valid fresh-context fact.
    FreshContext,
}

/// Complete report preserving every quorum condition independently.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "D2 exposes each named quorum dimension independently for auditability"
)]
pub struct QuorumReport {
    submitted_reviews: u16,
    covered_categories: Vec<ReviewCategory>,
    submitted_review_count: bool,
    required_category_coverage: bool,
    distinct_reviewer_identities: bool,
    producer_independence: bool,
    distinct_contexts: bool,
    distinct_model_families: bool,
    distinct_providers: bool,
    no_shared_ancestry: bool,
    fresh_context: bool,
}

impl QuorumReport {
    /// Evaluates only fully submitted cycles for the exact current binding.
    #[must_use]
    pub fn evaluate(binding: &ReviewBinding, cycles: &[ReviewCycle]) -> Self {
        let current = cycles
            .iter()
            .filter(|cycle| {
                cycle.phase() == ReviewCyclePhase::Submitted
                    && cycle.assignment().binding_digest() == binding.digest()
                    && cycle.assignment().revision() == binding.revision()
                    && cycle.submission().is_some()
            })
            .collect::<Vec<_>>();
        let submitted_reviews = u16::try_from(current.len()).unwrap_or(u16::MAX);
        let mut categories = current
            .iter()
            .flat_map(|cycle| {
                cycle.submission().into_iter().flat_map(crate::ReviewSubmission::categories)
            })
            .copied()
            .collect::<Vec<_>>();
        categories.sort_unstable();
        categories.dedup();

        let identities = binding.independence();
        let reviewer_values = current
            .iter()
            .map(|cycle| cycle.assignment().reviewer().actor_id())
            .collect::<Vec<_>>();
        let context_values =
            current.iter().map(|cycle| cycle.assignment().reviewer().context()).collect::<Vec<_>>();
        let model_values = current
            .iter()
            .map(|cycle| cycle.assignment().reviewer().model_family())
            .collect::<Vec<_>>();
        let provider_values = current
            .iter()
            .map(|cycle| cycle.assignment().reviewer().provider())
            .collect::<Vec<_>>();
        let ancestry_values = current
            .iter()
            .map(|cycle| cycle.assignment().reviewer().ancestry())
            .collect::<Vec<_>>();
        Self::from_wire(
            submitted_reviews,
            categories.clone(),
            submitted_reviews >= binding.reviewer_quorum(),
            binding
                .required_categories()
                .iter()
                .all(|required| categories.binary_search(required).is_ok()),
            !identities.distinct_reviewers() || all_unique(reviewer_values),
            !identities.independent_from_producer()
                || current.iter().all(|cycle| {
                    let reviewer = cycle.assignment().reviewer();
                    reviewer.independent_from_producer()
                        && binding.producer_actors().binary_search(&reviewer.actor_id()).is_err()
                }),
            !identities.distinct_contexts() || all_unique(context_values),
            !identities.distinct_model_families() || all_unique(model_values),
            !identities.distinct_providers() || all_unique(provider_values),
            !identities.no_shared_ancestry()
                || (all_unique(ancestry_values.clone())
                    && ancestry_values.iter().all(|ancestry| {
                        binding.producer_ancestries().binary_search(ancestry).is_err()
                    })),
            !current.is_empty()
                && current.iter().all(|cycle| {
                    cycle.assignment().fresh_context()
                        && cycle.assignment().reviewer().context()
                            == cycle.assignment().context_plan_id().digest()
                }),
        )
    }

    #[allow(
        clippy::too_many_arguments,
        clippy::fn_params_excessive_bools,
        reason = "the canonical wire form retains every named quorum dimension independently"
    )]
    pub(super) const fn from_wire(
        submitted_reviews: u16,
        covered_categories: Vec<ReviewCategory>,
        submitted_review_count: bool,
        required_category_coverage: bool,
        distinct_reviewer_identities: bool,
        producer_independence: bool,
        distinct_contexts: bool,
        distinct_model_families: bool,
        distinct_providers: bool,
        no_shared_ancestry: bool,
        fresh_context: bool,
    ) -> Self {
        Self {
            submitted_reviews,
            covered_categories,
            submitted_review_count,
            required_category_coverage,
            distinct_reviewer_identities,
            producer_independence,
            distinct_contexts,
            distinct_model_families,
            distinct_providers,
            no_shared_ancestry,
            fresh_context,
        }
    }

    /// Returns the number of complete current submissions.
    #[must_use]
    pub const fn submitted_reviews(&self) -> u16 {
        self.submitted_reviews
    }
    /// Returns the canonical union of current submitted categories.
    #[must_use]
    pub const fn covered_categories(&self) -> &[ReviewCategory] {
        self.covered_categories.as_slice()
    }
    /// Returns one named dimension result.
    #[must_use]
    pub const fn passes(&self, dimension: QuorumDimension) -> bool {
        match dimension {
            QuorumDimension::SubmittedReviewCount => self.submitted_review_count,
            QuorumDimension::RequiredCategoryCoverage => self.required_category_coverage,
            QuorumDimension::DistinctReviewerIdentities => self.distinct_reviewer_identities,
            QuorumDimension::ProducerIndependence => self.producer_independence,
            QuorumDimension::DistinctContexts => self.distinct_contexts,
            QuorumDimension::DistinctModelFamilies => self.distinct_model_families,
            QuorumDimension::DistinctProviders => self.distinct_providers,
            QuorumDimension::NoSharedAncestry => self.no_shared_ancestry,
            QuorumDimension::FreshContext => self.fresh_context,
        }
    }
    /// Returns true only when every independently named dimension passes.
    #[must_use]
    pub const fn complete(&self) -> bool {
        self.submitted_review_count
            && self.required_category_coverage
            && self.distinct_reviewer_identities
            && self.producer_independence
            && self.distinct_contexts
            && self.distinct_model_families
            && self.distinct_providers
            && self.no_shared_ancestry
            && self.fresh_context
    }
}

fn all_unique<T: Ord>(values: Vec<T>) -> bool {
    let count = values.len();
    values.into_iter().collect::<BTreeSet<_>>().len() == count
}
