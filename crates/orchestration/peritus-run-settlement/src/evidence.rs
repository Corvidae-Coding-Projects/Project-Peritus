//! Candidate-bound qualification evidence.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use crate::CandidateIdentity;

#[doc = " Candidate axes on which one evidence observation actually depends."]
#[doc = ""]
#[doc = " Gate evidence observes content, public requirements, and the host execution context."]
#[doc = " Obligation and review conclusions observe content and public requirements. Repository history"]
#[doc = " fences delivery but does not by itself invalidate those conclusions."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EvidenceDependencies {
    #[doc = " Source content and public requirements."]
    Source,
    #[doc = " Source content, public requirements, and the observed execution context."]
    SourceAndExecution,
}
impl EvidenceDependencies {
    #[doc = " Deterministic gate dependency declaration."]
    pub const GATES: Self = Self::SourceAndExecution;
    #[doc = " Public-obligation dependency declaration."]
    pub const OBLIGATIONS: Self = Self::Source;
    #[doc = " Independent-review dependency declaration."]
    pub const REVIEW: Self = Self::OBLIGATIONS;
    #[doc = " Stable tag for persistence and transport."]
    #[must_use]
    pub const fn tag(self) -> u16 {
        match self {
            Self::Source => 1,
            Self::SourceAndExecution => 2,
        }
    }
    #[doc = " Restores one of the closed dependency declarations."]
    #[must_use]
    pub const fn from_tag(tag: u16) -> Option<Self> {
        match tag {
            1 => Some(Self::Source),
            2 => Some(Self::SourceAndExecution),
            _ => None,
        }
    }
    #[doc = " Whether host execution context is a declared dependency."]
    #[must_use]
    pub const fn execution(self) -> bool {
        matches!(self, Self::SourceAndExecution)
    }
}
#[doc = " Minimal fail-closed conclusion shared by gates, obligations, and review."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum QualificationEvidence {
    #[doc = " The evidence satisfies its declared qualification obligation."]
    Satisfied,
    #[doc = " The evidence was observed but does not satisfy its obligation."]
    Unsatisfied,
}
impl QualificationEvidence {
    #[doc = " Stable protocol tag."]
    #[must_use]
    pub const fn tag(self) -> u16 {
        match self {
            Self::Satisfied => 1,
            Self::Unsatisfied => 2,
        }
    }
    #[doc = " Decodes a stable protocol tag."]
    #[must_use]
    pub const fn from_tag(tag: u16) -> Option<Self> {
        match tag {
            1 => Some(Self::Satisfied),
            2 => Some(Self::Unsatisfied),
            _ => None,
        }
    }
    #[doc = " Whether the evidence satisfies its obligation."]
    #[must_use]
    pub const fn satisfied(self) -> bool {
        matches!(self, Self::Satisfied)
    }
}
#[doc = " Typed evidence value and the exact candidate checkpoint that produced it."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EvidenceRecord<T> {
    provenance: CandidateIdentity,
    dependencies: EvidenceDependencies,
    value: T,
}
impl<T> EvidenceRecord<T> {
    #[doc = " Binds a typed value to its producing checkpoint and declared dependency axes."]
    #[must_use]
    pub const fn new(
        provenance: CandidateIdentity,
        dependencies: EvidenceDependencies,
        value: T,
    ) -> Self {
        Self { provenance, dependencies, value }
    }
    #[doc = " Exact producing checkpoint."]
    #[must_use]
    pub const fn provenance(&self) -> &CandidateIdentity {
        &self.provenance
    }
    #[doc = " Candidate axes required for this observation to remain current."]
    #[must_use]
    pub const fn dependencies(&self) -> EvidenceDependencies {
        self.dependencies
    }
    #[doc = " Typed evidence payload."]
    #[must_use]
    pub const fn value(&self) -> &T {
        &self.value
    }
    #[doc = " Whether every declared dependency axis matches the supplied candidate."]
    #[must_use]
    pub fn binding_matches(&self, candidate: &CandidateIdentity) -> bool {
        self.provenance.matches_evidence_axes(candidate, self.dependencies.execution())
    }
}
#[doc = " Freshness and acquisition status of one typed evidence observation."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EvidenceStatus<T> {
    #[doc = " No evidence has been observed."]
    Missing,
    #[doc = " Evidence is available and intended to apply to the current candidate."]
    Current(EvidenceRecord<T>),
    #[doc = " Evidence acquisition completed with a typed negative result."]
    Failed(EvidenceRecord<T>),
    #[doc = " Evidence is retained for diagnostics but no longer authorizes the candidate."]
    Stale(EvidenceRecord<T>),
}
impl<T> EvidenceStatus<T> {
    #[doc = " Stable protocol tag."]
    #[must_use]
    pub const fn tag(&self) -> u16 {
        match self {
            Self::Missing => 1,
            Self::Current(_) => 2,
            Self::Failed(_) => 3,
            Self::Stale(_) => 4,
        }
    }
    #[doc = " Returns the retained record when one exists."]
    #[must_use]
    pub const fn record(&self) -> Option<&EvidenceRecord<T>> {
        match self {
            Self::Missing => None,
            Self::Current(record) | Self::Failed(record) | Self::Stale(record) => Some(record),
        }
    }
    #[doc = " Whether the status and provenance are current for `candidate`."]
    #[must_use]
    pub fn is_current_for(&self, candidate: &CandidateIdentity) -> bool {
        match self {
            Self::Current(record) | Self::Failed(record) => crate::verified::evidence_is_current(
                record.binding_matches(candidate),
                record.provenance.checkpoint_sequence(),
                candidate.checkpoint_sequence(),
                true,
            ),
            Self::Missing | Self::Stale(_) => false,
        }
    }
    #[doc = " Whether a retained stale record no longer binds the current candidate."]
    #[must_use]
    pub fn is_validly_stale_for(&self, candidate: &CandidateIdentity) -> bool {
        match self {
            Self::Stale(record) => !record.binding_matches(candidate),
            _ => true,
        }
    }
}
impl EvidenceStatus<QualificationEvidence> {
    #[doc = " Reconciles a retained observation against the current snapshot binding."]
    #[doc = ""]
    #[doc = " Provenance and payload are never rewritten. A matching retained observation can apply"]
    #[doc = " again after an exact reversion; a differing binding remains historical evidence."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = " Rejects a matching observation from a future checkpoint sequence."]
    pub fn reconcile_for(
        self,
        candidate: &CandidateIdentity,
    ) -> Result<Self, crate::SettlementError> {
        let record = match self {
            Self::Missing => return Ok(Self::Missing),
            Self::Current(record) | Self::Failed(record) | Self::Stale(record) => record,
        };
        if !record.binding_matches(candidate) {
            return Ok(Self::Stale(record));
        }
        if record.provenance.checkpoint_sequence() > candidate.checkpoint_sequence() {
            return Err(crate::SettlementError::new(
                crate::SettlementErrorKind::CurrentEvidenceBindingMismatch,
            ));
        }
        if record.value.satisfied() { Ok(Self::Current(record)) } else { Ok(Self::Failed(record)) }
    }
    #[doc = " Whether current evidence positively satisfies its obligation."]
    #[must_use]
    pub fn is_current_and_satisfied(&self, candidate: &CandidateIdentity) -> bool {
        match self {
            Self::Current(record) => {
                crate::verified::evidence_is_current(
                    record.binding_matches(candidate),
                    record.provenance.checkpoint_sequence(),
                    candidate.checkpoint_sequence(),
                    true,
                ) && record.value.satisfied()
            }
            _ => false,
        }
    }
}
