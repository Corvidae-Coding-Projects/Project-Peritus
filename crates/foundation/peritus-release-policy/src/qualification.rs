//! Signed H0-H3 qualification observations used as verified policy inputs.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use crate::{ConstructionError, EvidenceBinding, PrincipalId};
#[cfg(verus_only)]
use crate::ReleaseCandidate;
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

/// Required upstream qualification slice.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum QualificationSlice {
    /// H0 security qualification.
    H0Security = 0,
    /// H1 resilience qualification.
    H1Resilience = 1,
    /// H2 platform qualification.
    H2Platform = 2,
    /// H3 performance qualification.
    H3Performance = 3,
}

impl QualificationSlice {
    /// Canonical H0-H3 order.
    pub const ALL: [Self; 4] = [
        Self::H0Security,
        Self::H1Resilience,
        Self::H2Platform,
        Self::H3Performance,
    ];

    /// Returns the stable zero-based H-slice ordinal.
    #[must_use]
    pub const fn ordinal(self) -> u8 { self as u8 }
}

/// Fail-closed upstream qualification status.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum QualificationVerdict {
    /// The slice's own policy reported ready for this exact candidate.
    Ready,
    /// The slice did not report ready for this exact candidate.
    NotReadyForProduction,
}

/// Signed, independently reviewed H0/H1/H2/H3 report observation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct QualificationObservation {
    slice: QualificationSlice,
    binding: EvidenceBinding,
    verdict: QualificationVerdict,
    report_digest: Sha256Digest,
    signature_digest: Sha256Digest,
    signer: PrincipalId,
    reviewed: bool,
}

impl QualificationObservation {
    /// Creates a signed qualification observation.
    ///
    /// The signature is data for H4 verification; this crate neither verifies cryptography nor
    /// trusts a signer implicitly. Integration must admit authenticated observations from the
    /// configured signer registry before calling this constructor.
    ///
    /// # Errors
    ///
    /// Returns a typed error for a placeholder report or signature digest.
    pub fn new(
        slice: QualificationSlice,
        binding: EvidenceBinding,
        verdict: QualificationVerdict,
        report_digest: Sha256Digest,
        signature_digest: Sha256Digest,
        signer: PrincipalId,
        reviewed: bool,
    ) -> Result<Self, ConstructionError> {
        crate::candidate::require_digest(report_digest)?;
        crate::candidate::require_digest(signature_digest)?;
        Ok(Self { slice, binding, verdict, report_digest, signature_digest, signer, reviewed })
    }

    /// Returns the H-slice identity.
    #[must_use]
    pub const fn slice(&self) -> (slice: QualificationSlice)
        ensures slice == self.spec_slice()
    {
        self.slice
    }

    /// Returns the exact candidate/time/sequence/revision binding.
    #[must_use]
    pub const fn binding(&self) -> (binding: EvidenceBinding)
        ensures binding == self.spec_binding()
    {
        self.binding
    }

    /// Returns the slice's fail-closed verdict.
    #[must_use]
    pub const fn verdict(&self) -> (verdict: QualificationVerdict)
        ensures verdict == self.spec_verdict()
    {
        self.verdict
    }

    /// Returns the canonical report digest.
    #[must_use]
    pub const fn report_digest(&self) -> Sha256Digest { self.report_digest }

    /// Returns the detached signature digest.
    #[must_use]
    pub const fn signature_digest(&self) -> Sha256Digest { self.signature_digest }

    /// Returns the admitted signer identity.
    #[must_use]
    pub const fn signer(&self) -> PrincipalId { self.signer }

    /// Returns whether independent review was completed.
    #[must_use]
    pub const fn reviewed(&self) -> (reviewed: bool)
        ensures reviewed == self.spec_reviewed()
    {
        self.reviewed
    }

    /// Logical view of the H-slice identity.
    pub closed spec fn spec_slice(&self) -> QualificationSlice { self.slice }

    /// Logical view of the exact candidate, time, sequence, and revision binding.
    pub closed spec fn spec_binding(&self) -> EvidenceBinding { self.binding }

    /// Logical view of the slice's fail-closed verdict.
    pub closed spec fn spec_verdict(&self) -> QualificationVerdict { self.verdict }

    /// Logical view of whether independent review was completed.
    pub closed spec fn spec_reviewed(&self) -> bool { self.reviewed }

    /// Specification predicate for a qualification permitted to satisfy H4.
    pub open spec fn spec_contributes_to(
        &self,
        slice: QualificationSlice,
        candidate: ReleaseCandidate,
        evaluated_at: u64,
    ) -> bool {
        self.spec_slice() == slice
            && self.spec_binding().spec_is_current_for(candidate, evaluated_at)
            && self.spec_verdict() == QualificationVerdict::Ready
            && self.spec_reviewed()
    }

    /// Proves that stale or mismatched qualification input cannot satisfy H4.
    pub proof fn noncurrent_qualification_cannot_contribute(
        &self,
        slice: QualificationSlice,
        candidate: ReleaseCandidate,
        evaluated_at: u64,
    )
        requires !self.spec_binding().spec_is_current_for(candidate, evaluated_at),
        ensures !self.spec_contributes_to(slice, candidate, evaluated_at),
    {
        reveal(QualificationObservation::spec_contributes_to);
    }
}

} // verus!
