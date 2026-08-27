//! Typed canonical-evidence construction failures.

use vstd::prelude::*;

verus! {

/// Canonical evidence collection containing an invalid value.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum EvidenceCollection {
    /// R-SEC requirement observations.
    Requirements,
    /// Numbered acceptance-criterion observations.
    Criteria,
    /// Threat, control, unsafe, and TCB inventories.
    Inventories,
    /// Canonical manifest artifacts.
    Artifacts,
    /// Findings inside the external review.
    Findings,
    /// Declared independently reviewed security scopes.
    ReviewScopes,
}

/// Stable malformed-evidence category.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum EvidenceErrorKind {
    /// Values were not supplied in ascending canonical order.
    NonCanonicalOrder,
    /// A stable identity occurred more than once.
    DuplicateObservation,
    /// A nested finding was bound to a different candidate than its review.
    NestedCandidateMismatch,
}

/// Checked-construction failure with deterministic location.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EvidenceError {
    kind: EvidenceErrorKind,
    collection: EvidenceCollection,
    index: usize,
}

impl EvidenceError {
    pub(crate) const fn new(
        kind: EvidenceErrorKind,
        collection: EvidenceCollection,
        index: usize,
    ) -> Self {
        Self { kind, collection, index }
    }

    /// Returns the stable error category.
    #[must_use]
    pub const fn kind(self) -> EvidenceErrorKind { self.kind }

    /// Returns the invalid collection.
    #[must_use]
    pub const fn collection(self) -> EvidenceCollection { self.collection }

    /// Returns the zero-based first offending position.
    #[must_use]
    pub const fn index(self) -> usize { self.index }
}

} // verus!
