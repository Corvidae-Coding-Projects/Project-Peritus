//! Canonical artifact, criterion, and qualification assessments.

use crate::{AcceptanceCriterion, EvidenceRequirement, QualificationSlice};
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

/// Canonical assessment of one required artifact class.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EvidenceAssessment {
    requirement: EvidenceRequirement,
    satisfied: bool,
    contributing_count: u16,
    stale_count: u16,
    mismatched_count: u16,
    wrong_source_count: u16,
    unreviewed_count: u16,
    unsigned_count: u16,
    conflicting: bool,
    contributing_digest: Sha256Digest,
}

impl EvidenceAssessment {
    #[allow(clippy::too_many_arguments, reason = "canonical evidence diagnostics remain independently inspectable")]
    pub(crate) const fn new(
        requirement: EvidenceRequirement,
        satisfied: bool,
        contributing_count: u16,
        stale_count: u16,
        mismatched_count: u16,
        wrong_source_count: u16,
        unreviewed_count: u16,
        unsigned_count: u16,
        conflicting: bool,
        contributing_digest: Sha256Digest,
    ) -> Self {
        Self {
            requirement,
            satisfied,
            contributing_count,
            stale_count,
            mismatched_count,
            wrong_source_count,
            unreviewed_count,
            unsigned_count,
            conflicting,
            contributing_digest,
        }
    }

    /// Returns the requirement identity.
    #[must_use]
    pub const fn requirement(&self) -> EvidenceRequirement { self.requirement }

    /// Returns whether current, exact, reviewed, signed, nonconflicting evidence satisfied it.
    #[must_use]
    pub const fn is_satisfied(&self) -> (satisfied: bool)
        ensures satisfied == self.spec_is_satisfied()
    {
        self.satisfied
    }

    /// Logical view of whether this assessment is satisfied.
    pub closed spec fn spec_is_satisfied(&self) -> bool {
        self.satisfied
    }

    /// Returns the saturated number of contributing observations.
    #[must_use]
    pub const fn contributing_count(&self) -> u16 { self.contributing_count }

    /// Returns the saturated stale-observation count.
    #[must_use]
    pub const fn stale_count(&self) -> u16 { self.stale_count }

    /// Returns the saturated candidate/revision mismatch count.
    #[must_use]
    pub const fn mismatched_count(&self) -> u16 { self.mismatched_count }

    /// Returns the saturated wrong-source count.
    #[must_use]
    pub const fn wrong_source_count(&self) -> u16 { self.wrong_source_count }

    /// Returns the saturated unreviewed-observation count.
    #[must_use]
    pub const fn unreviewed_count(&self) -> u16 { self.unreviewed_count }

    /// Returns the saturated unsigned-observation count.
    #[must_use]
    pub const fn unsigned_count(&self) -> u16 { self.unsigned_count }

    /// Returns whether otherwise-contributing observations disagreed.
    #[must_use]
    pub const fn is_conflicting(&self) -> bool { self.conflicting }

    /// Returns the order-independent aggregate of contributing artifact digests.
    #[must_use]
    pub const fn contributing_digest(&self) -> Sha256Digest { self.contributing_digest }
}

/// Canonical assessment of one of the twenty-five production criteria.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CriterionAssessment {
    criterion: AcceptanceCriterion,
    satisfied: bool,
}

impl CriterionAssessment {
    pub(crate) const fn new(criterion: AcceptanceCriterion, satisfied: bool) -> Self {
        Self { criterion, satisfied }
    }

    /// Returns the criterion identity.
    #[must_use]
    pub const fn criterion(&self) -> AcceptanceCriterion { self.criterion }

    /// Returns whether every evidence requirement mapped to the criterion was satisfied.
    #[must_use]
    pub const fn is_satisfied(&self) -> (satisfied: bool)
        ensures satisfied == self.spec_is_satisfied()
    {
        self.satisfied
    }

    /// Logical view of whether this assessment is satisfied.
    pub closed spec fn spec_is_satisfied(&self) -> bool {
        self.satisfied
    }
}

/// Canonical H0-H3 input assessment.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct QualificationAssessment {
    slice: QualificationSlice,
    satisfied: bool,
    ready_count: u16,
    stale_count: u16,
    mismatched_count: u16,
    unreviewed_count: u16,
    not_ready_count: u16,
    conflicting: bool,
    report_digest: Sha256Digest,
}

impl QualificationAssessment {
    #[allow(clippy::too_many_arguments, reason = "qualification failure dimensions remain independently auditable")]
    pub(crate) const fn new(
        slice: QualificationSlice,
        satisfied: bool,
        ready_count: u16,
        stale_count: u16,
        mismatched_count: u16,
        unreviewed_count: u16,
        not_ready_count: u16,
        conflicting: bool,
        report_digest: Sha256Digest,
    ) -> Self {
        Self {
            slice,
            satisfied,
            ready_count,
            stale_count,
            mismatched_count,
            unreviewed_count,
            not_ready_count,
            conflicting,
            report_digest,
        }
    }

    /// Returns the H-slice identity.
    #[must_use]
    pub const fn slice(&self) -> QualificationSlice { self.slice }

    /// Returns whether a current exact signed ready report exists without conflicting input.
    #[must_use]
    pub const fn is_satisfied(&self) -> (satisfied: bool)
        ensures satisfied == self.spec_is_satisfied()
    {
        self.satisfied
    }

    /// Logical view of whether this assessment is satisfied.
    pub closed spec fn spec_is_satisfied(&self) -> bool {
        self.satisfied
    }

    /// Returns the saturated contributing ready-report count.
    #[must_use]
    pub const fn ready_count(&self) -> u16 { self.ready_count }

    /// Returns the saturated stale-report count.
    #[must_use]
    pub const fn stale_count(&self) -> u16 { self.stale_count }

    /// Returns the saturated mismatched-report count.
    #[must_use]
    pub const fn mismatched_count(&self) -> u16 { self.mismatched_count }

    /// Returns the saturated unreviewed-report count.
    #[must_use]
    pub const fn unreviewed_count(&self) -> u16 { self.unreviewed_count }

    /// Returns the saturated explicit-not-ready report count.
    #[must_use]
    pub const fn not_ready_count(&self) -> u16 { self.not_ready_count }

    /// Returns whether current reports disagreed in verdict or report digest.
    #[must_use]
    pub const fn is_conflicting(&self) -> bool { self.conflicting }

    /// Returns the order-independent aggregate of contributing report digests.
    #[must_use]
    pub const fn report_digest(&self) -> Sha256Digest { self.report_digest }
}

} // verus!
