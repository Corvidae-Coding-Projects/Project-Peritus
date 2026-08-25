//! Checked immutable lifecycle transitions for memory records.

#![allow(clippy::collapsible_if, reason = "the pinned Verus frontend lacks Rust let-chains")]

use super::{MemoryEvidence, MemoryRecord, MemoryTiming, reject_overlap};
use crate::lifecycle::revision_advances;
use crate::{
    Confidence, DeletionReason, EvidenceSet, Feedback, MemoryError, MemoryErrorKind, MemoryField,
    MemoryId, MemoryState, MemoryTombstone, Observation, QuarantineReason, StateSnapshot,
};
use peritus_types::RevisionNumber;
use vstd::prelude::*;

verus! {

impl MemoryRecord {
    /// Returns a reviewed revision. Quarantined memories remain quarantined until explicit release.
    ///
    /// # Errors
    ///
    /// Rejects non-advancing revisions/observations, evidence conflict, or terminal states.
    pub fn review(
        &self,
        revision: RevisionNumber,
        observation: Observation,
        confidence: Confidence,
        supporting: EvidenceSet,
        contradicting: EvidenceSet,
        feedback: Feedback,
    ) -> Result<Self, MemoryError> {
        if matches!(self.lifecycle.state(), MemoryState::Expired | MemoryState::Superseded) {
            return Err(MemoryError::transition(self.id, self.lifecycle.state()));
        }
        self.check_advance(revision, observation)?;
        reject_overlap(supporting.values(), contradicting.values())?;
        let evidence = MemoryEvidence {
            source_events: self.evidence.source_events.clone(),
            supporting,
            contradicting,
        };
        let timing = MemoryTiming {
            created: self.timing.created,
            reviewed: Some(observation),
            expires: self.timing.expires,
        };
        let lifecycle = StateSnapshot::revised(
            self.lifecycle.state(),
            confidence,
            feedback,
            revision,
            self.lifecycle.state_observation(),
            self.lifecycle.quarantine_reason(),
            self.lifecycle.superseded_by(),
        );
        Ok(self.revised(evidence, timing, lifecycle))
    }

    /// Quarantines an active memory in a new immutable revision.
    ///
    /// # Errors
    ///
    /// Rejects non-active state or non-advancing revision/observation.
    pub fn quarantine(
        &self,
        revision: RevisionNumber,
        observation: Observation,
        reason: QuarantineReason,
    ) -> Result<Self, MemoryError> {
        if self.lifecycle.state() != MemoryState::Active {
            return Err(MemoryError::transition(self.id, self.lifecycle.state()));
        }
        self.check_advance(revision, observation)?;
        let lifecycle = StateSnapshot::revised(
            MemoryState::Quarantined,
            self.lifecycle.confidence(),
            self.lifecycle.feedback(),
            revision,
            Some(observation),
            Some(reason),
            None,
        );
        Ok(self.revised(self.evidence.clone(), self.timing, lifecycle))
    }

    /// Releases quarantine only through a later review observation and revision.
    ///
    /// # Errors
    ///
    /// Rejects non-quarantined state or a stale review/revision.
    pub fn release(
        &self,
        revision: RevisionNumber,
        review_observation: Observation,
        confidence: Confidence,
        feedback: Feedback,
    ) -> Result<Self, MemoryError> {
        if self.lifecycle.state() != MemoryState::Quarantined {
            return Err(MemoryError::transition(self.id, self.lifecycle.state()));
        }
        if !revision_advances(self.revision(), revision) {
            return Err(MemoryError::memory(
                MemoryErrorKind::InvalidRevision,
                MemoryField::Revision,
                self.id,
            ));
        }
        if !review_observation.later_than(self.latest_observation()) {
            return Err(MemoryError::memory(
                MemoryErrorKind::ReleaseRequiresReview,
                MemoryField::Observation,
                self.id,
            ));
        }
        let timing = MemoryTiming {
            created: self.timing.created,
            reviewed: Some(review_observation),
            expires: self.timing.expires,
        };
        let lifecycle = StateSnapshot::revised(
            MemoryState::Active,
            confidence,
            feedback,
            revision,
            None,
            None,
            None,
        );
        Ok(self.revised(self.evidence.clone(), timing, lifecycle))
    }

    /// Marks an active memory expired at a later observation.
    ///
    /// # Errors
    ///
    /// Rejects non-active state or non-advancing revision/observation.
    pub fn expire(
        &self,
        revision: RevisionNumber,
        observation: Observation,
    ) -> Result<Self, MemoryError> {
        self.transition_from_active(revision, observation, MemoryState::Expired, None)
    }

    /// Supersedes an active memory with a distinct stable identifier.
    ///
    /// # Errors
    ///
    /// Rejects self-supersession, non-active state, or non-advancing revision/observation.
    pub fn supersede(
        &self,
        revision: RevisionNumber,
        observation: Observation,
        replacement: MemoryId,
    ) -> Result<Self, MemoryError> {
        if replacement == self.id {
            return Err(MemoryError::memory(
                MemoryErrorKind::DuplicateValue,
                MemoryField::MemoryId,
                self.id,
            ));
        }
        self.transition_from_active(
            revision,
            observation,
            MemoryState::Superseded,
            Some(replacement),
        )
    }

    /// Forgets any retained state and returns only a dominant deletion tombstone.
    ///
    /// # Errors
    ///
    /// Rejects non-advancing revision or deletion observation.
    pub fn forget(
        &self,
        revision: RevisionNumber,
        observation: Observation,
        reason: DeletionReason,
    ) -> Result<MemoryTombstone, MemoryError> {
        self.check_advance(revision, observation)?;
        Ok(MemoryTombstone::new(self.id, revision, observation, reason, self.content_digest()))
    }

    fn check_advance(
        &self,
        revision: RevisionNumber,
        observation: Observation,
    ) -> Result<(), MemoryError> {
        if !revision_advances(self.revision(), revision) {
            return Err(MemoryError::memory(
                MemoryErrorKind::InvalidRevision,
                MemoryField::Revision,
                self.id,
            ));
        }
        if !observation.later_than(self.latest_observation()) {
            return Err(MemoryError::memory(
                MemoryErrorKind::StaleObservation,
                MemoryField::Observation,
                self.id,
            ));
        }
        Ok(())
    }

    fn transition_from_active(
        &self,
        revision: RevisionNumber,
        observation: Observation,
        state: MemoryState,
        replacement: Option<MemoryId>,
    ) -> Result<Self, MemoryError> {
        if self.lifecycle.state() != MemoryState::Active {
            return Err(MemoryError::transition(self.id, self.lifecycle.state()));
        }
        self.check_advance(revision, observation)?;
        let lifecycle = StateSnapshot::revised(
            state,
            self.lifecycle.confidence(),
            self.lifecycle.feedback(),
            revision,
            Some(observation),
            None,
            replacement,
        );
        Ok(self.revised(self.evidence.clone(), self.timing, lifecycle))
    }

    fn revised(
        &self,
        evidence: MemoryEvidence,
        timing: MemoryTiming,
        lifecycle: StateSnapshot,
    ) -> Self {
        Self {
            id: self.id,
            scope: self.scope,
            material: self.material.clone(),
            evidence,
            timing,
            features: self.features.clone(),
            lifecycle,
        }
    }
}

} // verus!
