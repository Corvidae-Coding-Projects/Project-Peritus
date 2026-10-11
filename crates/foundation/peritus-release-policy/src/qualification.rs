//! Signed H0-H3 qualification observations used as verified policy inputs.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use crate::{ConstructionError, EvidenceBinding, PrincipalId};

use peritus_types::Sha256Digest;

#[doc = " Required upstream qualification slice."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum QualificationSlice {
    #[doc = " H0 security qualification."]
    H0Security = 0,
    #[doc = " H1 resilience qualification."]
    H1Resilience = 1,
    #[doc = " H2 platform qualification."]
    H2Platform = 2,
    #[doc = " H3 performance qualification."]
    H3Performance = 3,
}
impl QualificationSlice {
    #[doc = " Canonical H0-H3 order."]
    pub const ALL: [Self; 4] =
        [Self::H0Security, Self::H1Resilience, Self::H2Platform, Self::H3Performance];
    #[doc = " Returns the stable zero-based H-slice ordinal."]
    #[must_use]
    pub const fn ordinal(self) -> u8 {
        self as u8
    }
}
#[doc = " Fail-closed upstream qualification status."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum QualificationVerdict {
    #[doc = " The slice\'s own policy reported ready for this exact candidate."]
    Ready,
    #[doc = " The slice did not report ready for this exact candidate."]
    NotReadyForProduction,
}
#[doc = " Signed, independently reviewed H0/H1/H2/H3 report observation."]
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
    #[doc = " Creates a signed qualification observation."]
    #[doc = ""]
    #[doc = " The signature is data for H4 verification; this crate neither verifies cryptography nor"]
    #[doc = " trusts a signer implicitly. Integration must admit authenticated observations from the"]
    #[doc = " configured signer registry before calling this constructor."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a typed error for a placeholder report or signature digest."]
    pub fn new(
        slice: QualificationSlice,
        binding: EvidenceBinding,
        verdict: QualificationVerdict,
        report_digest: Sha256Digest,
        signature_digest: Sha256Digest,
        signer: PrincipalId,
        reviewed: bool,
    ) -> Result<Self, ConstructionError> {
        crate::validation::require_digest(report_digest)?;
        crate::validation::require_digest(signature_digest)?;
        Ok(Self { slice, binding, verdict, report_digest, signature_digest, signer, reviewed })
    }
    #[doc = " Returns the H-slice identity."]
    #[must_use]
    pub const fn slice(&self) -> QualificationSlice {
        self.slice
    }
    #[doc = " Returns the exact candidate/time/sequence/revision binding."]
    #[must_use]
    pub const fn binding(&self) -> EvidenceBinding {
        self.binding
    }
    #[doc = " Returns the slice\'s fail-closed verdict."]
    #[must_use]
    pub const fn verdict(&self) -> QualificationVerdict {
        self.verdict
    }
    #[doc = " Returns the canonical report digest."]
    #[must_use]
    pub const fn report_digest(&self) -> Sha256Digest {
        self.report_digest
    }
    #[doc = " Returns the detached signature digest."]
    #[must_use]
    pub const fn signature_digest(&self) -> Sha256Digest {
        self.signature_digest
    }
    #[doc = " Returns the admitted signer identity."]
    #[must_use]
    pub const fn signer(&self) -> PrincipalId {
        self.signer
    }
    #[doc = " Returns whether independent review was completed."]
    #[must_use]
    pub const fn reviewed(&self) -> bool {
        self.reviewed
    }
}
