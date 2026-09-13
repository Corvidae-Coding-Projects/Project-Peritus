//! Same-workload performance requirements and measured evidence.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use crate::{EvidenceBinding, ObligationError, ObligationErrorKind};
use peritus_types::Sha256Digest;
use vstd::prelude::*;

mod model;

verus! {

/// Statistic selected by the public performance requirement.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PerformanceStatistic {
    /// Arithmetic mean.
    Mean,
    /// Median observation.
    Median,
    /// Minimum observation.
    Minimum,
    /// Maximum observation.
    Maximum,
    /// 95th percentile.
    Percentile95,
    /// 99th percentile.
    Percentile99,
}

/// Public threshold, expressed in the workload's fixed integer unit.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PerformanceExpectation {
    /// Candidate statistic must not exceed the absolute threshold.
    CandidateAtMost(u64),
    /// Candidate statistic must meet the absolute threshold.
    CandidateAtLeast(u64),
    /// Baseline minus candidate must meet the requested improvement.
    ImprovementAtLeast(u64),
    /// Candidate may exceed baseline by no more than this amount.
    RegressionAtMost(u64),
}

/// Public same-workload performance contract.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PerformanceRequirement {
    workload_identity: Sha256Digest,
    statistic: PerformanceStatistic,
    minimum_repetitions: u32,
    public_threshold: PerformanceExpectation,
}

impl PerformanceRequirement {
    /// Exact workload identity in the public requirement.
    pub closed spec fn spec_workload(&self) -> Sha256Digest { self.workload_identity }
    /// Exact requested statistic.
    pub closed spec fn spec_statistic(&self) -> PerformanceStatistic { self.statistic }
    /// Exact required repetition count.
    pub closed spec fn spec_repetitions(&self) -> u32 { self.minimum_repetitions }
    /// Exact threshold selected by the public requirement.
    pub closed spec fn spec_threshold(&self) -> PerformanceExpectation { self.public_threshold }

    /// Creates a nonzero repeated-measurement requirement.
    ///
    /// # Errors
    ///
    /// Rejects zero repetitions.
    pub const fn new(
        workload_identity: Sha256Digest,
        statistic: PerformanceStatistic,
        minimum_repetitions: u32,
        public_threshold: PerformanceExpectation,
    ) -> (result: Result<Self, ObligationError>)
        ensures result.is_ok() == (minimum_repetitions > 0),
            match result {
                Ok(value) => value.spec_workload() == workload_identity
                    && value.spec_statistic() == statistic
                    && value.spec_repetitions() == minimum_repetitions
                    && value.spec_threshold() == public_threshold,
                Err(_) => true,
            },
    {
        if minimum_repetitions == 0 {
            Err(ObligationError::plain(ObligationErrorKind::InvalidPerformance))
        } else {
            Ok(Self {
                workload_identity,
                statistic,
                minimum_repetitions,
                public_threshold,
            })
        }
    }

    /// Exact workload identity shared by baseline and candidate.
    #[must_use]
    pub const fn workload_identity(self) -> (value: Sha256Digest)
        ensures value == self.spec_workload(),
    { self.workload_identity }

    /// Public statistic.
    #[must_use]
    pub const fn statistic(self) -> (value: PerformanceStatistic)
        ensures value == self.spec_statistic(),
    { self.statistic }

    /// Minimum repetitions required for each measurement set.
    #[must_use]
    pub const fn minimum_repetitions(self) -> (value: u32)
        ensures value == self.spec_repetitions(),
    { self.minimum_repetitions }

    /// Public threshold.
    #[must_use]
    pub const fn public_threshold(self) -> (value: PerformanceExpectation)
        ensures value == self.spec_threshold(),
    { self.public_threshold }
}

/// Candidate-bound performance evidence retaining every required measurement field.
#[derive(Debug, Eq, PartialEq)]
pub struct PerformanceEvidence {
    binding: EvidenceBinding,
    workload_identity: Sha256Digest,
    baseline: u64,
    candidate: u64,
    repetitions: u32,
    statistic: PerformanceStatistic,
    noise_margin: u64,
    public_threshold: PerformanceExpectation,
}

impl PerformanceEvidence {
    /// Exact binding to the observed candidate and obligation.
    pub closed spec fn spec_binding(&self) -> EvidenceBinding { self.binding }
    /// Exact identity of the measured workload.
    pub closed spec fn spec_workload(&self) -> Sha256Digest { self.workload_identity }
    /// Exact baseline measurement.
    pub closed spec fn spec_baseline(&self) -> u64 { self.baseline }
    /// Exact candidate measurement.
    pub closed spec fn spec_candidate(&self) -> u64 { self.candidate }
    /// Exact repetition count retained by the observation.
    pub closed spec fn spec_repetitions(&self) -> u32 { self.repetitions }
    /// Exact statistic used by the measurement.
    pub closed spec fn spec_statistic(&self) -> PerformanceStatistic { self.statistic }
    /// Exact admitted noise margin.
    pub closed spec fn spec_noise(&self) -> u64 { self.noise_margin }
    /// Exact copy of the public threshold.
    pub closed spec fn spec_threshold(&self) -> PerformanceExpectation { self.public_threshold }

    /// Exact observation-to-requirement match and unbounded-integer threshold semantics.
    pub open spec fn spec_satisfies(&self, requirement: PerformanceRequirement) -> bool {
        self.spec_workload().spec_bytes()@ == requirement.spec_workload().spec_bytes()@
            && self.spec_statistic() == requirement.spec_statistic()
            && self.spec_repetitions() >= requirement.spec_repetitions()
            && self.spec_threshold() == requirement.spec_threshold()
            && self.spec_threshold().spec_met(self.spec_baseline(), self.spec_candidate(), self.spec_noise())
    }

    /// Creates complete repeated-measurement evidence.
    ///
    /// # Errors
    ///
    /// Rejects zero repetitions.
    #[allow(clippy::too_many_arguments, reason = "all public performance evidence fields remain explicit")]
    pub fn new(
        binding: EvidenceBinding,
        workload_identity: Sha256Digest,
        baseline: u64,
        candidate: u64,
        repetitions: u32,
        statistic: PerformanceStatistic,
        noise_margin: u64,
        public_threshold: PerformanceExpectation,
    ) -> (result: Result<Self, ObligationError>)
        ensures result.is_ok() == (repetitions > 0),
            match result {
                Ok(value) => value.spec_binding() == binding
                    && value.spec_workload() == workload_identity
                    && value.spec_baseline() == baseline
                    && value.spec_candidate() == candidate
                    && value.spec_repetitions() == repetitions
                    && value.spec_statistic() == statistic
                    && value.spec_noise() == noise_margin
                    && value.spec_threshold() == public_threshold,
                Err(_) => true,
            },
    {
        if repetitions == 0 {
            Err(ObligationError::plain(ObligationErrorKind::InvalidPerformance))
        } else {
            Ok(Self {
                binding,
                workload_identity,
                baseline,
                candidate,
                repetitions,
                statistic,
                noise_margin,
                public_threshold,
            })
        }
    }

    /// Complete current-candidate binding.
    #[must_use]
    pub const fn binding(&self) -> (value: &EvidenceBinding)
        ensures *value == self.spec_binding(),
    { &self.binding }

    /// Same-workload identity.
    #[must_use]
    pub const fn workload_identity(&self) -> (value: Sha256Digest)
        ensures value == self.spec_workload(),
    { self.workload_identity }

    /// Baseline statistic.
    #[must_use]
    pub const fn baseline(&self) -> (value: u64)
        ensures value == self.spec_baseline(),
    { self.baseline }

    /// Candidate statistic.
    #[must_use]
    pub const fn candidate(&self) -> (value: u64)
        ensures value == self.spec_candidate(),
    { self.candidate }

    /// Repetitions in each measurement set.
    #[must_use]
    pub const fn repetitions(&self) -> (value: u32)
        ensures value == self.spec_repetitions(),
    { self.repetitions }

    /// Measured statistic.
    #[must_use]
    pub const fn statistic(&self) -> (value: PerformanceStatistic)
        ensures value == self.spec_statistic(),
    { self.statistic }

    /// Admitted public noise margin.
    #[must_use]
    pub const fn noise_margin(&self) -> (value: u64)
        ensures value == self.spec_noise(),
    { self.noise_margin }

    /// Threshold copied from the public requirement.
    #[must_use]
    pub const fn public_threshold(&self) -> (value: PerformanceExpectation)
        ensures value == self.spec_threshold(),
    { self.public_threshold }

    /// Whether this evidence measures the exact contract and meets its threshold.
    #[must_use]
    pub fn satisfies(&self, requirement: PerformanceRequirement) -> (satisfied: bool)
        ensures satisfied == self.spec_satisfies(requirement),
    {
        matches!(crate::order::compare(self.workload_identity.as_bytes(), requirement.workload_identity().as_bytes()), core::cmp::Ordering::Equal)
            && self.statistic.same(requirement.statistic())
            && self.repetitions >= requirement.minimum_repetitions()
            && self.public_threshold.same(requirement.public_threshold())
            && threshold_met(
                self.baseline,
                self.candidate,
                self.noise_margin,
                self.public_threshold,
            )
    }
}

const fn threshold_met(
    baseline: u64,
    candidate: u64,
    noise_margin: u64,
    threshold: PerformanceExpectation,
) -> (met: bool)
    ensures met == threshold.spec_met(baseline, candidate, noise_margin),
{
    match threshold {
        PerformanceExpectation::CandidateAtMost(limit) => {
            candidate <= limit.saturating_add(noise_margin)
        }
        PerformanceExpectation::CandidateAtLeast(limit) => {
            candidate.saturating_add(noise_margin) >= limit
        }
        PerformanceExpectation::ImprovementAtLeast(delta) => {
            (baseline as u128) + (noise_margin as u128) >= (candidate as u128) + (delta as u128)
        }
        PerformanceExpectation::RegressionAtMost(delta) => {
            candidate <= baseline.saturating_add(delta).saturating_add(noise_margin)
        }
    }
}

} // verus!
