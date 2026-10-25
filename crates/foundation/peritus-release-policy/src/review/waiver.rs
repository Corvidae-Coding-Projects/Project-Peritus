//! Independent finding-waiver observations.

use crate::{ConstructionError, EvidenceBinding, FindingId, PrincipalId, ReleaseCandidate};
use peritus_types::Sha256Digest;

#[doc = " Explicit independent waiver for one exact non-release-blocking finding."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WaiverObservation {
    finding_id: FindingId,
    binding: EvidenceBinding,
    authority: PrincipalId,
    waiver_digest: Sha256Digest,
    justification_digest: Sha256Digest,
    approved: bool,
}
impl WaiverObservation {
    #[doc = " Creates one signed waiver observation."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a typed error for a placeholder waiver or justification digest."]
    pub fn new(
        finding_id: FindingId,
        binding: EvidenceBinding,
        authority: PrincipalId,
        waiver_digest: Sha256Digest,
        justification_digest: Sha256Digest,
        approved: bool,
    ) -> Result<Self, ConstructionError> {
        crate::validation::require_digest(waiver_digest)?;
        crate::validation::require_digest(justification_digest)?;
        Ok(Self { finding_id, binding, authority, waiver_digest, justification_digest, approved })
    }
    #[doc = " Returns the exact finding identity."]
    #[must_use]
    pub const fn finding_id(&self) -> FindingId {
        self.finding_id
    }
    #[doc = " Returns the exact candidate/time/sequence/revision binding."]
    #[must_use]
    pub const fn binding(&self) -> EvidenceBinding {
        self.binding
    }
    #[doc = " Returns the admitted waiver authority."]
    #[must_use]
    pub const fn authority(&self) -> PrincipalId {
        self.authority
    }
    #[doc = " Returns the signed waiver digest."]
    #[must_use]
    pub const fn waiver_digest(&self) -> Sha256Digest {
        self.waiver_digest
    }
    #[doc = " Returns the retained justification digest."]
    #[must_use]
    pub const fn justification_digest(&self) -> Sha256Digest {
        self.justification_digest
    }
    #[doc = " Returns whether authority approved the waiver."]
    #[must_use]
    pub const fn approved(&self) -> bool {
        self.approved
    }
    #[doc = " Returns whether the waiver is exact and current for the evaluation candidate."]
    #[must_use]
    pub const fn is_current_for(&self, candidate: ReleaseCandidate, evaluated_at: u64) -> bool {
        self.binding().is_current_for(candidate, evaluated_at)
    }
}
