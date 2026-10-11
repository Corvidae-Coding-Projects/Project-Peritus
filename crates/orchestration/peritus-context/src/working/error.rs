//! Redaction-safe working-state validation failures.

#[doc = " Stable structural rejection; errors never contain model text or source bytes."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkingError {
    #[doc = " An identity or sequence used its reserved zero value."]
    ZeroSequence,
    #[doc = " Limits were zero or exceeded the implementation\'s bounded envelope."]
    InvalidLimit,
    #[doc = " A collection or content exceeded the configured limit."]
    Capacity,
    #[doc = " A source range exceeded the exact artifact length."]
    SourceRange,
    #[doc = " A source handle was reused for different evidence."]
    SourceConflict,
    #[doc = " A new source did not follow the contiguous committed prefix."]
    SourceSequence,
    #[doc = " Referenced evidence has not been ingested."]
    MissingSource,
    #[doc = " Evidence appeared in both support and contradiction sets."]
    ConflictingEvidence,
    #[doc = " Ordered identifiers were duplicated or not strictly increasing."]
    NonCanonicalOrder,
    #[doc = " A run, workspace, task, or role boundary was crossed."]
    BindingMismatch,
    #[doc = " An update was based on another committed revision."]
    RevisionMismatch,
    #[doc = " Conversation revision moved backwards."]
    StaleConversation,
    #[doc = " Advancing the revision would overflow."]
    RevisionExhausted,
    #[doc = " An entry had no source or empty content."]
    EmptyEntry,
    #[doc = " A dependency or superseded entry was absent."]
    MissingEntry,
    #[doc = " Dependency or supersession edges formed a cycle."]
    DependencyCycle,
    #[doc = " An agent proposed a status reserved for host invalidation."]
    DerivedStatus,
    #[doc = " A superseded identity cannot be resurrected or assigned another successor."]
    AlreadySuperseded,
    #[doc = " The proposed conclusion depends on stale or superseded material."]
    StaleEntry,
}
impl WorkingError {}

impl core::fmt::Display for WorkingError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "working-state rejection: {self:?}")
    }
}

impl std::error::Error for WorkingError {}
