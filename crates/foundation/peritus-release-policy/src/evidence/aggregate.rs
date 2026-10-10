//! Bounded aggregate supplied to the pure H4 release evaluator.

use super::EvidenceObservation;
use crate::{
    ConstructionError, ConstructionErrorKind, FindingObservation, QualificationObservation,
    ReviewObservation, WaiverObservation,
};

#[doc = " Bounded raw evidence supplied to the pure release evaluator."]
#[derive(Debug, Eq, PartialEq)]
pub struct ReleaseEvidence {
    observations: Vec<EvidenceObservation>,
    qualifications: Vec<QualificationObservation>,
    reviews: Vec<ReviewObservation>,
    findings: Vec<FindingObservation>,
    waivers: Vec<WaiverObservation>,
}
impl ReleaseEvidence {
    #[doc = " Maximum observations retained in any one collection."]
    pub const MAX_COLLECTION_LEN: usize = 4_096;
    #[doc = " Creates a bounded release-evidence aggregate."]
    #[doc = ""]
    #[doc = " Input order is deliberately unrestricted. The evaluator projects it into the closed"]
    #[doc = " requirement, H-slice, criterion, review, and finding order before producing a decision."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`ConstructionErrorKind::CollectionLimitExceeded`] when any collection is too large."]
    pub fn new(
        observations: Vec<EvidenceObservation>,
        qualifications: Vec<QualificationObservation>,
        reviews: Vec<ReviewObservation>,
        findings: Vec<FindingObservation>,
        waivers: Vec<WaiverObservation>,
    ) -> Result<Self, ConstructionError> {
        if observations.len() > Self::MAX_COLLECTION_LEN
            || qualifications.len() > Self::MAX_COLLECTION_LEN
            || reviews.len() > Self::MAX_COLLECTION_LEN
            || findings.len() > Self::MAX_COLLECTION_LEN
            || waivers.len() > Self::MAX_COLLECTION_LEN
        {
            return Err(ConstructionError::new(ConstructionErrorKind::CollectionLimitExceeded));
        }
        Ok(Self { observations, qualifications, reviews, findings, waivers })
    }
    #[doc = " Returns raw artifact observations."]
    #[must_use]
    pub const fn observations(&self) -> &[EvidenceObservation] {
        self.observations.as_slice()
    }
    #[doc = " Returns raw signed H0-H3 observations."]
    #[must_use]
    pub const fn qualifications(&self) -> &[QualificationObservation] {
        self.qualifications.as_slice()
    }
    #[doc = " Returns raw independent-review observations."]
    #[must_use]
    pub const fn reviews(&self) -> &[ReviewObservation] {
        self.reviews.as_slice()
    }
    #[doc = " Returns raw finding observations."]
    #[must_use]
    pub const fn findings(&self) -> &[FindingObservation] {
        self.findings.as_slice()
    }
    #[doc = " Returns raw waiver observations."]
    #[must_use]
    pub const fn waivers(&self) -> &[WaiverObservation] {
        self.waivers.as_slice()
    }
}
