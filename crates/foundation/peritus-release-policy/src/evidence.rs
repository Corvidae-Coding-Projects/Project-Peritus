//! Bounded observations for canonical H4 release evidence.

mod aggregate;
mod observation;

pub use self::aggregate::ReleaseEvidence;
pub use self::observation::EvidenceObservation;

use crate::{ConstructionError, ConstructionErrorKind, ReleaseCandidate};
use vstd::prelude::*;

verus! {

/// Exact candidate, time, sequence, and source-revision binding for one observation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EvidenceBinding {
    candidate: ReleaseCandidate,
    observed_at: u64,
    expires_at: u64,
    sequence: u64,
    source_revision: u64,
}

impl EvidenceBinding {
    /// Exact constructor admission over sequence, revision, and validity interval.
    pub open spec fn inputs_valid(
        observed_at: u64,
        expires_at: u64,
        sequence: u64,
        source_revision: u64,
    ) -> bool {
        sequence > 0 && source_revision > 0 && observed_at <= expires_at
    }

    /// Exact failure precedence for rejected binding inputs.
    pub open spec fn construction_error(
        observed_at: u64,
        expires_at: u64,
        sequence: u64,
        source_revision: u64,
        error: ConstructionError,
    ) -> bool {
        if sequence == 0 || source_revision == 0 {
            error.spec_kind() == ConstructionErrorKind::ZeroRevision
        } else {
            observed_at > expires_at
                && error.spec_kind() == ConstructionErrorKind::InvalidValidityInterval
        }
    }

    /// Creates a checked evidence binding.
    ///
    /// Times are monotonic release-clock ticks. They are not interpreted as wall-clock time by the
    /// policy. The source revision must equal [`ReleaseCandidate::source_revision`] to contribute.
    ///
    /// # Errors
    ///
    /// Returns a typed error for zero sequence/revision or an inverted validity interval.
    pub const fn new(
        candidate: ReleaseCandidate,
        observed_at: u64,
        expires_at: u64,
        sequence: u64,
        source_revision: u64,
    ) -> (result: Result<Self, ConstructionError>)
        ensures
            result.is_ok() == Self::inputs_valid(
                observed_at, expires_at, sequence, source_revision),
            match result {
                Ok(value) => value.spec_candidate() == candidate
                    && value.spec_observed_at() == observed_at
                    && value.spec_expires_at() == expires_at
                    && value.spec_sequence() == sequence
                    && value.spec_source_revision() == source_revision,
                Err(error) => Self::construction_error(
                    observed_at, expires_at, sequence, source_revision, error),
            },
    {
        if sequence == 0 || source_revision == 0 {
            return Err(ConstructionError::new(ConstructionErrorKind::ZeroRevision));
        }
        if expires_at < observed_at {
            return Err(ConstructionError::new(
                ConstructionErrorKind::InvalidValidityInterval,
            ));
        }
        Ok(Self { candidate, observed_at, expires_at, sequence, source_revision })
    }

    /// Returns the exact candidate named by the observation.
    #[must_use]
    pub const fn candidate(&self) -> (candidate: ReleaseCandidate)
        ensures candidate == self.spec_candidate()
    {
        self.candidate
    }

    /// Returns the monotonic observation tick.
    #[must_use]
    pub const fn observed_at(&self) -> (observed_at: u64)
        ensures observed_at == self.spec_observed_at()
    {
        self.observed_at
    }

    /// Returns the inclusive expiration tick.
    #[must_use]
    pub const fn expires_at(&self) -> (expires_at: u64)
        ensures expires_at == self.spec_expires_at()
    {
        self.expires_at
    }

    /// Returns the positive source sequence.
    #[must_use]
    pub const fn sequence(&self) -> (sequence: u64)
        ensures sequence == self.spec_sequence()
    {
        self.sequence
    }

    /// Returns the positive producing source revision.
    #[must_use]
    pub const fn source_revision(&self) -> (source_revision: u64)
        ensures source_revision == self.spec_source_revision()
    {
        self.source_revision
    }

    /// Logical view of the exact candidate named by the observation.
    pub closed spec fn spec_candidate(&self) -> ReleaseCandidate { self.candidate }

    /// Logical view of the monotonic observation tick.
    pub closed spec fn spec_observed_at(&self) -> u64 { self.observed_at }

    /// Logical view of the inclusive expiration tick.
    pub closed spec fn spec_expires_at(&self) -> u64 { self.expires_at }

    /// Logical view of the positive source sequence.
    pub closed spec fn spec_sequence(&self) -> u64 { self.sequence }

    /// Logical view of the positive producing source revision.
    pub closed spec fn spec_source_revision(&self) -> u64 { self.source_revision }

    /// Specification predicate for exact-current binding.
    pub open spec fn spec_is_current_for(
        &self,
        candidate: ReleaseCandidate,
        evaluated_at: u64,
    ) -> bool {
        crate::candidate::candidate_matches_exactly(self.spec_candidate(), candidate)
            && self.spec_source_revision() == candidate.spec_source_revision()
            && self.spec_observed_at() <= evaluated_at
            && evaluated_at <= self.spec_expires_at()
    }

    /// Returns whether this binding is exact and current for an evaluation.
    #[must_use]
    pub const fn is_current_for(
        &self,
        candidate: ReleaseCandidate,
        evaluated_at: u64,
    ) -> (current: bool)
        ensures current == self.spec_is_current_for(candidate, evaluated_at)
    {
        let current = crate::candidate::candidate_matches(&self.candidate(), &candidate)
            && self.source_revision() == candidate.source_revision()
            && self.observed_at() <= evaluated_at
            && evaluated_at <= self.expires_at();
        proof {
            reveal(EvidenceBinding::spec_is_current_for);
        }
        current
    }

    /// Returns whether candidate or producing-revision identity differs.
    #[must_use]
    pub const fn is_mismatched(&self, candidate: ReleaseCandidate) -> (mismatched: bool)
        ensures mismatched == self.spec_is_mismatched(candidate)
    {
        let mismatched = !crate::candidate::candidate_matches(&self.candidate(), &candidate)
            || self.source_revision() != candidate.source_revision();
        proof {
            reveal(EvidenceBinding::spec_is_mismatched);
        }
        mismatched
    }

    /// Specification predicate for candidate or source-revision mismatch.
    pub open spec fn spec_is_mismatched(&self, candidate: ReleaseCandidate) -> bool {
        !crate::candidate::candidate_matches_exactly(self.spec_candidate(), candidate)
            || self.spec_source_revision() != candidate.spec_source_revision()
    }

    /// Returns whether only the time window makes the otherwise exact binding stale.
    #[must_use]
    pub const fn is_stale_at(
        &self,
        candidate: ReleaseCandidate,
        evaluated_at: u64,
    ) -> (stale: bool)
        ensures stale == self.spec_is_stale_at(candidate, evaluated_at)
    {
        let stale = crate::candidate::candidate_matches(&self.candidate(), &candidate)
            && self.source_revision() == candidate.source_revision()
            && (evaluated_at < self.observed_at || evaluated_at > self.expires_at)
        ;
        proof {
            reveal(EvidenceBinding::spec_is_stale_at);
        }
        stale
    }

    /// Specification predicate for a stale otherwise exact binding.
    pub open spec fn spec_is_stale_at(
        &self,
        candidate: ReleaseCandidate,
        evaluated_at: u64,
    ) -> bool {
        crate::candidate::candidate_matches_exactly(self.spec_candidate(), candidate)
            && self.spec_source_revision() == candidate.spec_source_revision()
            && (evaluated_at < self.spec_observed_at() || evaluated_at > self.spec_expires_at())
    }
}

/// Logical equality of every field in two evidence bindings.
pub open spec fn bindings_match(left: EvidenceBinding, right: EvidenceBinding) -> bool {
    crate::candidate::candidate_matches_exactly(left.spec_candidate(), right.spec_candidate())
        && left.spec_observed_at() == right.spec_observed_at()
        && left.spec_expires_at() == right.spec_expires_at()
        && left.spec_sequence() == right.spec_sequence()
        && left.spec_source_revision() == right.spec_source_revision()
}

/// Executable equality of every field in two evidence bindings.
pub const fn bindings_equal(left: &EvidenceBinding, right: &EvidenceBinding) -> (equal: bool)
    ensures equal == bindings_match(*left, *right),
{
    crate::candidate::candidate_matches(&left.candidate(), &right.candidate())
        && left.observed_at() == right.observed_at()
        && left.expires_at() == right.expires_at()
        && left.sequence() == right.sequence()
        && left.source_revision() == right.source_revision()
}

} // verus!
