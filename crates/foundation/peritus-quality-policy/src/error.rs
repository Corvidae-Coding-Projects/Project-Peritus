//! Stable failures returned by checked evidence constructors.

#[doc = " Canonical collection rejected by evidence validation."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CanonicalEvidenceCollection {
    #[doc = " Gate observations ordered by gate identity."]
    Gates,
    #[doc = " Review observations ordered by review-cycle identity and one-based cycle ordinal."]
    Reviews,
    #[doc = " Categories within one review."]
    ReviewCategories,
    #[doc = " Findings within one review."]
    Findings,
    #[doc = " Required artifact observations ordered by requirement identity."]
    Evidence,
    #[doc = " Human approval observations ordered by request identity."]
    Approvals,
    #[doc = " Waiver observations ordered by finding identity."]
    Waivers,
}
#[doc = " Stable category for malformed or contradictory evidence."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EvidenceErrorKind {
    #[doc = " Values were not supplied in ascending canonical order."]
    NonCanonicalOrder,
    #[doc = " A canonical identity occurred more than once."]
    DuplicateObservation,
    #[doc = " One review did not cover any category."]
    EmptyReviewCategories,
    #[doc = " A finding resolution was bound to a revision other than its enclosing review."]
    ResolutionRevisionMismatch,
    #[doc = " Approval observations reused the same semantic subject."]
    DuplicateApprovalSubject,
    #[doc = " A waiver did not identify an approval for the same finding."]
    WaiverApprovalSubjectMismatch,
}
#[doc = " Typed checked-construction failure."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EvidenceError {
    kind: EvidenceErrorKind,
    collection: CanonicalEvidenceCollection,
    index: usize,
}
impl EvidenceError {
    pub(crate) const fn new(
        kind: EvidenceErrorKind,
        collection: CanonicalEvidenceCollection,
        index: usize,
    ) -> Self {
        Self { kind, collection, index }
    }
    #[doc = " Returns the stable error category."]
    #[must_use]
    pub const fn kind(&self) -> EvidenceErrorKind {
        self.kind
    }
    #[doc = " Returns the collection in which validation failed."]
    #[must_use]
    pub const fn collection(&self) -> CanonicalEvidenceCollection {
        self.collection
    }
    #[doc = " Returns the zero-based index of the first offending value."]
    #[must_use]
    pub const fn index(&self) -> usize {
        self.index
    }
}
