//! Canonical fail-closed diagnostic vocabulary.

use crate::{EvidenceRequirement, QualificationSlice};

#[doc = " One canonical reason H4 did not return [`crate::ReleaseVerdict::Ready`]."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Diagnostic {
    #[doc = " No contributing observation exists for a requirement."]
    MissingEvidence(EvidenceRequirement),
    #[doc = " Otherwise matching evidence was outside its validity window."]
    StaleEvidence(EvidenceRequirement, u16),
    #[doc = " Evidence named another candidate or producing revision."]
    MismatchedEvidence(EvidenceRequirement, u16),
    #[doc = " Evidence came from the wrong source class."]
    WrongEvidenceSource(EvidenceRequirement, u16),
    #[doc = " Evidence had not completed independent review."]
    UnreviewedEvidence(EvidenceRequirement, u16),
    #[doc = " Evidence was not source-authenticated."]
    UnsignedEvidence(EvidenceRequirement, u16),
    #[doc = " Current contributing observations disagreed."]
    ConflictingEvidence(EvidenceRequirement),
    #[doc = " No ready signed report exists for a required H-slice."]
    MissingQualification(QualificationSlice),
    #[doc = " Qualification report was stale."]
    StaleQualification(QualificationSlice, u16),
    #[doc = " Qualification report named another candidate or source revision."]
    MismatchedQualification(QualificationSlice, u16),
    #[doc = " Qualification report lacked independent review."]
    UnreviewedQualification(QualificationSlice, u16),
    #[doc = " Qualification explicitly reported not ready."]
    QualificationNotReady(QualificationSlice, u16),
    #[doc = " Current qualification reports disagreed."]
    ConflictingQualification(QualificationSlice),
    #[doc = " Too few clean independent approvals exist."]
    ReviewerQuorum {
        #[doc = " Required clean independent approvals."]
        required: u16,
        #[doc = " Observed clean independent approvals."]
        observed: u16,
    },
    #[doc = " Reviews were stale."]
    StaleReviews(u16),
    #[doc = " Reviews named another candidate or source revision."]
    MismatchedReviews(u16),
    #[doc = " Reviews required changes."]
    ChangesRequired(u16),
    #[doc = " A reviewer was also the candidate producer."]
    SelfReview(u16),
    #[doc = " A review lacked producer independence."]
    NonIndependentReview(u16),
    #[doc = " Current reviews reused a reviewer identity."]
    DuplicateReviewer,
    #[doc = " Current reviews reused a fresh-context digest."]
    SharedReviewContext,
    #[doc = " Observations with one review identity disagreed."]
    ConflictingReview,
    #[doc = " Findings or waivers were stale."]
    StaleFindingState(u16),
    #[doc = " Findings or waivers named another candidate or source revision."]
    MismatchedFindingState(u16),
    #[doc = " Findings remain unresolved."]
    OpenFindings(u16),
    #[doc = " Unresolved release-blocking findings remain."]
    ReleaseBlockingFindings(u16),
    #[doc = " Ignored findings remain."]
    IgnoredFindings(u16),
    #[doc = " Quarantined findings remain."]
    QuarantinedFindings(u16),
    #[doc = " A requested or supplied waiver was invalid."]
    InvalidWaivers(u16),
    #[doc = " Observations with one finding identity disagreed."]
    ConflictingFinding,
}
