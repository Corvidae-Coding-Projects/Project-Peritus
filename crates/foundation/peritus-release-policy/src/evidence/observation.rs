//! One immutable observation of a release evidence requirement.

use super::EvidenceBinding;
use crate::{ConstructionError, EvidenceRequirement, EvidenceSourceKind, ReleaseCandidate};
use peritus_types::Sha256Digest;

#[doc = " One immutable artifact observation for a closed H4 evidence requirement."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EvidenceObservation {
    requirement: EvidenceRequirement,
    source_kind: EvidenceSourceKind,
    binding: EvidenceBinding,
    artifact_digest: Sha256Digest,
    attestation_digest: Sha256Digest,
    reviewed: bool,
    signed: bool,
}
impl EvidenceObservation {
    #[doc = " Creates one checked evidence observation."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`crate::ConstructionErrorKind::ZeroDigest`] for an artifact placeholder, or for a"]
    #[doc = " signed observation whose attestation digest is a placeholder."]
    #[allow(
        clippy::too_many_arguments,
        reason = "evidence authenticity and status remain explicit inputs"
    )]
    pub fn new(
        requirement: EvidenceRequirement,
        source_kind: EvidenceSourceKind,
        binding: EvidenceBinding,
        artifact_digest: Sha256Digest,
        attestation_digest: Sha256Digest,
        reviewed: bool,
        signed: bool,
    ) -> Result<Self, ConstructionError> {
        crate::validation::require_digest(artifact_digest)?;
        if signed {
            crate::validation::require_digest(attestation_digest)?;
        }
        Ok(Self {
            requirement,
            source_kind,
            binding,
            artifact_digest,
            attestation_digest,
            reviewed,
            signed,
        })
    }
    #[doc = " Returns the closed evidence requirement."]
    #[must_use]
    pub const fn requirement(&self) -> EvidenceRequirement {
        self.requirement
    }
    #[doc = " Returns the authenticated source class."]
    #[must_use]
    pub const fn source_kind(&self) -> EvidenceSourceKind {
        self.source_kind
    }
    #[doc = " Returns the exact candidate/time/sequence/revision binding."]
    #[must_use]
    pub const fn binding(&self) -> EvidenceBinding {
        self.binding
    }
    #[doc = " Returns the retained artifact digest."]
    #[must_use]
    pub const fn artifact_digest(&self) -> Sha256Digest {
        self.artifact_digest
    }
    #[doc = " Returns the detached attestation digest."]
    #[must_use]
    pub const fn attestation_digest(&self) -> Sha256Digest {
        self.attestation_digest
    }
    #[doc = " Returns whether independent review was completed."]
    #[must_use]
    pub const fn reviewed(&self) -> bool {
        self.reviewed
    }
    #[doc = " Returns whether the source authenticated the observation."]
    #[must_use]
    pub const fn signed(&self) -> bool {
        self.signed
    }
    #[doc = " Returns whether this observation may contribute to one required assessment."]
    #[must_use]
    pub const fn contributes_to(
        &self,
        requirement: EvidenceRequirement,
        candidate: ReleaseCandidate,
        evaluated_at: u64,
    ) -> bool {
        let observed_requirement = self.requirement();
        let observed_source_kind = self.source_kind();
        let required_source_kind = requirement.source_kind();
        let binding = self.binding();
        let current = binding.is_current_for(candidate, evaluated_at);
        let reviewed = self.reviewed();
        let signed = self.signed();
        if !crate::catalog::requirements_equal(observed_requirement, requirement)
            || !crate::catalog::source_kinds_equal(observed_source_kind, required_source_kind)
            || !current
            || !reviewed
            || !signed
        {
            return false;
        }
        true
    }
}
