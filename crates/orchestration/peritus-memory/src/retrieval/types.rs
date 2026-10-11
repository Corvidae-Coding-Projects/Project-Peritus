//! Checked retrieval inputs and immutable explainable outputs.

use crate::{
    BasisPoints, ClaimTypeSet, Confidence, FeatureKey, MemoryError, MemoryErrorKind, MemoryField,
    MemoryScope, Observation, RetrievalFeatures, ScopePolicy,
};
use peritus_role::RoleProfile;

#[doc = " Maximum selected results in one retrieval plan."]
pub const MAX_RETRIEVAL_RESULTS: u16 = 256;
#[doc = " Maximum records or tombstones accepted by one in-process plan."]
pub const MAX_RETRIEVAL_INPUTS: usize = 4_096;
#[doc = " Canonical feature keys that every eligible record must provide."]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequiredFeatures {
    values: Vec<FeatureKey>,
}
impl RequiredFeatures {
    #[doc = " Creates a canonical bounded key set. Empty sets are valid."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a typed error for excessive, duplicate, or unordered keys."]
    pub fn new(values: Vec<FeatureKey>) -> Result<Self, MemoryError> {
        if values.len() > crate::claim::MAX_RETRIEVAL_FEATURES {
            return Err(MemoryError::field(MemoryErrorKind::LimitExceeded, MemoryField::Features));
        }
        if values.len() > 1 {
            let mut index = 1;
            while index < values.len() {
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
    #[doc = " Returns an empty requirement set."]
    #[must_use]
    pub const fn empty() -> Self {
        Self { values: Vec::new() }
    }
    #[doc = " Returns required keys in canonical order."]
    #[must_use]
    pub const fn values(&self) -> &[FeatureKey] {
        self.values.as_slice()
    }
}
#[doc = " Relative integer weights for the six required ranking components."]
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
    #[doc = " Creates weights whose sum is exactly 10,000 basis points."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`MemoryErrorKind::InvalidBound`] unless the exact sum is 10,000."]
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
    #[doc = " Returns the scope-specificity weight."]
    #[must_use]
    pub const fn scope(self) -> BasisPoints {
        self.scope
    }
    #[doc = " Returns the feature-relevance weight."]
    #[must_use]
    pub const fn relevance(self) -> BasisPoints {
        self.relevance
    }
    #[doc = " Returns the confidence weight."]
    #[must_use]
    pub const fn confidence(self) -> BasisPoints {
        self.confidence
    }
    #[doc = " Returns the evidence-balance weight."]
    #[must_use]
    pub const fn evidence(self) -> BasisPoints {
        self.evidence
    }
    #[doc = " Returns the recency weight."]
    #[must_use]
    pub const fn recency(self) -> BasisPoints {
        self.recency
    }
    #[doc = " Returns the feedback weight."]
    #[must_use]
    pub const fn feedback(self) -> BasisPoints {
        self.feedback
    }
}
#[doc = " Explicit negative-signal policy applied before ranking."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FeedbackPolicy {
    negative_quarantine_at: Option<BasisPoints>,
    contradiction_quarantine_at: Option<BasisPoints>,
}
impl FeedbackPolicy {
    #[doc = " Creates explicit optional quarantine thresholds."]
    #[must_use]
    pub const fn new(
        negative_quarantine_at: Option<BasisPoints>,
        contradiction_quarantine_at: Option<BasisPoints>,
    ) -> Self {
        Self { negative_quarantine_at, contradiction_quarantine_at }
    }
    #[doc = " Returns the negative-feedback quarantine threshold."]
    #[must_use]
    pub const fn negative_quarantine_at(self) -> Option<BasisPoints> {
        self.negative_quarantine_at
    }
    #[doc = " Returns the contradiction-ratio quarantine threshold."]
    #[must_use]
    pub const fn contradiction_quarantine_at(self) -> Option<BasisPoints> {
        self.contradiction_quarantine_at
    }
}
#[doc = " Checked result, confidence, and review-freshness limits."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetrievalLimits {
    max_results: u16,
    minimum_confidence: Confidence,
    max_review_age: Option<u64>,
}
impl RetrievalLimits {
    #[doc = " Creates bounded retrieval limits."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a typed error for zero or more than 256 results."]
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
    #[doc = " Returns the maximum selected count."]
    #[must_use]
    pub const fn max_results(self) -> u16 {
        self.max_results
    }
    #[doc = " Returns the minimum confidence."]
    #[must_use]
    pub const fn minimum_confidence(self) -> Confidence {
        self.minimum_confidence
    }
    #[doc = " Returns required review freshness in ticks within the same epoch."]
    #[must_use]
    pub const fn max_review_age(self) -> Option<u64> {
        self.max_review_age
    }
}
#[doc = " Immutable retrieval policy."]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetrievalPolicy {
    limits: RetrievalLimits,
    accepted_claims: ClaimTypeSet,
    ranking: RankingWeights,
    feedback: FeedbackPolicy,
    scope: ScopePolicy,
}
impl RetrievalPolicy {
    #[doc = " Creates policy from independently checked groups."]
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
    #[doc = " Returns result, confidence, and freshness limits."]
    #[must_use]
    pub const fn limits(&self) -> RetrievalLimits {
        self.limits
    }
    #[doc = " Returns accepted claim categories."]
    #[must_use]
    pub const fn accepted_claims(&self) -> &ClaimTypeSet {
        &self.accepted_claims
    }
    #[doc = " Returns integer ranking weights."]
    #[must_use]
    pub const fn ranking(&self) -> RankingWeights {
        self.ranking
    }
    #[doc = " Returns explicit negative-signal behavior."]
    #[must_use]
    pub const fn feedback(&self) -> FeedbackPolicy {
        self.feedback
    }
    #[doc = " Returns scope compatibility behavior."]
    #[must_use]
    pub const fn scope_policy(&self) -> ScopePolicy {
        self.scope
    }
}
#[doc = " One immutable caller-supplied retrieval request."]
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
    #[doc = " Creates a checked query with a nonzero token budget."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`MemoryErrorKind::InvalidBound`] for a zero budget."]
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
    #[doc = " Returns the exact query scope."]
    #[must_use]
    pub const fn scope(&self) -> &MemoryScope {
        &self.scope
    }
    #[doc = " Returns the frozen role profile used for memory visibility."]
    #[must_use]
    pub const fn role(&self) -> &RoleProfile {
        &self.role
    }
    #[doc = " Returns the caller-supplied logical observation."]
    #[must_use]
    pub const fn observation(&self) -> Observation {
        self.observation
    }
    #[doc = " Returns canonical query features."]
    #[must_use]
    pub const fn features(&self) -> &RetrievalFeatures {
        &self.features
    }
    #[doc = " Returns canonical required feature keys."]
    #[must_use]
    pub const fn required_features(&self) -> &RequiredFeatures {
        &self.required_features
    }
    #[doc = " Returns the exact selected-token budget."]
    #[must_use]
    pub const fn token_budget(&self) -> u32 {
        self.token_budget
    }
}
