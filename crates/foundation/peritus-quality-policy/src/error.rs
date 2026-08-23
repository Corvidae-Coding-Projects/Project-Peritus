//! Stable failures returned by checked evidence constructors.

use vstd::prelude::*;

verus! {

/// Canonical collection rejected by evidence validation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CanonicalEvidenceCollection {
    /// Gate observations ordered by gate identity.
    Gates,
    /// Review observations ordered by review-cycle identity and one-based cycle ordinal.
    Reviews,
    /// Categories within one review.
    ReviewCategories,
    /// Findings within one review.
    Findings,
    /// Required artifact observations ordered by requirement identity.
    Evidence,
    /// Human approval observations ordered by request identity.
    Approvals,
    /// Waiver observations ordered by finding identity.
    Waivers,
}

/// Stable category for malformed or contradictory evidence.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EvidenceErrorKind {
    /// Values were not supplied in ascending canonical order.
    NonCanonicalOrder,
    /// A canonical identity occurred more than once.
    DuplicateObservation,
    /// One review did not cover any category.
    EmptyReviewCategories,
    /// A finding resolution was bound to a revision other than its enclosing review.
    ResolutionRevisionMismatch,
    /// Approval observations reused the same semantic subject.
    DuplicateApprovalSubject,
    /// A waiver did not identify an approval for the same finding.
    WaiverApprovalSubjectMismatch,
}

/// Typed checked-construction failure.
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

    /// Returns the stable error category.
    #[must_use]
    pub const fn kind(&self) -> EvidenceErrorKind { self.kind }

    /// Returns the collection in which validation failed.
    #[must_use]
    pub const fn collection(&self) -> CanonicalEvidenceCollection { self.collection }

    /// Returns the zero-based index of the first offending value.
    #[must_use]
    pub const fn index(&self) -> usize { self.index }
}

} // verus!
