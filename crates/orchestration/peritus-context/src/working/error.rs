//! Redaction-safe working-state validation failures.

use vstd::prelude::*;

verus! {
/// Stable structural rejection; errors never contain model text or source bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkingError {
    /// An identity or sequence used its reserved zero value.
    ZeroSequence,
    /// Limits were zero or exceeded the implementation's bounded envelope.
    InvalidLimit,
    /// A collection or content exceeded the configured limit.
    Capacity,
    /// A source range exceeded the exact artifact length.
    SourceRange,
    /// A source handle was reused for different evidence.
    SourceConflict,
    /// A new source did not follow the contiguous committed prefix.
    SourceSequence,
    /// Referenced evidence has not been ingested.
    MissingSource,
    /// Evidence appeared in both support and contradiction sets.
    ConflictingEvidence,
    /// Ordered identifiers were duplicated or not strictly increasing.
    NonCanonicalOrder,
    /// A run, workspace, task, or role boundary was crossed.
    BindingMismatch,
    /// An update was based on another committed revision.
    RevisionMismatch,
    /// Conversation revision moved backwards.
    StaleConversation,
    /// Advancing the revision would overflow.
    RevisionExhausted,
    /// An entry had no source or empty content.
    EmptyEntry,
    /// A dependency or superseded entry was absent.
    MissingEntry,
    /// Dependency or supersession edges formed a cycle.
    DependencyCycle,
    /// An agent proposed a status reserved for host invalidation.
    DerivedStatus,
    /// A superseded identity cannot be resurrected or assigned another successor.
    AlreadySuperseded,
    /// The proposed conclusion depends on stale or superseded material.
    StaleEntry,
}
}

#[cfg(not(verus_only))]
impl core::fmt::Display for WorkingError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "working-state rejection: {self:?}")
    }
}

#[cfg(not(verus_only))]
impl std::error::Error for WorkingError {}
