//! Logical observations and explicit immutable memory lifecycle states.

use crate::{Confidence, Feedback, MemoryError, MemoryErrorKind, MemoryField, MemoryId};
use peritus_types::RevisionNumber;
use vstd::prelude::*;

verus! {

/// Explicit logical time supplied by the caller; no wall clock is consulted.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Observation {
    epoch: u64,
    tick: u64,
}

impl Observation {
    /// Creates a logical observation. Epoch zero is reserved as invalid.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryErrorKind::InvalidBound`] for epoch zero.
    pub const fn new(epoch: u64, tick: u64) -> Result<Self, MemoryError> {
        if epoch == 0 {
            Err(MemoryError::field(MemoryErrorKind::InvalidBound, MemoryField::Observation))
        } else {
            Ok(Self { epoch, tick })
        }
    }

    /// Returns the caller-defined logical epoch.
    #[must_use]
    pub const fn epoch(self) -> u64 { self.epoch }

    /// Returns the monotonic tick within the epoch.
    #[must_use]
    pub const fn tick(self) -> u64 { self.tick }

    pub(crate) fn later_than(self, other: Self) -> bool { self > other }
}

/// Reason an active memory was isolated from retrieval.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum QuarantineReason {
    /// Explicit contradicting evidence crossed policy.
    Contradiction,
    /// Negative retrieval feedback crossed policy.
    NegativeFeedback,
    /// A review determined that support is insufficient.
    Unsupported,
    /// A caller identified suspected memory poisoning.
    SuspectedPoisoning,
    /// A human or orchestrator requested bounded investigation.
    ManualReview,
}

/// Reason retained by a deletion tombstone without retaining deleted content.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DeletionReason {
    /// An authenticated user requested forgetting.
    UserRequest,
    /// A retention policy required deletion.
    RetentionPolicy,
    /// The content was invalid or poisoned.
    InvalidContent,
    /// A project or workspace was removed.
    ScopeRemoved,
}

/// Retrieval-visible lifecycle state. Forgotten content is represented only by a tombstone.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MemoryState {
    /// Eligible for policy filtering and ranking.
    Active,
    /// Isolated pending a later review.
    Quarantined,
    /// Explicitly expired.
    Expired,
    /// Replaced by another memory identifier.
    Superseded,
}

/// Checked lifecycle metadata embedded in an immutable record revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StateSnapshot {
    state: MemoryState,
    confidence: Confidence,
    feedback: Feedback,
    revision: RevisionNumber,
    state_observation: Option<Observation>,
    quarantine_reason: Option<QuarantineReason>,
    superseded_by: Option<MemoryId>,
}

impl StateSnapshot {
    /// Creates an initial active snapshot from already checked values.
    #[must_use]
    pub const fn active(
        confidence: Confidence,
        feedback: Feedback,
        revision: RevisionNumber,
    ) -> Self {
        Self {
            state: MemoryState::Active,
            confidence,
            feedback,
            revision,
            state_observation: None,
            quarantine_reason: None,
            superseded_by: None,
        }
    }

    pub(crate) const fn revised(
        state: MemoryState,
        confidence: Confidence,
        feedback: Feedback,
        revision: RevisionNumber,
        state_observation: Option<Observation>,
        quarantine_reason: Option<QuarantineReason>,
        superseded_by: Option<MemoryId>,
    ) -> Self {
        Self {
            state,
            confidence,
            feedback,
            revision,
            state_observation,
            quarantine_reason,
            superseded_by,
        }
    }

    /// Returns the lifecycle state.
    #[must_use]
    pub const fn state(&self) -> MemoryState { self.state }

    /// Returns evidence confidence.
    #[must_use]
    pub const fn confidence(&self) -> Confidence { self.confidence }

    /// Returns explicit feedback.
    #[must_use]
    pub const fn feedback(&self) -> Feedback { self.feedback }

    /// Returns the immutable record revision.
    #[must_use]
    pub const fn revision(&self) -> (result: RevisionNumber)
        ensures result.spec_value() == self.spec_revision_value(),
    {
        self.revision
    }

    /// Returns the mathematical immutable revision used by lifecycle specifications.
    pub closed spec fn spec_revision_value(&self) -> int { self.revision.spec_value() }

    /// Returns the observation that established the current non-active state.
    #[must_use]
    pub const fn state_observation(&self) -> Option<Observation> { self.state_observation }

    /// Returns the quarantine reason when quarantined.
    #[must_use]
    pub const fn quarantine_reason(&self) -> Option<QuarantineReason> {
        self.quarantine_reason
    }

    /// Returns the replacement memory identifier when superseded.
    #[must_use]
    pub const fn superseded_by(&self) -> Option<MemoryId> { self.superseded_by }
}

pub const fn revision_advances(old: RevisionNumber, new: RevisionNumber) -> bool {
    new.get() > old.get()
}

} // verus!
