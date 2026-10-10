//! Typed canonical-evidence construction failures.

#[doc = " Canonical evidence collection containing an invalid value."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum EvidenceCollection {
    #[doc = " R-SEC requirement observations."]
    Requirements,
    #[doc = " Numbered acceptance-criterion observations."]
    Criteria,
    #[doc = " Threat, control, unsafe, and TCB inventories."]
    Inventories,
    #[doc = " Canonical manifest artifacts."]
    Artifacts,
    #[doc = " Findings inside the external review."]
    Findings,
    #[doc = " Declared independently reviewed security scopes."]
    ReviewScopes,
}
#[doc = " Stable malformed-evidence category."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum EvidenceErrorKind {
    #[doc = " Values were not supplied in ascending canonical order."]
    NonCanonicalOrder,
    #[doc = " A stable identity occurred more than once."]
    DuplicateObservation,
    #[doc = " A nested finding was bound to a different candidate than its review."]
    NestedCandidateMismatch,
}
#[doc = " Checked-construction failure with deterministic location."]
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
    #[doc = " Returns the stable error category."]
    #[must_use]
    pub const fn kind(self) -> EvidenceErrorKind {
        self.kind
    }
    #[doc = " Returns the invalid collection."]
    #[must_use]
    pub const fn collection(self) -> EvidenceCollection {
        self.collection
    }
    #[doc = " Returns the zero-based first offending position."]
    #[must_use]
    pub const fn index(self) -> usize {
        self.index
    }
}
