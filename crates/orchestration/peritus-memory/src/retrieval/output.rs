//! Immutable ranked candidates, exclusions, and complete retrieval plans.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use crate::{BasisPoints, MemoryId, MemoryMaterial, MemoryScope, MemoryState, MemoryTombstone};
use peritus_types::{RevisionNumber, Sha256Digest};

#[doc = " Six bounded components and their normalized deterministic total."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RankScore {
    scope: BasisPoints,
    relevance: BasisPoints,
    confidence: BasisPoints,
    evidence: BasisPoints,
    recency: BasisPoints,
    feedback: BasisPoints,
    total: BasisPoints,
}
impl RankScore {
    pub(crate) const fn from_components(
        scope: BasisPoints,
        relevance: BasisPoints,
        confidence: BasisPoints,
        evidence: BasisPoints,
        recency: BasisPoints,
        feedback: BasisPoints,
        total: BasisPoints,
    ) -> Self {
        Self { scope, relevance, confidence, evidence, recency, feedback, total }
    }
    #[doc = " Returns scope specificity."]
    #[must_use]
    pub const fn scope(self) -> BasisPoints {
        self.scope
    }
    #[doc = " Returns feature relevance."]
    #[must_use]
    pub const fn relevance(self) -> BasisPoints {
        self.relevance
    }
    #[doc = " Returns record confidence."]
    #[must_use]
    pub const fn confidence(self) -> BasisPoints {
        self.confidence
    }
    #[doc = " Returns supporting-versus-contradicting evidence balance."]
    #[must_use]
    pub const fn evidence(self) -> BasisPoints {
        self.evidence
    }
    #[doc = " Returns logical review recency."]
    #[must_use]
    pub const fn recency(self) -> BasisPoints {
        self.recency
    }
    #[doc = " Returns positive-versus-negative feedback balance."]
    #[must_use]
    pub const fn feedback(self) -> BasisPoints {
        self.feedback
    }
    #[doc = " Returns the normalized weighted total."]
    #[must_use]
    pub const fn total(self) -> BasisPoints {
        self.total
    }
}
#[doc = " Stable normal reason why one candidate was not selected."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExclusionReason {
    #[doc = " A deletion tombstone dominates this revision."]
    Tombstoned,
    #[doc = " Scope compatibility failed."]
    ScopeMismatch,
    #[doc = " The frozen role policy excludes all memory."]
    RolePolicy,
    #[doc = " The record is explicitly quarantined."]
    Quarantined,
    #[doc = " The record is explicitly expired."]
    Expired,
    #[doc = " The record was superseded."]
    Superseded,
    #[doc = " The optional expiry observation has passed."]
    ExpiryReached,
    #[doc = " The record describes an observation later than the query."]
    FutureObservation,
    #[doc = " Confidence is below policy."]
    BelowConfidence,
    #[doc = " The claim category is not accepted by policy."]
    UnsupportedClaim,
    #[doc = " Supporting evidence is empty."]
    UnsupportedEvidence,
    #[doc = " A required feature key is absent."]
    MissingRequiredFeature,
    #[doc = " Review is absent or older than explicit freshness policy."]
    StaleReview,
    #[doc = " Negative feedback crossed the quarantine threshold."]
    NegativeFeedback,
    #[doc = " Contradiction crossed the quarantine threshold."]
    Contradiction,
    #[doc = " A higher-ranked candidate consumed the result count limit."]
    ResultLimit,
    #[doc = " The complete candidate would exceed the token budget."]
    TokenBudget,
}
#[doc = " Metadata for an excluded candidate."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExcludedMemory {
    id: MemoryId,
    revision: RevisionNumber,
    reason: ExclusionReason,
    score: Option<RankScore>,
}
impl ExcludedMemory {
    pub(crate) const fn new(
        id: MemoryId,
        revision: RevisionNumber,
        reason: ExclusionReason,
        score: Option<RankScore>,
    ) -> Self {
        Self { id, revision, reason, score }
    }
    #[doc = " Returns the candidate identity."]
    #[must_use]
    pub const fn id(self) -> MemoryId {
        self.id
    }
    #[doc = " Returns the candidate revision."]
    #[must_use]
    pub const fn revision(self) -> RevisionNumber {
        self.revision
    }
    #[doc = " Returns the typed exclusion reason."]
    #[must_use]
    pub const fn reason(self) -> ExclusionReason {
        self.reason
    }
    #[doc = " Returns ranking detail when filtering reached the budget/result stage."]
    #[must_use]
    pub const fn score(self) -> Option<RankScore> {
        self.score
    }
}
#[doc = " Selected non-authoritative memory metadata with mandatory quote boundaries."]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryCandidate {
    id: MemoryId,
    revision: RevisionNumber,
    scope: MemoryScope,
    material: MemoryMaterial,
    score: RankScore,
}
impl MemoryCandidate {
    pub(crate) const fn new(
        id: MemoryId,
        revision: RevisionNumber,
        scope: MemoryScope,
        material: MemoryMaterial,
        ranking_score: RankScore,
    ) -> Self {
        Self { id, revision, scope, material, score: ranking_score }
    }
    #[doc = " Returns the stable memory identifier."]
    #[must_use]
    pub const fn id(&self) -> MemoryId {
        self.id
    }
    #[doc = " Returns the selected record revision."]
    #[must_use]
    pub const fn revision(&self) -> RevisionNumber {
        self.revision
    }
    #[doc = " Returns the exact compatible scope."]
    #[must_use]
    pub const fn scope(&self) -> &MemoryScope {
        &self.scope
    }
    #[doc = " Returns inert payload and retained provenance."]
    #[must_use]
    pub const fn material(&self) -> &MemoryMaterial {
        &self.material
    }
    #[doc = " Returns all deterministic ranking components."]
    #[must_use]
    pub const fn score(&self) -> RankScore {
        self.score
    }
    #[doc = " Returns the exact content digest."]
    #[must_use]
    pub const fn content_digest(&self) -> Sha256Digest {
        self.material.digest()
    }
    #[doc = " Returns the nonzero estimated token cost."]
    #[must_use]
    pub const fn estimated_tokens(&self) -> u32 {
        self.material.estimated_tokens()
    }
    #[doc = " Always true: memory is quoted evidence, never executable instruction text."]
    #[must_use]
    pub const fn quoted_evidence(&self) -> bool {
        self.material.quoted_evidence()
    }
    #[doc = " Returns the mandatory opening delimiter for provider-neutral materialization."]
    #[must_use]
    pub const fn quote_open() -> &'static [u8] {
        b"<peritus-memory-evidence>"
    }
    #[doc = " Returns the mandatory closing delimiter for provider-neutral materialization."]
    #[must_use]
    pub const fn quote_close() -> &'static [u8] {
        b"</peritus-memory-evidence>"
    }
}
#[doc = " Complete outcome explanation for one input candidate."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CandidateExplanation {
    #[doc = " Candidate was selected with this exact score."]
    Selected(
        #[doc = " Stable memory lineage identifier."] MemoryId,
        #[doc = " Exact immutable selected revision."] RevisionNumber,
        #[doc = " Complete deterministic ranking score."] RankScore,
    ),
    #[doc = " Candidate was excluded for a typed reason."]
    Excluded(#[doc = " Complete exclusion metadata."] ExcludedMemory),
}
impl CandidateExplanation {
    #[doc = " Returns the explained candidate identity."]
    #[must_use]
    pub const fn id(self) -> MemoryId {
        match self {
            Self::Selected(id, _, _) => id,
            Self::Excluded(excluded) => excluded.id(),
        }
    }
}
#[doc = " Complete deterministic retrieval output."]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetrievalPlan {
    selected: Vec<MemoryCandidate>,
    explanations: Vec<CandidateExplanation>,
    token_budget: u32,
    used_tokens: u32,
}
impl RetrievalPlan {
    pub(crate) const fn new(
        selected: Vec<MemoryCandidate>,
        explanations: Vec<CandidateExplanation>,
        token_budget: u32,
        used_tokens: u32,
    ) -> Self {
        Self { selected, explanations, token_budget, used_tokens }
    }
    #[doc = " Returns selected candidates in descending rank with stable-ID tie breaking."]
    #[must_use]
    pub const fn selected(&self) -> &[MemoryCandidate] {
        self.selected.as_slice()
    }
    #[doc = " Returns one explanation per input record in stable-ID order."]
    #[must_use]
    pub const fn explanations(&self) -> &[CandidateExplanation] {
        self.explanations.as_slice()
    }
    #[doc = " Returns the caller-supplied token budget."]
    #[must_use]
    pub const fn token_budget(&self) -> u32 {
        self.token_budget
    }
    #[doc = " Returns selected estimated tokens."]
    #[must_use]
    pub const fn used_tokens(&self) -> u32 {
        self.used_tokens
    }
    #[doc = " Returns the remaining token budget."]
    #[must_use]
    pub const fn remaining_tokens(&self) -> u32 {
        match self.token_budget.checked_sub(self.used_tokens) {
            Some(remaining) => remaining,
            None => 0,
        }
    }
}
pub(super) fn dominant_tombstone(
    id: MemoryId,
    tombstones: &[MemoryTombstone],
) -> Option<&MemoryTombstone> {
    let mut found: Option<&MemoryTombstone> = None;
    let mut index = 0;
    while index < tombstones.len() {
        if tombstones[index].memory_id() == id {
            if let Some(existing) = found {
                if tombstones[index].last_known_revision() > existing.last_known_revision() {
                    found = Some(&tombstones[index]);
                }
            } else {
                found = Some(&tombstones[index]);
            }
        }
        index += 1;
    }
    found
}
pub(super) const fn state_reason(state: MemoryState) -> Option<ExclusionReason> {
    match state {
        MemoryState::Active => None,
        MemoryState::Quarantined => Some(ExclusionReason::Quarantined),
        MemoryState::Expired => Some(ExclusionReason::Expired),
        MemoryState::Superseded => Some(ExclusionReason::Superseded),
    }
}
