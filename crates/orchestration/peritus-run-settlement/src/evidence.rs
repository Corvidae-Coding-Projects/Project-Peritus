//! Candidate-bound qualification evidence.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use crate::CandidateIdentity;
use vstd::prelude::*;

verus! {

/// Minimal fail-closed conclusion shared by gates, obligations, and review.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum QualificationEvidence {
    /// The evidence satisfies its declared qualification obligation.
    Satisfied,
    /// The evidence was observed but does not satisfy its obligation.
    Unsatisfied,
}

impl QualificationEvidence {
    /// Stable protocol tag.
    #[must_use]
    pub const fn tag(self) -> u16 {
        match self {
            Self::Satisfied => 1,
            Self::Unsatisfied => 2,
        }
    }

    /// Decodes a stable protocol tag.
    #[must_use]
    pub const fn from_tag(tag: u16) -> Option<Self> {
        match tag {
            1 => Some(Self::Satisfied),
            2 => Some(Self::Unsatisfied),
            _ => None,
        }
    }

    /// Whether the evidence satisfies its obligation.
    #[must_use]
    pub const fn satisfied(self) -> bool { matches!(self, Self::Satisfied) }
}

/// Typed evidence value and the exact candidate checkpoint that produced it.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EvidenceRecord<T> {
    provenance: CandidateIdentity,
    value: T,
}

impl<T> EvidenceRecord<T> {
    /// Binds a typed evidence value to its exact producing checkpoint.
    #[must_use]
    pub const fn new(provenance: CandidateIdentity, value: T) -> Self {
        Self { provenance, value }
    }

    /// Exact producing checkpoint.
    #[must_use]
    pub const fn provenance(&self) -> &CandidateIdentity { &self.provenance }

    /// Typed evidence payload.
    #[must_use]
    pub const fn value(&self) -> &T { &self.value }
}

/// Freshness and acquisition status of one typed evidence observation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EvidenceStatus<T> {
    /// No evidence has been observed.
    Missing,
    /// Evidence is available and intended to apply to the current candidate.
    Current(EvidenceRecord<T>),
    /// Evidence acquisition completed with a typed negative result.
    Failed(EvidenceRecord<T>),
    /// Evidence is retained for diagnostics but no longer authorizes the candidate.
    Stale(EvidenceRecord<T>),
}

impl<T> EvidenceStatus<T> {
    /// Stable protocol tag.
    #[must_use]
    pub const fn tag(&self) -> u16 {
        match self {
            Self::Missing => 1,
            Self::Current(_) => 2,
            Self::Failed(_) => 3,
            Self::Stale(_) => 4,
        }
    }

    /// Returns the retained record when one exists.
    #[must_use]
    pub const fn record(&self) -> Option<&EvidenceRecord<T>> {
        match self {
            Self::Missing => None,
            Self::Current(record) | Self::Failed(record) | Self::Stale(record) => Some(record),
        }
    }

    /// Whether the status and provenance are current for `candidate`.
    #[must_use]
    pub fn is_current_for(&self, candidate: &CandidateIdentity) -> bool {
        match self {
            Self::Current(record) | Self::Failed(record) => {
                crate::verified::evidence_is_current(
                    record.provenance.same_candidate(candidate),
                    record.provenance.checkpoint_sequence(),
                    candidate.checkpoint_sequence(),
                    true,
                )
            }
            Self::Missing | Self::Stale(_) => false,
        }
    }

    /// Whether a retained stale record no longer binds the current candidate.
    #[must_use]
    pub fn is_validly_stale_for(&self, candidate: &CandidateIdentity) -> bool {
        match self {
            Self::Stale(record) => !record.provenance.same_candidate(candidate),
            _ => true,
        }
    }
}

impl EvidenceStatus<QualificationEvidence> {
    /// Whether current evidence positively satisfies its obligation.
    #[must_use]
    pub fn is_current_and_satisfied(&self, candidate: &CandidateIdentity) -> bool {
        match self {
            Self::Current(record) => {
                crate::verified::evidence_is_current(
                    record.provenance.same_candidate(candidate),
                    record.provenance.checkpoint_sequence(),
                    candidate.checkpoint_sequence(),
                    true,
                )
                    && record.value.satisfied()
            }
            _ => false,
        }
    }
}

} // verus!
