//! Canonical fail-closed diagnostic vocabulary.

use crate::{EvidenceRequirement, QualificationSlice};
use vstd::prelude::*;

verus! {

/// One canonical reason H4 did not return [`crate::ReleaseVerdict::Ready`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Diagnostic {
    /// No contributing observation exists for a requirement.
    MissingEvidence(EvidenceRequirement),
    /// Otherwise matching evidence was outside its validity window.
    StaleEvidence(EvidenceRequirement, u16),
    /// Evidence named another candidate or producing revision.
    MismatchedEvidence(EvidenceRequirement, u16),
    /// Evidence came from the wrong source class.
    WrongEvidenceSource(EvidenceRequirement, u16),
    /// Evidence had not completed independent review.
    UnreviewedEvidence(EvidenceRequirement, u16),
    /// Evidence was not source-authenticated.
    UnsignedEvidence(EvidenceRequirement, u16),
    /// Current contributing observations disagreed.
    ConflictingEvidence(EvidenceRequirement),
    /// No ready signed report exists for a required H-slice.
    MissingQualification(QualificationSlice),
    /// Qualification report was stale.
    StaleQualification(QualificationSlice, u16),
    /// Qualification report named another candidate or source revision.
    MismatchedQualification(QualificationSlice, u16),
    /// Qualification report lacked independent review.
    UnreviewedQualification(QualificationSlice, u16),
    /// Qualification explicitly reported not ready.
    QualificationNotReady(QualificationSlice, u16),
    /// Current qualification reports disagreed.
    ConflictingQualification(QualificationSlice),
    /// Too few clean independent approvals exist.
    ReviewerQuorum {
        /// Required clean independent approvals.
        required: u16,
        /// Observed clean independent approvals.
        observed: u16,
    },
    /// Reviews were stale.
    StaleReviews(u16),
    /// Reviews named another candidate or source revision.
    MismatchedReviews(u16),
    /// Reviews required changes.
    ChangesRequired(u16),
    /// A reviewer was also the candidate producer.
    SelfReview(u16),
    /// A review lacked producer independence.
    NonIndependentReview(u16),
    /// Current reviews reused a reviewer identity.
    DuplicateReviewer,
    /// Current reviews reused a fresh-context digest.
    SharedReviewContext,
    /// Observations with one review identity disagreed.
    ConflictingReview,
    /// Findings or waivers were stale.
    StaleFindingState(u16),
    /// Findings or waivers named another candidate or source revision.
    MismatchedFindingState(u16),
    /// Findings remain unresolved.
    OpenFindings(u16),
    /// Unresolved release-blocking findings remain.
    ReleaseBlockingFindings(u16),
    /// Ignored findings remain.
    IgnoredFindings(u16),
    /// Quarantined findings remain.
    QuarantinedFindings(u16),
    /// A requested or supplied waiver was invalid.
    InvalidWaivers(u16),
    /// Observations with one finding identity disagreed.
    ConflictingFinding,
}

} // verus!
