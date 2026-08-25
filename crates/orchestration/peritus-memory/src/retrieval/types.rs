//! Checked retrieval inputs and immutable explainable outputs.

use crate::{
    BasisPoints, ClaimTypeSet, Confidence, FeatureKey, MemoryError, MemoryErrorKind, MemoryField,
    MemoryScope, Observation, RetrievalFeatures, ScopePolicy,
};
use peritus_role::RoleProfile;
use vstd::prelude::*;

verus! {

/// Maximum selected results in one retrieval plan.
pub const MAX_RETRIEVAL_RESULTS: u16 = 256;
/// Maximum records or tombstones accepted by one in-process plan.
pub const MAX_RETRIEVAL_INPUTS: usize = 4_096;

/// Canonical feature keys that every eligible record must provide.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequiredFeatures {
    values: Vec<FeatureKey>,
}

impl RequiredFeatures {
    /// Creates a canonical bounded key set. Empty sets are valid.
    ///
    /// # Errors
    ///
    /// Returns a typed error for excessive, duplicate, or unordered keys.
    pub fn new(values: Vec<FeatureKey>) -> Result<Self, MemoryError> {
        if values.len() > crate::claim::MAX_RETRIEVAL_FEATURES {
            return Err(MemoryError::field(MemoryErrorKind::LimitExceeded, MemoryField::Features));
        }
        if values.len() > 1 {
            let mut index = 1;
            while index < values.len()
                invariant 1 <= index <= values.len(),
                decreases values.len() - index,
            {
                if values[index - 1] == values[index] {
                    return Err(MemoryError::feature(
                        MemoryErrorKind::DuplicateValue,
                        values[index],
                    ));
                }
                if values[index - 1] > values[index] {
                    return Err(MemoryError::feature(
                        MemoryErrorKind::NonCanonicalOrder,
                        values[index],
                    ));
                }
                index += 1;
            }
        }
        Ok(Self { values })
    }

    /// Returns an empty requirement set.
    #[must_use]
    pub const fn empty() -> Self { Self { values: Vec::new() } }

    /// Returns required keys in canonical order.
    #[must_use]
    pub const fn values(&self) -> &[FeatureKey] { self.values.as_slice() }
}

/// Relative integer weights for the six required ranking components.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RankingWeights {
    scope: BasisPoints,
    relevance: BasisPoints,
    confidence: BasisPoints,
    evidence: BasisPoints,
    recency: BasisPoints,
    feedback: BasisPoints,
}

impl RankingWeights {
    /// Creates weights whose sum is exactly 10,000 basis points.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryErrorKind::InvalidBound`] unless the exact sum is 10,000.
    pub fn new(
        scope: BasisPoints,
        relevance: BasisPoints,
        confidence: BasisPoints,
        evidence: BasisPoints,
        recency: BasisPoints,
        feedback: BasisPoints,
    ) -> Result<Self, MemoryError> {
        let sum = u32::from(scope.get())
            + u32::from(relevance.get())
            + u32::from(confidence.get())
            + u32::from(evidence.get())
            + u32::from(recency.get())
            + u32::from(feedback.get());
        if sum != 10_000 {
            return Err(MemoryError::field(MemoryErrorKind::InvalidBound, MemoryField::Score));
        }
        Ok(Self { scope, relevance, confidence, evidence, recency, feedback })
    }

    /// Returns the scope-specificity weight.
    #[must_use]
    pub const fn scope(self) -> BasisPoints { self.scope }
    /// Returns the feature-relevance weight.
    #[must_use]
    pub const fn relevance(self) -> BasisPoints { self.relevance }
    /// Returns the confidence weight.
    #[must_use]
    pub const fn confidence(self) -> BasisPoints { self.confidence }
    /// Returns the evidence-balance weight.
    #[must_use]
    pub const fn evidence(self) -> BasisPoints { self.evidence }
    /// Returns the recency weight.
    #[must_use]
    pub const fn recency(self) -> BasisPoints { self.recency }
    /// Returns the feedback weight.
    #[must_use]
    pub const fn feedback(self) -> BasisPoints { self.feedback }
}

/// Explicit negative-signal policy applied before ranking.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FeedbackPolicy {
    negative_quarantine_at: Option<BasisPoints>,
    contradiction_quarantine_at: Option<BasisPoints>,
}

impl FeedbackPolicy {
    /// Creates explicit optional quarantine thresholds.
    #[must_use]
    pub const fn new(
        negative_quarantine_at: Option<BasisPoints>,
        contradiction_quarantine_at: Option<BasisPoints>,
    ) -> Self {
        Self { negative_quarantine_at, contradiction_quarantine_at }
    }

    /// Returns the negative-feedback quarantine threshold.
    #[must_use]
    pub const fn negative_quarantine_at(self) -> Option<BasisPoints> {
        self.negative_quarantine_at
    }

    /// Returns the contradiction-ratio quarantine threshold.
    #[must_use]
    pub const fn contradiction_quarantine_at(self) -> Option<BasisPoints> {
        self.contradiction_quarantine_at
    }
}

/// Checked result, confidence, and review-freshness limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetrievalLimits {
    max_results: u16,
    minimum_confidence: Confidence,
    max_review_age: Option<u64>,
}

impl RetrievalLimits {
    /// Creates bounded retrieval limits.
    ///
    /// # Errors
    ///
    /// Returns a typed error for zero or more than 256 results.
    pub const fn new(
        max_results: u16,
        minimum_confidence: Confidence,
        max_review_age: Option<u64>,
    ) -> Result<Self, MemoryError> {
        if max_results == 0 || max_results > MAX_RETRIEVAL_RESULTS {
            return Err(MemoryError::field(
                MemoryErrorKind::InvalidBound,
                MemoryField::ResultLimit,
            ));
        }
        Ok(Self { max_results, minimum_confidence, max_review_age })
    }

    /// Returns the maximum selected count.
    #[must_use]
    pub const fn max_results(self) -> u16 { self.max_results }
    /// Returns the minimum confidence.
    #[must_use]
    pub const fn minimum_confidence(self) -> Confidence { self.minimum_confidence }
    /// Returns required review freshness in ticks within the same epoch.
    #[must_use]
    pub const fn max_review_age(self) -> Option<u64> { self.max_review_age }
}

/// Immutable retrieval policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetrievalPolicy {
    limits: RetrievalLimits,
    accepted_claims: ClaimTypeSet,
    ranking: RankingWeights,
    feedback: FeedbackPolicy,
    scope: ScopePolicy,
}

impl RetrievalPolicy {
    /// Creates policy from independently checked groups.
    #[must_use]
    pub const fn new(
        limits: RetrievalLimits,
        accepted_claims: ClaimTypeSet,
        ranking: RankingWeights,
        feedback: FeedbackPolicy,
        scope: ScopePolicy,
    ) -> Self {
        Self { limits, accepted_claims, ranking, feedback, scope }
    }

    /// Returns result, confidence, and freshness limits.
    #[must_use]
    pub const fn limits(&self) -> RetrievalLimits { self.limits }
    /// Returns accepted claim categories.
    #[must_use]
    pub const fn accepted_claims(&self) -> &ClaimTypeSet { &self.accepted_claims }
    /// Returns integer ranking weights.
    #[must_use]
    pub const fn ranking(&self) -> RankingWeights { self.ranking }
    /// Returns explicit negative-signal behavior.
    #[must_use]
    pub const fn feedback(&self) -> FeedbackPolicy { self.feedback }
    /// Returns scope compatibility behavior.
    #[must_use]
    pub const fn scope_policy(&self) -> ScopePolicy { self.scope }
}

/// One immutable caller-supplied retrieval request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetrievalQuery {
    scope: MemoryScope,
    role: RoleProfile,
    observation: Observation,
    features: RetrievalFeatures,
    required_features: RequiredFeatures,
    token_budget: u32,
}

impl RetrievalQuery {
    /// Creates a checked query with a nonzero token budget.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryErrorKind::InvalidBound`] for a zero budget.
    pub fn new(
        scope: MemoryScope,
        role: RoleProfile,
        observation: Observation,
        features: RetrievalFeatures,
        required_features: RequiredFeatures,
        token_budget: u32,
    ) -> Result<Self, MemoryError> {
        if token_budget == 0 {
            return Err(MemoryError::field(
                MemoryErrorKind::InvalidBound,
                MemoryField::TokenBudget,
            ));
        }
        Ok(Self { scope, role, observation, features, required_features, token_budget })
    }

    /// Returns the exact query scope.
    #[must_use]
    pub const fn scope(&self) -> &MemoryScope { &self.scope }
    /// Returns the frozen role profile used for memory visibility.
    #[must_use]
    pub const fn role(&self) -> &RoleProfile { &self.role }
    /// Returns the caller-supplied logical observation.
    #[must_use]
    pub const fn observation(&self) -> Observation { self.observation }
    /// Returns canonical query features.
    #[must_use]
    pub const fn features(&self) -> &RetrievalFeatures { &self.features }
    /// Returns canonical required feature keys.
    #[must_use]
    pub const fn required_features(&self) -> &RequiredFeatures { &self.required_features }
    /// Returns the exact selected-token budget.
    #[must_use]
    pub const fn token_budget(&self) -> (result: u32)
        ensures result == self.spec_token_budget(),
    {
        self.token_budget
    }

    /// Returns the mathematical token budget used by specifications.
    pub closed spec fn spec_token_budget(&self) -> int { self.token_budget as int }
}

} // verus!
