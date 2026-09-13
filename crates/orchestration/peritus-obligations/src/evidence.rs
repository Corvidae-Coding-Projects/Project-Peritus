//! Candidate- and ledger-bound typed obligation evidence.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use crate::{
    BrowserEvidence, LifecycleEvidence, ObligationError, ObligationErrorKind, ObligationLimits,
    PathId, PerformanceEvidence, SchemaEvidence,
};
use peritus_run_settlement::CandidateIdentity;
use peritus_spec::RequirementId;
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

/// Provenance common to every obligation evidence value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceBinding {
    requirement_id: RequirementId,
    ledger_digest: Sha256Digest,
    candidate: CandidateIdentity,
    evidence_digest: Sha256Digest,
    observed_candidate_paths: Vec<PathId>,
}

impl EvidenceBinding {
    /// Creates one canonical candidate-bound evidence identity.
    ///
    /// # Errors
    ///
    /// Rejects oversized, duplicate, or unordered observed candidate paths.
    pub fn new(
        requirement_id: RequirementId,
        ledger_digest: Sha256Digest,
        candidate: CandidateIdentity,
        evidence_digest: Sha256Digest,
        observed_candidate_paths: Vec<PathId>,
        limits: ObligationLimits,
    ) -> Result<Self, ObligationError> {
        if observed_candidate_paths.len() > limits.max_paths_per_requirement() {
            return Err(ObligationError::numbers(
                ObligationErrorKind::LimitExceeded,
                limits.max_paths_per_requirement() as u64,
                observed_candidate_paths.len() as u64,
            ));
        }
        let mut index = 0;
        while index < observed_candidate_paths.len()
            invariant index <= observed_candidate_paths.len(),
            decreases observed_candidate_paths.len() - index,
        {
            if index > 0 {
                if observed_candidate_paths[index - 1] == observed_candidate_paths[index] {
                    return Err(ObligationError::plain(ObligationErrorKind::DuplicateValue));
                }
                if observed_candidate_paths[index - 1] > observed_candidate_paths[index] {
                    return Err(ObligationError::plain(ObligationErrorKind::NonCanonicalOrder));
                }
            }
            index += 1;
        }
        Ok(Self {
            requirement_id,
            ledger_digest,
            candidate,
            evidence_digest,
            observed_candidate_paths,
        })
    }

    /// Exact requirement identity.
    #[must_use]
    pub const fn requirement_id(&self) -> RequirementId { self.requirement_id }

    /// Exact ledger extraction digest.
    #[must_use]
    pub const fn ledger_digest(&self) -> Sha256Digest { self.ledger_digest }

    /// Candidate checkpoint producing the observation.
    #[must_use]
    pub const fn candidate(&self) -> &CandidateIdentity { &self.candidate }

    /// Digest of the complete evidence payload at its observing boundary.
    #[must_use]
    pub const fn evidence_digest(&self) -> Sha256Digest { self.evidence_digest }

    /// Candidate paths directly observed by this evidence.
    #[must_use]
    pub const fn observed_candidate_paths(&self) -> &[PathId] {
        self.observed_candidate_paths.as_slice()
    }

    /// Whether this binding is current for an exact requirement, ledger, and candidate.
    #[must_use]
    pub fn is_current_for(
        &self,
        requirement_id: RequirementId,
        ledger_digest: Sha256Digest,
        candidate: &CandidateIdentity,
    ) -> bool {
        self.requirement_id == requirement_id
            && self.ledger_digest == ledger_digest
            && self.candidate.same_candidate(candidate)
            && self.candidate.checkpoint_sequence() <= candidate.checkpoint_sequence()
    }

    /// Whether all mandatory candidate paths are present in this canonical observation.
    #[must_use]
    pub fn contains_path(&self, path_id: PathId) -> bool {
        let mut index = 0;
        while index < self.observed_candidate_paths.len()
            invariant index <= self.observed_candidate_paths.len(),
            decreases self.observed_candidate_paths.len() - index,
        {
            if self.observed_candidate_paths[index] == path_id {
                return true;
            }
            if self.observed_candidate_paths[index] > path_id {
                return false;
            }
            index += 1;
        }
        false
    }
}

/// Generic direct evidence for hard, conditional, alternative, and generated-output clauses.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectEvidence {
    binding: EvidenceBinding,
    satisfied: bool,
}

impl DirectEvidence {
    /// Creates one direct observation.
    #[must_use]
    pub const fn new(binding: EvidenceBinding, satisfied: bool) -> Self {
        Self { binding, satisfied }
    }

    /// Complete current-candidate binding.
    #[must_use]
    pub const fn binding(&self) -> &EvidenceBinding { &self.binding }

    /// Whether the direct observation satisfied the public clause.
    #[must_use]
    pub const fn satisfied(&self) -> bool { self.satisfied }
}

/// Candidate-bound observation of one public external effect.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalEffectEvidence {
    binding: EvidenceBinding,
    effect_identity: Sha256Digest,
    observed_at_public_boundary: bool,
    completed: bool,
}

impl ExternalEffectEvidence {
    /// Creates one external-effect observation.
    #[must_use]
    pub const fn new(
        binding: EvidenceBinding,
        effect_identity: Sha256Digest,
        observed_at_public_boundary: bool,
        completed: bool,
    ) -> Self {
        Self { binding, effect_identity, observed_at_public_boundary, completed }
    }

    /// Complete current-candidate binding.
    #[must_use]
    pub const fn binding(&self) -> &EvidenceBinding { &self.binding }

    /// Exact requested effect identity.
    #[must_use]
    pub const fn effect_identity(&self) -> Sha256Digest { self.effect_identity }

    /// Whether the effect was observed outside the internal model.
    #[must_use]
    pub const fn observed_at_public_boundary(&self) -> bool {
        self.observed_at_public_boundary
    }

    /// Whether the public effect reached its requested terminal.
    #[must_use]
    pub const fn completed(&self) -> bool { self.completed }
}

/// Closed typed evidence vocabulary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RequirementEvidence {
    Direct(DirectEvidence),
    Performance(PerformanceEvidence),
    Lifecycle(LifecycleEvidence),
    Schema(SchemaEvidence),
    Browser(BrowserEvidence),
    ExternalEffect(ExternalEffectEvidence),
}

impl RequirementEvidence {
    /// Common provenance binding.
    #[must_use]
    pub const fn binding(&self) -> &EvidenceBinding {
        match self {
            Self::Direct(value) => value.binding(),
            Self::Performance(value) => value.binding(),
            Self::Lifecycle(value) => value.binding(),
            Self::Schema(value) => value.binding(),
            Self::Browser(value) => value.binding(),
            Self::ExternalEffect(value) => value.binding(),
        }
    }

    /// Stable requirement identity used for canonical ordering.
    #[must_use]
    pub const fn requirement_id(&self) -> RequirementId { self.binding().requirement_id() }
}

} // verus!
