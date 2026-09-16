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
    ) -> (assessment: Self)
        ensures
            assessment.spec_requirement() == requirement,
            assessment.spec_is_satisfied() == satisfied,
            assessment.spec_contributing_count() == contributing_count,
            assessment.spec_stale_count() == stale_count,
            assessment.spec_mismatched_count() == mismatched_count,
            assessment.spec_wrong_source_count() == wrong_source_count,
            assessment.spec_unreviewed_count() == unreviewed_count,
            assessment.spec_unsigned_count() == unsigned_count,
            assessment.spec_is_conflicting() == conflicting,
            assessment.spec_contributing_digest() == contributing_digest,
    {
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
    pub const fn requirement(&self) -> (requirement: EvidenceRequirement)
        ensures requirement == self.spec_requirement()
    {
        self.requirement
    }

    /// Logical view of the requirement identity.
    pub closed spec fn spec_requirement(&self) -> EvidenceRequirement { self.requirement }

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
    pub const fn contributing_count(&self) -> (count: u16)
        ensures count == self.spec_contributing_count()
    { self.contributing_count }

    /// Logical view of the contributing-observation count.
    pub closed spec fn spec_contributing_count(&self) -> u16 { self.contributing_count }

    /// Returns the saturated stale-observation count.
    #[must_use]
    pub const fn stale_count(&self) -> (count: u16)
        ensures count == self.spec_stale_count()
    { self.stale_count }

    /// Logical view of the stale-observation count.
    pub closed spec fn spec_stale_count(&self) -> u16 { self.stale_count }

    /// Returns the saturated candidate/revision mismatch count.
    #[must_use]
    pub const fn mismatched_count(&self) -> (count: u16)
        ensures count == self.spec_mismatched_count()
    { self.mismatched_count }

    /// Logical view of the candidate/revision mismatch count.
    pub closed spec fn spec_mismatched_count(&self) -> u16 { self.mismatched_count }

    /// Returns the saturated wrong-source count.
    #[must_use]
    pub const fn wrong_source_count(&self) -> (count: u16)
        ensures count == self.spec_wrong_source_count()
    { self.wrong_source_count }

    /// Logical view of the wrong-source count.
    pub closed spec fn spec_wrong_source_count(&self) -> u16 { self.wrong_source_count }

    /// Returns the saturated unreviewed-observation count.
    #[must_use]
    pub const fn unreviewed_count(&self) -> (count: u16)
        ensures count == self.spec_unreviewed_count()
    { self.unreviewed_count }

    /// Logical view of the unreviewed-observation count.
    pub closed spec fn spec_unreviewed_count(&self) -> u16 { self.unreviewed_count }

    /// Returns the saturated unsigned-observation count.
    #[must_use]
    pub const fn unsigned_count(&self) -> (count: u16)
        ensures count == self.spec_unsigned_count()
    { self.unsigned_count }

    /// Logical view of the unsigned-observation count.
    pub closed spec fn spec_unsigned_count(&self) -> u16 { self.unsigned_count }

    /// Returns whether otherwise-contributing observations disagreed.
    #[must_use]
    pub const fn is_conflicting(&self) -> (conflicting: bool)
        ensures conflicting == self.spec_is_conflicting()
    { self.conflicting }

    /// Logical view of whether contributing observations conflict.
    pub closed spec fn spec_is_conflicting(&self) -> bool { self.conflicting }

    /// Logical condition under which this assessment emits no diagnostic.
    pub(crate) open spec fn spec_diagnostics_clear(&self) -> bool {
        self.spec_contributing_count() > 0
            && self.spec_stale_count() == 0
            && self.spec_mismatched_count() == 0
            && self.spec_wrong_source_count() == 0
            && self.spec_unreviewed_count() == 0
            && self.spec_unsigned_count() == 0
            && !self.spec_is_conflicting()
    }

    /// Returns the order-independent aggregate of contributing artifact digests.
    #[must_use]
    pub const fn contributing_digest(&self) -> (digest: Sha256Digest)
        ensures digest == self.spec_contributing_digest()
    { self.contributing_digest }

    /// Logical view of the aggregate contributing-artifact digest.
    pub closed spec fn spec_contributing_digest(&self) -> Sha256Digest {
        self.contributing_digest
    }
}

/// Canonical assessment of one of the twenty-five production criteria.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CriterionAssessment {
    criterion: AcceptanceCriterion,
    satisfied: bool,
}

impl CriterionAssessment {
    pub(crate) const fn new(
        criterion: AcceptanceCriterion,
        satisfied: bool,
    ) -> (assessment: Self)
        ensures
            assessment.spec_criterion() == criterion,
            assessment.spec_is_satisfied() == satisfied,
    {
        Self { criterion, satisfied }
    }

    /// Returns the criterion identity.
    #[must_use]
    pub const fn criterion(&self) -> (criterion: AcceptanceCriterion)
        ensures criterion == self.spec_criterion()
    { self.criterion }

    /// Logical view of the criterion identity.
    pub closed spec fn spec_criterion(&self) -> AcceptanceCriterion { self.criterion }

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
    ) -> (assessment: Self)
        ensures
            assessment.spec_slice() == slice,
            assessment.spec_is_satisfied() == satisfied,
            assessment.spec_ready_count() == ready_count,
            assessment.spec_stale_count() == stale_count,
            assessment.spec_mismatched_count() == mismatched_count,
            assessment.spec_unreviewed_count() == unreviewed_count,
            assessment.spec_not_ready_count() == not_ready_count,
            assessment.spec_is_conflicting() == conflicting,
            assessment.spec_report_digest() == report_digest,
    {
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
    pub const fn slice(&self) -> (slice: QualificationSlice)
        ensures slice == self.spec_slice()
    { self.slice }

    /// Logical view of the H-slice identity.
    pub closed spec fn spec_slice(&self) -> QualificationSlice { self.slice }

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
    pub const fn ready_count(&self) -> (count: u16)
        ensures count == self.spec_ready_count()
    { self.ready_count }

    /// Logical view of the contributing ready-report count.
    pub closed spec fn spec_ready_count(&self) -> u16 { self.ready_count }

    /// Returns the saturated stale-report count.
    #[must_use]
    pub const fn stale_count(&self) -> (count: u16)
        ensures count == self.spec_stale_count()
    { self.stale_count }

    /// Logical view of the stale-report count.
    pub closed spec fn spec_stale_count(&self) -> u16 { self.stale_count }

    /// Returns the saturated mismatched-report count.
    #[must_use]
    pub const fn mismatched_count(&self) -> (count: u16)
        ensures count == self.spec_mismatched_count()
    { self.mismatched_count }

    /// Logical view of the mismatched-report count.
    pub closed spec fn spec_mismatched_count(&self) -> u16 { self.mismatched_count }

    /// Returns the saturated unreviewed-report count.
    #[must_use]
    pub const fn unreviewed_count(&self) -> (count: u16)
        ensures count == self.spec_unreviewed_count()
    { self.unreviewed_count }

    /// Logical view of the unreviewed-report count.
    pub closed spec fn spec_unreviewed_count(&self) -> u16 { self.unreviewed_count }

    /// Returns the saturated explicit-not-ready report count.
    #[must_use]
    pub const fn not_ready_count(&self) -> (count: u16)
        ensures count == self.spec_not_ready_count()
    { self.not_ready_count }

    /// Logical view of the explicit-not-ready report count.
    pub closed spec fn spec_not_ready_count(&self) -> u16 { self.not_ready_count }

    /// Returns whether current reports disagreed in verdict or report digest.
    #[must_use]
    pub const fn is_conflicting(&self) -> (conflicting: bool)
        ensures conflicting == self.spec_is_conflicting()
    { self.conflicting }

    /// Logical view of whether contributing reports conflict.
    pub closed spec fn spec_is_conflicting(&self) -> bool { self.conflicting }

    pub(crate) open spec fn spec_diagnostics_clear(&self) -> bool {
        self.spec_ready_count() > 0
            && self.spec_stale_count() == 0
            && self.spec_mismatched_count() == 0
            && self.spec_unreviewed_count() == 0
            && self.spec_not_ready_count() == 0
            && !self.spec_is_conflicting()
    }

    /// Returns the order-independent aggregate of contributing report digests.
    #[must_use]
    pub const fn report_digest(&self) -> (digest: Sha256Digest)
        ensures digest == self.spec_report_digest()
    { self.report_digest }

    /// Logical view of the aggregate contributing-report digest.
    pub closed spec fn spec_report_digest(&self) -> Sha256Digest { self.report_digest }
}

} // verus!
