//! Bounded observations for canonical H4 release evidence.

mod aggregate;
mod observation;

pub use self::aggregate::ReleaseEvidence;
pub use self::observation::EvidenceObservation;

use crate::{ConstructionError, ConstructionErrorKind, ReleaseCandidate};

#[doc = " Exact candidate, time, sequence, and source-revision binding for one observation."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EvidenceBinding {
    candidate: ReleaseCandidate,
    observed_at: u64,
    expires_at: u64,
    sequence: u64,
    source_revision: u64,
}
impl EvidenceBinding {
    #[doc = " Creates a checked evidence binding."]
    #[doc = ""]
    #[doc = " Times are monotonic release-clock ticks. They are not interpreted as wall-clock time by the"]
    #[doc = " policy. The source revision must equal [`ReleaseCandidate::source_revision`] to contribute."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a typed error for zero sequence/revision or an inverted validity interval."]
    pub const fn new(
        candidate: ReleaseCandidate,
        observed_at: u64,
        expires_at: u64,
        sequence: u64,
        source_revision: u64,
    ) -> Result<Self, ConstructionError> {
        if sequence == 0 || source_revision == 0 {
            return Err(ConstructionError::new(ConstructionErrorKind::ZeroRevision));
        }
        if expires_at < observed_at {
            return Err(ConstructionError::new(ConstructionErrorKind::InvalidValidityInterval));
        }
        Ok(Self { candidate, observed_at, expires_at, sequence, source_revision })
    }
    #[doc = " Returns the exact candidate named by the observation."]
    #[must_use]
    pub const fn candidate(&self) -> ReleaseCandidate {
        self.candidate
    }
    #[doc = " Returns the monotonic observation tick."]
    #[must_use]
    pub const fn observed_at(&self) -> u64 {
        self.observed_at
    }
    #[doc = " Returns the inclusive expiration tick."]
    #[must_use]
    pub const fn expires_at(&self) -> u64 {
        self.expires_at
    }
    #[doc = " Returns the positive source sequence."]
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
    #[doc = " Returns the positive producing source revision."]
    #[must_use]
    pub const fn source_revision(&self) -> u64 {
        self.source_revision
    }
    #[doc = " Returns whether this binding is exact and current for an evaluation."]
    #[must_use]
    pub const fn is_current_for(&self, candidate: ReleaseCandidate, evaluated_at: u64) -> bool {
        crate::candidate::candidate_matches(&self.candidate(), &candidate)
            && self.source_revision() == candidate.source_revision()
            && self.observed_at() <= evaluated_at
            && evaluated_at <= self.expires_at()
    }
    #[doc = " Returns whether candidate or producing-revision identity differs."]
    #[must_use]
    pub const fn is_mismatched(&self, candidate: ReleaseCandidate) -> bool {
        !crate::candidate::candidate_matches(&self.candidate(), &candidate)
            || self.source_revision() != candidate.source_revision()
    }
    #[doc = " Returns whether only the time window makes the otherwise exact binding stale."]
    #[must_use]
    pub const fn is_stale_at(&self, candidate: ReleaseCandidate, evaluated_at: u64) -> bool {
        crate::candidate::candidate_matches(&self.candidate(), &candidate)
            && self.source_revision() == candidate.source_revision()
            && (evaluated_at < self.observed_at || evaluated_at > self.expires_at)
    }
}
#[doc = " Executable equality of every field in two evidence bindings."]
pub const fn bindings_equal(left: &EvidenceBinding, right: &EvidenceBinding) -> bool {
    crate::candidate::candidate_matches(&left.candidate(), &right.candidate())
        && left.observed_at() == right.observed_at()
        && left.expires_at() == right.expires_at()
        && left.sequence() == right.sequence()
        && left.source_revision() == right.source_revision()
}
