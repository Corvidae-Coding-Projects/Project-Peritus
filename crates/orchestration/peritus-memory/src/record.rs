//! Immutable memory records and checked lifecycle revisions.

#![allow(clippy::collapsible_if, reason = "the pinned Verus frontend lacks Rust let-chains")]

use crate::{
    EvidenceSet, MemoryError, MemoryErrorKind, MemoryField, MemoryId, MemoryMaterial, MemoryScope,
    MemoryState, Observation, RetrievalFeatures, SourceEventSet, StateSnapshot,
};
use peritus_types::{EvidenceId, RevisionNumber, Sha256Digest};
use vstd::prelude::*;

mod transitions;

verus! {

/// Canonical evidence bindings for one memory record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryEvidence {
    source_events: SourceEventSet,
    supporting: EvidenceSet,
    contradicting: EvidenceSet,
}

impl MemoryEvidence {
    /// Creates evidence bindings and rejects support/contradiction overlap.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryErrorKind::ConflictingEvidence`] for any identifier in both sets.
    pub fn new(
        source_events: SourceEventSet,
        supporting: EvidenceSet,
        contradicting: EvidenceSet,
    ) -> Result<Self, MemoryError> {
        reject_overlap(supporting.values(), contradicting.values())?;
        Ok(Self { source_events, supporting, contradicting })
    }

    /// Returns canonical source events.
    #[must_use]
    pub const fn source_events(&self) -> &SourceEventSet { &self.source_events }

    /// Returns canonical supporting evidence.
    #[must_use]
    pub const fn supporting(&self) -> &EvidenceSet { &self.supporting }

    /// Returns canonical contradicting evidence.
    #[must_use]
    pub const fn contradicting(&self) -> &EvidenceSet { &self.contradicting }
}

/// Checked creation, review, and optional expiry observations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryTiming {
    created: Observation,
    reviewed: Option<Observation>,
    expires: Option<Observation>,
}

impl MemoryTiming {
    /// Creates checked logical timing metadata.
    ///
    /// # Errors
    ///
    /// Returns a typed error when review or expiry precedes creation.
    pub fn new(
        created: Observation,
        reviewed: Option<Observation>,
        expires: Option<Observation>,
    ) -> Result<Self, MemoryError> {
        if let Some(reviewed_value) = reviewed {
            if reviewed_value < created {
                return Err(MemoryError::field(
                    MemoryErrorKind::StaleObservation,
                    MemoryField::Observation,
                ));
            }
        }
        if let Some(expiry_value) = expires {
            if expiry_value < created {
                return Err(MemoryError::field(
                    MemoryErrorKind::ExpiryBeforeCreation,
                    MemoryField::Expiry,
                ));
            }
        }
        Ok(Self { created, reviewed, expires })
    }

    /// Returns the creation observation.
    #[must_use]
    pub const fn created(&self) -> Observation { self.created }

    /// Returns the latest successful review observation.
    #[must_use]
    pub const fn reviewed(&self) -> Option<Observation> { self.reviewed }

    /// Returns the optional expiry observation.
    #[must_use]
    pub const fn expires(&self) -> Option<Observation> { self.expires }
}

/// Complete immutable scoped derived-memory record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryRecord {
    id: MemoryId,
    scope: MemoryScope,
    material: MemoryMaterial,
    evidence: MemoryEvidence,
    timing: MemoryTiming,
    features: RetrievalFeatures,
    lifecycle: StateSnapshot,
}

impl MemoryRecord {
    /// Creates an initial active memory record from checked value groups.
    ///
    /// # Errors
    ///
    /// Returns a typed error if an imported lifecycle snapshot is not initial active state or if
    /// the review/expiry observations are inconsistent.
    pub fn new(
        id: MemoryId,
        scope: MemoryScope,
        material: MemoryMaterial,
        evidence: MemoryEvidence,
        timing: MemoryTiming,
        features: RetrievalFeatures,
        lifecycle: StateSnapshot,
    ) -> Result<Self, MemoryError> {
        if lifecycle.state() != MemoryState::Active
            || lifecycle.state_observation().is_some()
            || lifecycle.quarantine_reason().is_some()
            || lifecycle.superseded_by().is_some()
        {
            return Err(MemoryError::transition(id, lifecycle.state()));
        }
        Ok(Self { id, scope, material, evidence, timing, features, lifecycle })
    }

    /// Returns the stable memory lineage identifier.
    #[must_use]
    pub const fn id(&self) -> (result: MemoryId)
        ensures result.spec_bytes() == self.spec_id_bytes(),
    {
        self.id
    }

    /// Returns the stable lineage identifier used by specifications.
    pub closed spec fn spec_id_bytes(&self) -> Seq<u8> { self.id.spec_bytes() }

    /// Returns the exact durable scope.
    #[must_use]
    pub const fn scope(&self) -> &MemoryScope { &self.scope }

    /// Returns the typed inert claim material.
    #[must_use]
    pub const fn material(&self) -> &MemoryMaterial { &self.material }

    /// Returns source, supporting, and contradicting evidence.
    #[must_use]
    pub const fn evidence(&self) -> &MemoryEvidence { &self.evidence }

    /// Returns creation, review, and expiry observations.
    #[must_use]
    pub const fn timing(&self) -> &MemoryTiming { &self.timing }

    /// Returns canonical retrieval features.
    #[must_use]
    pub const fn features(&self) -> &RetrievalFeatures { &self.features }

    /// Returns lifecycle metadata.
    #[must_use]
    pub const fn lifecycle(&self) -> &StateSnapshot { &self.lifecycle }

    /// Returns the current immutable record revision.
    #[must_use]
    pub const fn revision(&self) -> (result: RevisionNumber)
        ensures result.spec_value() == self.spec_revision_value(),
    {
        self.lifecycle.revision()
    }

    /// Returns the mathematical immutable revision used by specifications.
    pub closed spec fn spec_revision_value(&self) -> int {
        self.lifecycle.spec_revision_value()
    }

    /// Returns the record content digest.
    #[must_use]
    pub const fn content_digest(&self) -> Sha256Digest { self.material.digest() }

    /// Returns the latest observation represented by this revision.
    #[must_use]
    pub fn latest_observation(&self) -> Observation {
        let mut latest = self.timing.created;
        if let Some(reviewed) = self.timing.reviewed {
            if reviewed > latest {
                latest = reviewed;
            }
        }
        if let Some(state_observation) = self.lifecycle.state_observation() {
            if state_observation > latest {
                latest = state_observation;
            }
        }
        latest
    }

}

fn reject_overlap(
    supporting: &[EvidenceId],
    contradicting: &[EvidenceId],
) -> Result<(), MemoryError> {
    let mut support_index = 0;
    let mut contradiction_index = 0;
    while support_index < supporting.len() && contradiction_index < contradicting.len()
        invariant
            support_index <= supporting.len(),
            contradiction_index <= contradicting.len(),
        decreases supporting.len() - support_index + contradicting.len() - contradiction_index,
    {
        if supporting[support_index] == contradicting[contradiction_index] {
            return Err(MemoryError::field(
                MemoryErrorKind::ConflictingEvidence,
                MemoryField::ContradictingEvidence,
            ));
        }
        if supporting[support_index] < contradicting[contradiction_index] {
            support_index += 1;
        } else {
            contradiction_index += 1;
        }
    }
    Ok(())
}

} // verus!
