//! Stable, actionable memory validation and planning failures.

use crate::{FeatureKey, MemoryId, MemoryState};
use vstd::prelude::*;

verus! {

/// Stable category for a memory failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MemoryErrorKind {
    /// A required collection or byte payload is empty.
    EmptyValue,
    /// A caller-supplied bound is outside its supported interval.
    InvalidBound,
    /// A stable identifier is the forbidden all-zero value.
    ZeroIdentifier,
    /// A collection exceeds its explicit production limit.
    LimitExceeded,
    /// Supplied content does not match its SHA-256 digest.
    DigestMismatch,
    /// Values are not in canonical strictly increasing order.
    NonCanonicalOrder,
    /// A canonical set contains a duplicate value.
    DuplicateValue,
    /// Supporting and contradicting evidence overlap.
    ConflictingEvidence,
    /// A scope kind is missing its corresponding dimension.
    IncompleteScope,
    /// An observation precedes an observation it must follow.
    StaleObservation,
    /// An expiry observation precedes creation.
    ExpiryBeforeCreation,
    /// A revision is zero, stale, skipped, or overflows.
    InvalidRevision,
    /// The requested lifecycle transition is not legal from the current state.
    InvalidTransition,
    /// A quarantine release lacks a later review.
    ReleaseRequiresReview,
    /// Canonical replay contains conflicting revisions for one memory.
    ConflictingRevision,
    /// A tombstone does not bind the record digest it claims to delete.
    TombstoneDigestMismatch,
    /// Checked token or score arithmetic overflowed.
    ArithmeticOverflow,
}

/// Stable field associated with a memory failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MemoryField {
    /// Stable memory identifier.
    MemoryId,
    /// Stable repository identifier.
    RepositoryId,
    /// Stable feature key.
    FeatureKey,
    /// Source journal events.
    SourceEvents,
    /// Supporting evidence.
    SupportingEvidence,
    /// Contradicting evidence.
    ContradictingEvidence,
    /// Retrieval features.
    Features,
    /// Memory content.
    Content,
    /// Memory scope.
    Scope,
    /// Confidence or another basis-point value.
    Score,
    /// Logical observation.
    Observation,
    /// Expiry observation.
    Expiry,
    /// Immutable record revision.
    Revision,
    /// Lifecycle state.
    Lifecycle,
    /// Retrieval token budget.
    TokenBudget,
    /// Retrieval result limit.
    ResultLimit,
    /// Tombstone sequence.
    Tombstones,
    /// Record replay sequence.
    Records,
}

/// Comparable structured error returned by all checked memory APIs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryError {
    kind: MemoryErrorKind,
    field: MemoryField,
    memory_id: Option<MemoryId>,
    feature_key: Option<FeatureKey>,
    state: Option<MemoryState>,
}

impl MemoryError {
    pub(crate) const fn field(kind: MemoryErrorKind, field: MemoryField) -> Self {
        Self { kind, field, memory_id: None, feature_key: None, state: None }
    }

    pub(crate) const fn memory(
        kind: MemoryErrorKind,
        field: MemoryField,
        memory_id: MemoryId,
    ) -> Self {
        Self { kind, field, memory_id: Some(memory_id), feature_key: None, state: None }
    }

    pub(crate) const fn feature(kind: MemoryErrorKind, feature_key: FeatureKey) -> Self {
        Self {
            kind,
            field: MemoryField::Features,
            memory_id: None,
            feature_key: Some(feature_key),
            state: None,
        }
    }

    pub(crate) const fn transition(memory_id: MemoryId, state: MemoryState) -> Self {
        Self {
            kind: MemoryErrorKind::InvalidTransition,
            field: MemoryField::Lifecycle,
            memory_id: Some(memory_id),
            feature_key: None,
            state: Some(state),
        }
    }

    /// Returns the stable failure category.
    #[must_use]
    pub const fn kind(&self) -> MemoryErrorKind { self.kind }

    /// Returns the field whose invariant failed.
    #[must_use]
    pub const fn field_value(&self) -> MemoryField { self.field }

    /// Returns the affected memory identifier, when known.
    #[must_use]
    pub const fn memory_id(&self) -> Option<MemoryId> { self.memory_id }

    /// Returns the affected feature key, when known.
    #[must_use]
    pub const fn feature_key(&self) -> Option<FeatureKey> { self.feature_key }

    /// Returns the state from which an illegal transition was requested, when applicable.
    #[must_use]
    pub const fn state(&self) -> Option<MemoryState> { self.state }
}

} // verus!
