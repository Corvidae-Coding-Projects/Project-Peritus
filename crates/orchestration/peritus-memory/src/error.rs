//! Stable, actionable memory validation and planning failures.

use crate::{FeatureKey, MemoryId, MemoryState};

#[doc = " Stable category for a memory failure."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MemoryErrorKind {
    #[doc = " A required collection or byte payload is empty."]
    EmptyValue,
    #[doc = " A caller-supplied bound is outside its supported interval."]
    InvalidBound,
    #[doc = " A stable identifier is the forbidden all-zero value."]
    ZeroIdentifier,
    #[doc = " A collection exceeds its explicit production limit."]
    LimitExceeded,
    #[doc = " Supplied content does not match its SHA-256 digest."]
    DigestMismatch,
    #[doc = " Values are not in canonical strictly increasing order."]
    NonCanonicalOrder,
    #[doc = " A canonical set contains a duplicate value."]
    DuplicateValue,
    #[doc = " Supporting and contradicting evidence overlap."]
    ConflictingEvidence,
    #[doc = " A scope kind is missing its corresponding dimension."]
    IncompleteScope,
    #[doc = " An observation precedes an observation it must follow."]
    StaleObservation,
    #[doc = " An expiry observation precedes creation."]
    ExpiryBeforeCreation,
    #[doc = " A revision is zero, stale, skipped, or overflows."]
    InvalidRevision,
    #[doc = " The requested lifecycle transition is not legal from the current state."]
    InvalidTransition,
    #[doc = " A quarantine release lacks a later review."]
    ReleaseRequiresReview,
    #[doc = " Canonical replay contains conflicting revisions for one memory."]
    ConflictingRevision,
    #[doc = " A tombstone does not bind the record digest it claims to delete."]
    TombstoneDigestMismatch,
    #[doc = " Checked token or score arithmetic overflowed."]
    ArithmeticOverflow,
}
#[doc = " Stable field associated with a memory failure."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MemoryField {
    #[doc = " Stable memory identifier."]
    MemoryId,
    #[doc = " Stable repository identifier."]
    RepositoryId,
    #[doc = " Stable feature key."]
    FeatureKey,
    #[doc = " Source journal events."]
    SourceEvents,
    #[doc = " Supporting evidence."]
    SupportingEvidence,
    #[doc = " Contradicting evidence."]
    ContradictingEvidence,
    #[doc = " Retrieval features."]
    Features,
    #[doc = " Memory content."]
    Content,
    #[doc = " Memory scope."]
    Scope,
    #[doc = " Confidence or another basis-point value."]
    Score,
    #[doc = " Logical observation."]
    Observation,
    #[doc = " Expiry observation."]
    Expiry,
    #[doc = " Immutable record revision."]
    Revision,
    #[doc = " Lifecycle state."]
    Lifecycle,
    #[doc = " Retrieval token budget."]
    TokenBudget,
    #[doc = " Retrieval result limit."]
    ResultLimit,
    #[doc = " Tombstone sequence."]
    Tombstones,
    #[doc = " Record replay sequence."]
    Records,
}
#[doc = " Comparable structured error returned by all checked memory APIs."]
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
    #[doc = " Returns the stable failure category."]
    #[must_use]
    pub const fn kind(&self) -> MemoryErrorKind {
        self.kind
    }
    #[doc = " Returns the field whose invariant failed."]
    #[must_use]
    pub const fn field_value(&self) -> MemoryField {
        self.field
    }
    #[doc = " Returns the affected memory identifier, when known."]
    #[must_use]
    pub const fn memory_id(&self) -> Option<MemoryId> {
        self.memory_id
    }
    #[doc = " Returns the affected feature key, when known."]
    #[must_use]
    pub const fn feature_key(&self) -> Option<FeatureKey> {
        self.feature_key
    }
    #[doc = " Returns the state from which an illegal transition was requested, when applicable."]
    #[must_use]
    pub const fn state(&self) -> Option<MemoryState> {
        self.state
    }
}
