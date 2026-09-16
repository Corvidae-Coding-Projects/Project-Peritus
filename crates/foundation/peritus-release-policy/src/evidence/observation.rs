//! One immutable observation of a release evidence requirement.

use super::EvidenceBinding;
use crate::{
    ConstructionError, EvidenceRequirement, EvidenceSourceKind, ReleaseCandidate,
};
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

/// One immutable artifact observation for a closed H4 evidence requirement.
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
    /// Creates one checked evidence observation.
    ///
    /// # Errors
    ///
    /// Returns [`crate::ConstructionErrorKind::ZeroDigest`] for an artifact placeholder, or for a
    /// signed observation whose attestation digest is a placeholder.
    #[allow(clippy::too_many_arguments, reason = "evidence authenticity and status remain explicit inputs")]
    pub fn new(
        requirement: EvidenceRequirement,
        source_kind: EvidenceSourceKind,
        binding: EvidenceBinding,
        artifact_digest: Sha256Digest,
        attestation_digest: Sha256Digest,
        reviewed: bool,
        signed: bool,
    ) -> (result: Result<Self, ConstructionError>)
        ensures
            result.is_ok() == (crate::validation::spec_digest_nonzero(artifact_digest)
                && (!signed || crate::validation::spec_digest_nonzero(attestation_digest))),
            match result {
                Ok(value) => value.spec_requirement() == requirement
                    && value.spec_source_kind() == source_kind
                    && value.spec_binding() == binding
                    && value.spec_artifact_digest() == artifact_digest
                    && value.spec_attestation_digest() == attestation_digest
                    && value.spec_reviewed() == reviewed
                    && value.spec_signed() == signed,
                Err(error) => error.spec_kind() == crate::ConstructionErrorKind::ZeroDigest,
            },
    {
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

    /// Returns the closed evidence requirement.
    #[must_use]
    pub const fn requirement(&self) -> (requirement: EvidenceRequirement)
        ensures requirement == self.spec_requirement()
    {
        self.requirement
    }

    /// Returns the authenticated source class.
    #[must_use]
    pub const fn source_kind(&self) -> (source_kind: EvidenceSourceKind)
        ensures source_kind == self.spec_source_kind()
    {
        self.source_kind
    }

    /// Returns the exact candidate/time/sequence/revision binding.
    #[must_use]
    pub const fn binding(&self) -> (binding: EvidenceBinding)
        ensures binding == self.spec_binding()
    {
        self.binding
    }

    /// Returns the retained artifact digest.
    #[must_use]
    pub const fn artifact_digest(&self) -> (digest: Sha256Digest)
        ensures digest == self.spec_artifact_digest()
    {
        self.artifact_digest
    }

    /// Returns the detached attestation digest.
    #[must_use]
    pub const fn attestation_digest(&self) -> (digest: Sha256Digest)
        ensures digest == self.spec_attestation_digest()
    {
        self.attestation_digest
    }

    /// Returns whether independent review was completed.
    #[must_use]
    pub const fn reviewed(&self) -> (reviewed: bool)
        ensures reviewed == self.spec_reviewed()
    {
        self.reviewed
    }

    /// Returns whether the source authenticated the observation.
    #[must_use]
    pub const fn signed(&self) -> (signed: bool)
        ensures signed == self.spec_signed()
    {
        self.signed
    }

    /// Logical view of the closed evidence requirement.
    pub closed spec fn spec_requirement(&self) -> EvidenceRequirement { self.requirement }

    /// Logical view of the authenticated source class.
    pub closed spec fn spec_source_kind(&self) -> EvidenceSourceKind { self.source_kind }

    /// Logical view of the exact candidate, time, sequence, and revision binding.
    pub closed spec fn spec_binding(&self) -> EvidenceBinding { self.binding }

    /// Logical view of the retained artifact digest.
    pub closed spec fn spec_artifact_digest(&self) -> Sha256Digest { self.artifact_digest }

    /// Logical view of the detached attestation digest.
    pub closed spec fn spec_attestation_digest(&self) -> Sha256Digest { self.attestation_digest }

    /// Logical view of whether independent review was completed.
    pub closed spec fn spec_reviewed(&self) -> bool { self.reviewed }

    /// Logical view of whether the source authenticated the observation.
    pub closed spec fn spec_signed(&self) -> bool { self.signed }

    /// Specification predicate for evidence permitted to contribute to readiness.
    pub open spec fn spec_contributes_to(
        &self,
        requirement: EvidenceRequirement,
        candidate: ReleaseCandidate,
        evaluated_at: u64,
    ) -> bool {
        self.spec_requirement() == requirement
            && self.spec_source_kind() == requirement.spec_source_kind()
            && self.spec_binding().spec_is_current_for(candidate, evaluated_at)
            && self.spec_reviewed()
            && self.spec_signed()
    }

    /// Returns whether this observation may contribute to one required assessment.
    #[must_use]
    pub const fn contributes_to(
        &self,
        requirement: EvidenceRequirement,
        candidate: ReleaseCandidate,
        evaluated_at: u64,
    ) -> (contributes: bool)
        ensures contributes == self.spec_contributes_to(requirement, candidate, evaluated_at)
    {
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
        proof {
            reveal(EvidenceObservation::spec_contributes_to);
            reveal(EvidenceRequirement::spec_source_kind);
            assert(self.spec_requirement() == requirement);
            assert(self.spec_source_kind() == requirement.spec_source_kind());
            assert(binding == self.spec_binding());
            assert(binding.spec_is_current_for(candidate, evaluated_at));
            assert(self.spec_binding().spec_is_current_for(candidate, evaluated_at));
            assert(self.spec_reviewed());
            assert(self.spec_signed());
            assert(self.spec_contributes_to(requirement, candidate, evaluated_at));
        }
        true
    }

    /// Proves that noncurrent evidence cannot contribute to readiness.
    pub proof fn noncurrent_evidence_cannot_contribute(
        &self,
        requirement: EvidenceRequirement,
        candidate: ReleaseCandidate,
        evaluated_at: u64,
    )
        requires !self.spec_binding().spec_is_current_for(candidate, evaluated_at),
        ensures !self.spec_contributes_to(requirement, candidate, evaluated_at),
    {
        reveal(EvidenceObservation::spec_contributes_to);
    }

    /// Proves that a wrong requirement or source class cannot contribute to readiness.
    pub proof fn mismatched_evidence_cannot_contribute(
        &self,
        requirement: EvidenceRequirement,
        candidate: ReleaseCandidate,
        evaluated_at: u64,
    )
        requires self.spec_requirement() != requirement
            || self.spec_source_kind() != requirement.spec_source_kind(),
        ensures !self.spec_contributes_to(requirement, candidate, evaluated_at),
    {
        reveal(EvidenceObservation::spec_contributes_to);
    }
}

} // verus!
