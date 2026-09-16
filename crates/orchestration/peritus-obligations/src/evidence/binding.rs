//! Exact candidate/ledger evidence binding and canonical observed-path membership.

use crate::{ObligationError, ObligationErrorKind, ObligationLimits, PathId};
use core::cmp::Ordering;
use peritus_run_settlement::CandidateIdentity;
use peritus_spec::RequirementId;
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

pub open spec fn path_keys(paths: Seq<PathId>) -> Seq<Seq<u8>> {
    paths.map(|_index: int, path: PathId| path.spec_digest().spec_bytes()@)
}

/// Provenance common to every obligation evidence value.
#[derive(Debug, Eq, PartialEq)]
pub struct EvidenceBinding {
    requirement_id: RequirementId,
    ledger_digest: Sha256Digest,
    candidate: CandidateIdentity,
    evidence_digest: Sha256Digest,
    observed_candidate_paths: Vec<PathId>,
}

impl EvidenceBinding {
    /// Logical view of the exact requirement identity.
    pub closed spec fn spec_requirement_id(&self) -> RequirementId { self.requirement_id }
    /// Logical view of the exact ledger digest.
    pub closed spec fn spec_ledger_digest(&self) -> Sha256Digest { self.ledger_digest }
    /// Logical view of the producing candidate checkpoint.
    pub closed spec fn spec_candidate(&self) -> CandidateIdentity { self.candidate }
    /// Logical view of the complete observation digest.
    pub closed spec fn spec_evidence_digest(&self) -> Sha256Digest { self.evidence_digest }
    /// Logical view of all observed candidate paths in their actual stored order.
    pub closed spec fn spec_paths(&self) -> Seq<PathId> { self.observed_candidate_paths@ }
    /// Canonical path identities are strictly ordered and globally unique.
    pub open spec fn spec_paths_canonical(&self) -> bool {
        crate::order::ordered(path_keys(self.spec_paths()))
            && crate::order::unique(path_keys(self.spec_paths()))
    }
    #[verifier::type_invariant]
    closed spec fn invariant(&self) -> bool { self.spec_paths_canonical() }

    /// Complete semantic equality, including ordered observed paths.
    pub open spec fn spec_same_content(&self, other: &Self) -> bool {
        self.spec_requirement_id() == other.spec_requirement_id()
            && self.spec_ledger_digest() == other.spec_ledger_digest()
            && self.spec_candidate() == other.spec_candidate()
            && self.spec_evidence_digest() == other.spec_evidence_digest()
            && self.spec_paths() == other.spec_paths()
    }

    /// Exact currentness over supplied requirement, ledger, candidate and observation sequence.
    pub open spec fn spec_is_current_for(
        &self, requirement: RequirementId, ledger: Sha256Digest, candidate: &CandidateIdentity,
    ) -> bool {
        self.spec_requirement_id().spec_digest().spec_bytes()@
            == requirement.spec_digest().spec_bytes()@
            && self.spec_ledger_digest().spec_bytes()@ == ledger.spec_bytes()@
            && self.spec_candidate().spec_same_candidate(candidate)
            && self.spec_candidate().spec_checkpoint_sequence()
                <= candidate.spec_checkpoint_sequence()
    }

    /// Exact membership by every byte of the observed path identity.
    pub open spec fn spec_contains_path(&self, path: PathId) -> bool {
        exists |index: int| 0 <= index < self.spec_paths().len()
            && #[trigger] self.spec_paths()[index].spec_digest().spec_bytes()@
                == path.spec_digest().spec_bytes()@
    }

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
    ) -> (result: Result<Self, ObligationError>)
        ensures
            result.is_ok() == (observed_candidate_paths@.len() <= limits.spec_max_paths()
                && crate::order::ordered(path_keys(observed_candidate_paths@))),
            match result {
                Ok(value) => value.spec_requirement_id() == requirement_id
                    && value.spec_ledger_digest() == ledger_digest
                    && value.spec_candidate() == candidate
                    && value.spec_evidence_digest() == evidence_digest
                    && value.spec_paths() == observed_candidate_paths@
                    && value.spec_paths_canonical(),
                Err(_) => true,
            },
    {
        if observed_candidate_paths.len() > limits.max_paths_per_requirement() {
            return Err(ObligationError::numbers(
                ObligationErrorKind::LimitExceeded,
                limits.max_paths_per_requirement() as u64,
                observed_candidate_paths.len() as u64,
            ));
        }
        let mut index = 0;
        while index < observed_candidate_paths.len()
            invariant
                index <= observed_candidate_paths.len(),
                observed_candidate_paths.len() <= limits.spec_max_paths(),
                forall |prior: int| 1 <= prior < index ==>
                    crate::order::byte_order(
                        #[trigger] path_keys(observed_candidate_paths@)[prior - 1],
                        #[trigger] path_keys(observed_candidate_paths@)[prior]) == Ordering::Less,
            decreases observed_candidate_paths.len() - index,
        {
            if index > 0 {
                let order = crate::order::compare(
                    observed_candidate_paths[index - 1].digest().as_bytes(),
                    observed_candidate_paths[index].digest().as_bytes(),
                );
                proof {
                    if order != Ordering::Less {
                        assert(!crate::order::ordered(path_keys(observed_candidate_paths@))) by {
                            if crate::order::ordered(path_keys(observed_candidate_paths@)) {
                                assert(crate::order::byte_order(
                                    path_keys(observed_candidate_paths@)[index as int - 1],
                                    path_keys(observed_candidate_paths@)[index as int]) == Ordering::Less);
                            }
                        }
                    }
                }
                match order {
                    Ordering::Equal => return Err(ObligationError::plain(ObligationErrorKind::DuplicateValue)),
                    Ordering::Greater => return Err(ObligationError::plain(ObligationErrorKind::NonCanonicalOrder)),
                    Ordering::Less => {},
                }

            }
            index += 1;
        }
        proof {
            assert(crate::order::ordered(path_keys(observed_candidate_paths@)));
            crate::order::ordered_implies_unique(path_keys(observed_candidate_paths@), 32);
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
    pub const fn requirement_id(&self) -> (value: RequirementId)
        ensures value == self.spec_requirement_id(),
    { self.requirement_id }

    /// Exact ledger extraction digest.
    #[must_use]
    pub const fn ledger_digest(&self) -> (value: Sha256Digest)
        ensures value == self.spec_ledger_digest(),
    { self.ledger_digest }

    /// Candidate checkpoint producing the observation.
    #[must_use]
    pub const fn candidate(&self) -> (value: &CandidateIdentity)
        ensures *value == self.spec_candidate(),
    { &self.candidate }

    /// Digest of the complete evidence payload at its observing boundary.
    #[must_use]
    pub const fn evidence_digest(&self) -> (value: Sha256Digest)
        ensures value == self.spec_evidence_digest(),
    { self.evidence_digest }

    /// Candidate paths directly observed by this evidence.
    #[must_use]
    pub const fn observed_candidate_paths(&self) -> (value: &[PathId])
        ensures value@ == self.spec_paths(),
    {
        self.observed_candidate_paths.as_slice()
    }

    /// Whether this binding is current for an exact requirement, ledger, and candidate.
    #[must_use]
    pub fn is_current_for(
        &self,
        requirement_id: RequirementId,
        ledger_digest: Sha256Digest,
        candidate: &CandidateIdentity,
    ) -> (current: bool)
        ensures current == self.spec_is_current_for(requirement_id, ledger_digest, candidate),
    {
        matches!(crate::order::compare(self.requirement_id.digest().as_bytes(), requirement_id.digest().as_bytes()), Ordering::Equal)
            && matches!(crate::order::compare(self.ledger_digest.as_bytes(), ledger_digest.as_bytes()), Ordering::Equal)
            && self.candidate.same_candidate(candidate)
            && self.candidate.checkpoint_sequence() <= candidate.checkpoint_sequence()
    }

    /// Whether all mandatory candidate paths are present in this canonical observation.
    #[must_use]
    pub fn contains_path(&self, path_id: PathId) -> (contains: bool)
        ensures contains == self.spec_contains_path(path_id),
    {
        proof { use_type_invariant(self); }
        let mut index = 0;
        while index < self.observed_candidate_paths.len()
            invariant
                index <= self.spec_paths().len(),
                self.spec_paths_canonical(),
                forall |prior: int| 0 <= prior < index ==>
                    #[trigger] self.spec_paths()[prior].spec_digest().spec_bytes()@
                        != path_id.spec_digest().spec_bytes()@,
            decreases self.spec_paths().len() - index,
        {
            let order = crate::order::compare(
                self.observed_candidate_paths[index].digest().as_bytes(),
                path_id.digest().as_bytes(),
            );
            match order {
                Ordering::Equal => return true,
                Ordering::Greater => {
                    proof {
                        if self.spec_contains_path(path_id) {
                            let found = choose |found: int| #![trigger self.spec_paths()[found].spec_digest().spec_bytes()] 0 <= found < self.spec_paths().len()
                                && self.spec_paths()[found].spec_digest().spec_bytes()@
                                    == path_id.spec_digest().spec_bytes()@;
                            if found > index {
                                crate::order::ordered_pair(path_keys(self.spec_paths()), 32, index as int, found);
                            }
                            assert(false);
                        }
                    }
                    return false;
                },
                Ordering::Less => {},
            }
            index += 1;
        }
        false
    }
}

impl Clone for EvidenceBinding {
    fn clone(&self) -> (value: Self)
        ensures value.spec_requirement_id() == self.spec_requirement_id(),
            value.spec_ledger_digest() == self.spec_ledger_digest(),
            value.spec_candidate() == self.spec_candidate(),
            value.spec_evidence_digest() == self.spec_evidence_digest(),
            value.spec_paths() == self.spec_paths(),
    {
        proof { use_type_invariant(self); }
        let observed_candidate_paths = self.observed_candidate_paths.clone();
        proof { assert(observed_candidate_paths@ =~= self.observed_candidate_paths@); }
        Self {
            requirement_id: self.requirement_id,
            ledger_digest: self.ledger_digest,
            candidate: self.candidate,
            evidence_digest: self.evidence_digest,
            observed_candidate_paths,
        }
    }
}

} // verus!
