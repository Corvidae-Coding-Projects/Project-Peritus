//! Exact candidate/ledger evidence binding and canonical observed-path membership.

use crate::{ObligationError, ObligationErrorKind, ObligationLimits, PathId};
use core::cmp::Ordering;
use peritus_run_settlement::CandidateIdentity;
use peritus_spec::RequirementId;
use peritus_types::Sha256Digest;

#[doc = " Provenance common to every obligation evidence value."]
#[derive(Debug, Eq, PartialEq)]
pub struct EvidenceBinding {
    requirement_id: RequirementId,
    ledger_digest: Sha256Digest,
    candidate: CandidateIdentity,
    evidence_digest: Sha256Digest,
    observed_candidate_paths: Vec<PathId>,
}
impl EvidenceBinding {
    #[doc = " Creates one canonical candidate-bound evidence identity."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Rejects oversized, duplicate, or unordered observed candidate paths."]
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
        while index < observed_candidate_paths.len() {
            if index > 0 {
                let order = crate::order::compare(
                    observed_candidate_paths[index - 1].digest().as_bytes(),
                    observed_candidate_paths[index].digest().as_bytes(),
                );
                match order {
                    Ordering::Equal => {
                        return Err(ObligationError::plain(ObligationErrorKind::DuplicateValue));
                    }
                    Ordering::Greater => {
                        return Err(ObligationError::plain(ObligationErrorKind::NonCanonicalOrder));
                    }
                    Ordering::Less => {}
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
    #[doc = " Exact requirement identity."]
    #[must_use]
    pub const fn requirement_id(&self) -> RequirementId {
        self.requirement_id
    }
    #[doc = " Exact ledger extraction digest."]
    #[must_use]
    pub const fn ledger_digest(&self) -> Sha256Digest {
        self.ledger_digest
    }
    #[doc = " Candidate checkpoint producing the observation."]
    #[must_use]
    pub const fn candidate(&self) -> &CandidateIdentity {
        &self.candidate
    }
    #[doc = " Digest of the complete evidence payload at its observing boundary."]
    #[must_use]
    pub const fn evidence_digest(&self) -> Sha256Digest {
        self.evidence_digest
    }
    #[doc = " Candidate paths directly observed by this evidence."]
    #[must_use]
    pub const fn observed_candidate_paths(&self) -> &[PathId] {
        self.observed_candidate_paths.as_slice()
    }
    #[doc = " Whether this binding is current for an exact requirement, ledger, and candidate."]
    #[must_use]
    pub fn is_current_for(
        &self,
        requirement_id: RequirementId,
        ledger_digest: Sha256Digest,
        candidate: &CandidateIdentity,
    ) -> bool {
        matches!(
            crate::order::compare(
                self.requirement_id.digest().as_bytes(),
                requirement_id.digest().as_bytes()
            ),
            Ordering::Equal
        ) && matches!(
            crate::order::compare(self.ledger_digest.as_bytes(), ledger_digest.as_bytes()),
            Ordering::Equal
        ) && self.candidate.same_candidate(candidate)
            && self.candidate.checkpoint_sequence() <= candidate.checkpoint_sequence()
    }
    #[doc = " Whether all mandatory candidate paths are present in this canonical observation."]
    #[must_use]
    pub fn contains_path(&self, path_id: PathId) -> bool {
        let mut index = 0;
        while index < self.observed_candidate_paths.len() {
            let order = crate::order::compare(
                self.observed_candidate_paths[index].digest().as_bytes(),
                path_id.digest().as_bytes(),
            );
            match order {
                Ordering::Equal => return true,
                Ordering::Greater => {
                    return false;
                }
                Ordering::Less => {}
            }
            index += 1;
        }
        false
    }
}
impl Clone for EvidenceBinding {
    fn clone(&self) -> Self {
        let observed_candidate_paths = self.observed_candidate_paths.clone();
        Self {
            requirement_id: self.requirement_id,
            ledger_digest: self.ledger_digest,
            candidate: self.candidate,
            evidence_digest: self.evidence_digest,
            observed_candidate_paths,
        }
    }
}
