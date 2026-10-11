//! Logical observations and explicit immutable memory lifecycle states.

use crate::{Confidence, Feedback, MemoryError, MemoryErrorKind, MemoryField, MemoryId};
use peritus_types::RevisionNumber;

#[doc = " Explicit logical time supplied by the caller; no wall clock is consulted."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Observation {
    epoch: u64,
    tick: u64,
}
impl Observation {
    #[doc = " Creates a logical observation. Epoch zero is reserved as invalid."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`MemoryErrorKind::InvalidBound`] for epoch zero."]
    pub const fn new(epoch: u64, tick: u64) -> Result<Self, MemoryError> {
        if epoch == 0 {
            Err(MemoryError::field(MemoryErrorKind::InvalidBound, MemoryField::Observation))
        } else {
            Ok(Self { epoch, tick })
        }
    }
    #[doc = " Returns the caller-defined logical epoch."]
    #[must_use]
    pub const fn epoch(self) -> u64 {
        self.epoch
    }
    #[doc = " Returns the monotonic tick within the epoch."]
    #[must_use]
    pub const fn tick(self) -> u64 {
        self.tick
    }
    pub(crate) fn later_than(self, other: Self) -> bool {
        self > other
    }
}
#[doc = " Reason an active memory was isolated from retrieval."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum QuarantineReason {
    #[doc = " Explicit contradicting evidence crossed policy."]
    Contradiction,
    #[doc = " Negative retrieval feedback crossed policy."]
    NegativeFeedback,
    #[doc = " A review determined that support is insufficient."]
    Unsupported,
    #[doc = " A caller identified suspected memory poisoning."]
    SuspectedPoisoning,
    #[doc = " A human or orchestrator requested bounded investigation."]
    ManualReview,
}
#[doc = " Reason retained by a deletion tombstone without retaining deleted content."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DeletionReason {
    #[doc = " An authenticated user requested forgetting."]
    UserRequest,
    #[doc = " A retention policy required deletion."]
    RetentionPolicy,
    #[doc = " The content was invalid or poisoned."]
    InvalidContent,
    #[doc = " A project or workspace was removed."]
    ScopeRemoved,
}
#[doc = " Retrieval-visible lifecycle state. Forgotten content is represented only by a tombstone."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MemoryState {
    #[doc = " Eligible for policy filtering and ranking."]
    Active,
    #[doc = " Isolated pending a later review."]
    Quarantined,
    #[doc = " Explicitly expired."]
    Expired,
    #[doc = " Replaced by another memory identifier."]
    Superseded,
}
#[doc = " Checked lifecycle metadata embedded in an immutable record revision."]
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
    #[doc = " Creates an initial active snapshot from already checked values."]
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
    #[doc = " Returns the lifecycle state."]
    #[must_use]
    pub const fn state(&self) -> MemoryState {
        self.state
    }
    #[doc = " Returns evidence confidence."]
    #[must_use]
    pub const fn confidence(&self) -> Confidence {
        self.confidence
    }
    #[doc = " Returns explicit feedback."]
    #[must_use]
    pub const fn feedback(&self) -> Feedback {
        self.feedback
    }
    #[doc = " Returns the immutable record revision."]
    #[must_use]
    pub const fn revision(&self) -> RevisionNumber {
        self.revision
    }
    #[doc = " Returns the observation that established the current non-active state."]
    #[must_use]
    pub const fn state_observation(&self) -> Option<Observation> {
        self.state_observation
    }
    #[doc = " Returns the quarantine reason when quarantined."]
    #[must_use]
    pub const fn quarantine_reason(&self) -> Option<QuarantineReason> {
        self.quarantine_reason
    }
    #[doc = " Returns the replacement memory identifier when superseded."]
    #[must_use]
    pub const fn superseded_by(&self) -> Option<MemoryId> {
        self.superseded_by
    }
}
pub const fn revision_advances(old: RevisionNumber, new: RevisionNumber) -> bool {
    new.get() > old.get()
}
