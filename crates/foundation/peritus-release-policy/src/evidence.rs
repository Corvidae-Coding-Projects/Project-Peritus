//! Bounded observations for canonical H4 release evidence.

mod aggregate;

pub use self::aggregate::ReleaseEvidence;

use crate::{
    ConstructionError, ConstructionErrorKind, EvidenceRequirement, EvidenceSourceKind,
    ReleaseCandidate,
};
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

/// Exact candidate, time, sequence, and source-revision binding for one observation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EvidenceBinding {
    candidate: ReleaseCandidate,
    observed_at: u64,
    expires_at: u64,
    sequence: u64,
    source_revision: u64,
}

impl EvidenceBinding {
    /// Creates a checked evidence binding.
    ///
    /// Times are monotonic release-clock ticks. They are not interpreted as wall-clock time by the
    /// policy. The source revision must equal [`ReleaseCandidate::source_revision`] to contribute.
    ///
    /// # Errors
    ///
    /// Returns a typed error for zero sequence/revision or an inverted validity interval.
    pub const fn new(
        candidate: ReleaseCandidate,
        observed_at: u64,
        expires_at: u64,
        sequence: u64,
        source_revision: u64,
    ) -> Result<Self, ConstructionError> {
        if sequence == 0 || source_revision == 0 {
            return Err(ConstructionError::new(ConstructionErrorKind::ZeroRevision));
        }
        if expires_at < observed_at {
            return Err(ConstructionError::new(
                ConstructionErrorKind::InvalidValidityInterval,
            ));
        }
        Ok(Self { candidate, observed_at, expires_at, sequence, source_revision })
    }

    /// Returns the exact candidate named by the observation.
    #[must_use]
    pub const fn candidate(&self) -> (candidate: ReleaseCandidate)
        ensures candidate == self.spec_candidate()
    {
        self.candidate
    }

    /// Returns the monotonic observation tick.
    #[must_use]
    pub const fn observed_at(&self) -> (observed_at: u64)
        ensures observed_at == self.spec_observed_at()
    {
        self.observed_at
    }

    /// Returns the inclusive expiration tick.
    #[must_use]
    pub const fn expires_at(&self) -> (expires_at: u64)
        ensures expires_at == self.spec_expires_at()
    {
        self.expires_at
    }

    /// Returns the positive source sequence.
    #[must_use]
    pub const fn sequence(&self) -> (sequence: u64)
        ensures sequence == self.spec_sequence()
    {
        self.sequence
    }

    /// Returns the positive producing source revision.
    #[must_use]
    pub const fn source_revision(&self) -> (source_revision: u64)
        ensures source_revision == self.spec_source_revision()
    {
        self.source_revision
    }

    /// Logical view of the exact candidate named by the observation.
    pub closed spec fn spec_candidate(&self) -> ReleaseCandidate { self.candidate }

    /// Logical view of the monotonic observation tick.
    pub closed spec fn spec_observed_at(&self) -> u64 { self.observed_at }

    /// Logical view of the inclusive expiration tick.
    pub closed spec fn spec_expires_at(&self) -> u64 { self.expires_at }

    /// Logical view of the positive source sequence.
    pub closed spec fn spec_sequence(&self) -> u64 { self.sequence }

    /// Logical view of the positive producing source revision.
    pub closed spec fn spec_source_revision(&self) -> u64 { self.source_revision }

    /// Specification predicate for exact-current binding.
    pub open spec fn spec_is_current_for(
        &self,
        candidate: ReleaseCandidate,
        evaluated_at: u64,
    ) -> bool {
        crate::candidate::digest_bytes_equal_from(
            self.spec_candidate().spec_manifest_digest().spec_bytes(),
            candidate.spec_manifest_digest().spec_bytes(),
            0,
        )
            && self.spec_source_revision() == candidate.spec_source_revision()
            && self.spec_observed_at() <= evaluated_at
            && evaluated_at <= self.spec_expires_at()
    }

    /// Returns whether this binding is exact and current for an evaluation.
    #[must_use]
    pub fn is_current_for(
        &self,
        candidate: ReleaseCandidate,
        evaluated_at: u64,
    ) -> (current: bool)
        ensures current ==> self.spec_is_current_for(candidate, evaluated_at)
    {
        let current = self.candidate() == candidate
            && crate::candidate::digests_equal(
                self.candidate().manifest_digest(),
                candidate.manifest_digest(),
            )
            && self.source_revision() == candidate.source_revision()
            && self.observed_at() <= evaluated_at
            && evaluated_at <= self.expires_at();
        proof {
            reveal(EvidenceBinding::spec_is_current_for);
        }
        current
    }

    /// Returns whether candidate or producing-revision identity differs.
    #[must_use]
    pub fn is_mismatched(&self, candidate: ReleaseCandidate) -> bool {
        self.candidate != candidate || self.source_revision != candidate.source_revision()
    }

    /// Returns whether only the time window makes the otherwise exact binding stale.
    #[must_use]
    pub fn is_stale_at(&self, candidate: ReleaseCandidate, evaluated_at: u64) -> bool {
        self.candidate == candidate
            && self.source_revision == candidate.source_revision()
            && (evaluated_at < self.observed_at || evaluated_at > self.expires_at)
    }
}

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
    /// Returns [`ConstructionErrorKind::ZeroDigest`] for an artifact placeholder, or for a signed
    /// observation whose attestation digest is a placeholder.
    #[allow(clippy::too_many_arguments, reason = "evidence authenticity and status remain explicit inputs")]
    pub fn new(
        requirement: EvidenceRequirement,
        source_kind: EvidenceSourceKind,
        binding: EvidenceBinding,
        artifact_digest: Sha256Digest,
        attestation_digest: Sha256Digest,
        reviewed: bool,
        signed: bool,
    ) -> Result<Self, ConstructionError> {
        crate::candidate::require_digest(artifact_digest)?;
        if signed {
            crate::candidate::require_digest(attestation_digest)?;
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
    pub const fn artifact_digest(&self) -> Sha256Digest { self.artifact_digest }

    /// Returns the detached attestation digest.
    #[must_use]
    pub const fn attestation_digest(&self) -> Sha256Digest { self.attestation_digest }

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
    pub fn contributes_to(
        &self,
        requirement: EvidenceRequirement,
        candidate: ReleaseCandidate,
        evaluated_at: u64,
    ) -> (contributes: bool)
        ensures contributes ==> self.spec_contributes_to(requirement, candidate, evaluated_at)
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
