//! Bounded aggregate supplied to the pure H4 release evaluator.

use super::EvidenceObservation;
use crate::{
    ConstructionError, ConstructionErrorKind, FindingObservation, QualificationObservation,
    ReviewObservation, WaiverObservation,
};
use vstd::prelude::*;

verus! {

/// Bounded raw evidence supplied to the pure release evaluator.
#[derive(Debug, Eq, PartialEq)]
pub struct ReleaseEvidence {
    observations: Vec<EvidenceObservation>,
    qualifications: Vec<QualificationObservation>,
    reviews: Vec<ReviewObservation>,
    findings: Vec<FindingObservation>,
    waivers: Vec<WaiverObservation>,
}

impl ReleaseEvidence {
    /// Maximum observations retained in any one collection.
    pub const MAX_COLLECTION_LEN: usize = 4_096;

    /// Creates a bounded release-evidence aggregate.
    ///
    /// Input order is deliberately unrestricted. The evaluator projects it into the closed
    /// requirement, H-slice, criterion, review, and finding order before producing a decision.
    ///
    /// # Errors
    ///
    /// Returns [`ConstructionErrorKind::CollectionLimitExceeded`] when any collection is too large.
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
            return Err(ConstructionError::new(
                ConstructionErrorKind::CollectionLimitExceeded,
            ));
        }
        Ok(Self { observations, qualifications, reviews, findings, waivers })
    }

    /// Specification view of artifact observations.
    pub closed spec fn spec_observations(&self) -> Seq<EvidenceObservation> { self.observations@ }

    /// Specification view of H0-H3 qualification observations.
    pub closed spec fn spec_qualifications(&self) -> Seq<QualificationObservation> {
        self.qualifications@
    }

    /// Specification view of independent-review observations.
    pub closed spec fn spec_reviews(&self) -> Seq<ReviewObservation> { self.reviews@ }

    /// Specification view of finding observations.
    pub closed spec fn spec_findings(&self) -> Seq<FindingObservation> { self.findings@ }

    /// Specification view of waiver observations.
    pub closed spec fn spec_waivers(&self) -> Seq<WaiverObservation> { self.waivers@ }

    /// Returns raw artifact observations.
    #[must_use]
    pub const fn observations(&self) -> (result: &[EvidenceObservation])
        ensures result@ == self.spec_observations(),
    {
        self.observations.as_slice()
    }

    /// Returns raw signed H0-H3 observations.
    #[must_use]
    pub const fn qualifications(&self) -> (result: &[QualificationObservation])
        ensures result@ == self.spec_qualifications(),
    {
        self.qualifications.as_slice()
    }

    /// Returns raw independent-review observations.
    #[must_use]
    pub const fn reviews(&self) -> (result: &[ReviewObservation])
        ensures result@ == self.spec_reviews(),
    {
        self.reviews.as_slice()
    }

    /// Returns raw finding observations.
    #[must_use]
    pub const fn findings(&self) -> (result: &[FindingObservation])
        ensures result@ == self.spec_findings(),
    {
        self.findings.as_slice()
    }

    /// Returns raw waiver observations.
    #[must_use]
    pub const fn waivers(&self) -> (result: &[WaiverObservation])
        ensures result@ == self.spec_waivers(),
    {
        self.waivers.as_slice()
    }
}

} // verus!
