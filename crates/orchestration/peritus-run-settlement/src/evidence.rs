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
    pub const fn satisfied(self) -> (satisfied: bool)
        ensures satisfied == (self == Self::Satisfied),
    { matches!(self, Self::Satisfied) }
}

/// Typed evidence value and the exact candidate checkpoint that produced it.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EvidenceRecord<T> {
    provenance: CandidateIdentity,
    value: T,
}

impl<T> EvidenceRecord<T> {
    /// Logical view of the exact producing checkpoint.
    pub closed spec fn spec_provenance(&self) -> CandidateIdentity { self.provenance }
    /// Logical view of the retained evidence value.
    pub closed spec fn spec_value(&self) -> T { self.value }

    /// Binds a typed evidence value to its exact producing checkpoint.
    #[must_use]
    pub const fn new(provenance: CandidateIdentity, value: T) -> (record: Self)
        ensures record.spec_provenance() == provenance, record.spec_value() == value,
    {
        Self { provenance, value }
    }

    /// Exact producing checkpoint.
    #[must_use]
    pub const fn provenance(&self) -> (value: &CandidateIdentity)
        ensures *value == self.spec_provenance(),
    { &self.provenance }

    /// Typed evidence payload.
    #[must_use]
    pub const fn value(&self) -> (value: &T)
        ensures *value == self.spec_value(),
    { &self.value }
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
    /// Current or failed evidence has the exact candidate binding and an earlier or equal sequence.
    pub open spec fn spec_is_current_for(&self, candidate: &CandidateIdentity) -> bool {
        match self {
            Self::Current(record) | Self::Failed(record) =>
                record.spec_provenance().spec_same_candidate(candidate)
                    && record.spec_provenance().spec_checkpoint_sequence()
                        <= candidate.spec_checkpoint_sequence(),
            Self::Missing | Self::Stale(_) => false,
        }
    }

    /// A stale label is consistent only when the retained candidate binding differs.
    pub open spec fn spec_is_validly_stale_for(&self, candidate: &CandidateIdentity) -> bool {
        match self {
            Self::Stale(record) => !record.spec_provenance().spec_same_candidate(candidate),
            _ => true,
        }
    }

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
    pub fn is_current_for(&self, candidate: &CandidateIdentity) -> (current: bool)
        ensures current == self.spec_is_current_for(candidate),
    {
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
    pub fn is_validly_stale_for(&self, candidate: &CandidateIdentity) -> (valid: bool)
        ensures valid == self.spec_is_validly_stale_for(candidate),
    {
        match self {
            Self::Stale(record) => !record.provenance.same_candidate(candidate),
            _ => true,
        }
    }
}

impl EvidenceStatus<QualificationEvidence> {
    /// Only a positive current observation for the exact candidate satisfies qualification.
    pub open spec fn spec_is_current_and_satisfied(&self, candidate: &CandidateIdentity) -> bool {
        match self {
            Self::Current(record) => self.spec_is_current_for(candidate)
                && record.spec_value() == QualificationEvidence::Satisfied,
            _ => false,
        }
    }

    /// Whether current evidence positively satisfies its obligation.
    #[must_use]
    pub fn is_current_and_satisfied(&self, candidate: &CandidateIdentity) -> (satisfied: bool)
        ensures satisfied == self.spec_is_current_and_satisfied(candidate),
    {
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
