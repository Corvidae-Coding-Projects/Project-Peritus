//! Same-workload performance requirements and measured evidence.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use crate::{EvidenceBinding, ObligationError, ObligationErrorKind};
use peritus_types::Sha256Digest;

mod model;

#[doc = " Statistic selected by the public performance requirement."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PerformanceStatistic {
    #[doc = " Arithmetic mean."]
    Mean,
    #[doc = " Median observation."]
    Median,
    #[doc = " Minimum observation."]
    Minimum,
    #[doc = " Maximum observation."]
    Maximum,
    #[doc = " 95th percentile."]
    Percentile95,
    #[doc = " 99th percentile."]
    Percentile99,
}
#[doc = " Public threshold, expressed in the workload\'s fixed integer unit."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PerformanceExpectation {
    #[doc = " Candidate statistic must not exceed the absolute threshold."]
    CandidateAtMost(u64),
    #[doc = " Candidate statistic must meet the absolute threshold."]
    CandidateAtLeast(u64),
    #[doc = " Baseline minus candidate must meet the requested improvement."]
    ImprovementAtLeast(u64),
    #[doc = " Candidate may exceed baseline by no more than this amount."]
    RegressionAtMost(u64),
}
#[doc = " Public same-workload performance contract."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PerformanceRequirement {
    workload_identity: Sha256Digest,
    statistic: PerformanceStatistic,
    minimum_repetitions: u32,
    public_threshold: PerformanceExpectation,
}
impl PerformanceRequirement {
    #[doc = " Creates a nonzero repeated-measurement requirement."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Rejects zero repetitions."]
    pub const fn new(
        workload_identity: Sha256Digest,
        statistic: PerformanceStatistic,
        minimum_repetitions: u32,
        public_threshold: PerformanceExpectation,
    ) -> Result<Self, ObligationError> {
        if minimum_repetitions == 0 {
            Err(ObligationError::plain(ObligationErrorKind::InvalidPerformance))
        } else {
            Ok(Self { workload_identity, statistic, minimum_repetitions, public_threshold })
        }
    }
    #[doc = " Exact workload identity shared by baseline and candidate."]
    #[must_use]
    pub const fn workload_identity(self) -> Sha256Digest {
        self.workload_identity
    }
    #[doc = " Public statistic."]
    #[must_use]
    pub const fn statistic(self) -> PerformanceStatistic {
        self.statistic
    }
    #[doc = " Minimum repetitions required for each measurement set."]
    #[must_use]
    pub const fn minimum_repetitions(self) -> u32 {
        self.minimum_repetitions
    }
    #[doc = " Public threshold."]
    #[must_use]
    pub const fn public_threshold(self) -> PerformanceExpectation {
        self.public_threshold
    }
}
#[doc = " Candidate-bound performance evidence retaining every required measurement field."]
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
    #[doc = " Creates complete repeated-measurement evidence."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Rejects zero repetitions."]
    #[allow(
        clippy::too_many_arguments,
        reason = "all public performance evidence fields remain explicit"
    )]
    pub fn new(
        binding: EvidenceBinding,
        workload_identity: Sha256Digest,
        baseline: u64,
        candidate: u64,
        repetitions: u32,
        statistic: PerformanceStatistic,
        noise_margin: u64,
        public_threshold: PerformanceExpectation,
    ) -> Result<Self, ObligationError> {
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
    #[doc = " Complete current-candidate binding."]
    #[must_use]
    pub const fn binding(&self) -> &EvidenceBinding {
        &self.binding
    }
    #[doc = " Same-workload identity."]
    #[must_use]
    pub const fn workload_identity(&self) -> Sha256Digest {
        self.workload_identity
    }
    #[doc = " Baseline statistic."]
    #[must_use]
    pub const fn baseline(&self) -> u64 {
        self.baseline
    }
    #[doc = " Candidate statistic."]
    #[must_use]
    pub const fn candidate(&self) -> u64 {
        self.candidate
    }
    #[doc = " Repetitions in each measurement set."]
    #[must_use]
    pub const fn repetitions(&self) -> u32 {
        self.repetitions
    }
    #[doc = " Measured statistic."]
    #[must_use]
    pub const fn statistic(&self) -> PerformanceStatistic {
        self.statistic
    }
    #[doc = " Admitted public noise margin."]
    #[must_use]
    pub const fn noise_margin(&self) -> u64 {
        self.noise_margin
    }
    #[doc = " Threshold copied from the public requirement."]
    #[must_use]
    pub const fn public_threshold(&self) -> PerformanceExpectation {
        self.public_threshold
    }
    #[doc = " Whether this evidence measures the exact contract and meets its threshold."]
    #[must_use]
    pub fn satisfies(&self, requirement: PerformanceRequirement) -> bool {
        matches!(
            crate::order::compare(
                self.workload_identity.as_bytes(),
                requirement.workload_identity().as_bytes()
            ),
            core::cmp::Ordering::Equal
        ) && self.statistic.same(requirement.statistic())
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
) -> bool {
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
