//! Mathematical performance thresholds and exact copies of their measured inputs.

use super::{PerformanceEvidence, PerformanceExpectation, PerformanceStatistic};
use vstd::prelude::*;

verus! {

impl PerformanceExpectation {
    /// Public threshold interpreted over unbounded integers, retaining negative improvements.
    pub open spec fn spec_met(self, baseline: u64, candidate: u64, noise: u64) -> bool {
        match self {
            Self::CandidateAtMost(limit) => candidate as int <= limit as int + noise as int,
            Self::CandidateAtLeast(limit) => candidate as int + noise as int >= limit as int,
            Self::ImprovementAtLeast(delta) => baseline as int - candidate as int + noise as int >= delta as int,
            Self::RegressionAtMost(delta) => candidate as int - baseline as int <= delta as int + noise as int,
        }
    }

    /// Exact executable equality of both threshold variant and magnitude.
    pub(super) const fn same(self, other: Self) -> (same: bool)
        ensures same == (self == other),
    {
        match (self, other) {
            (Self::CandidateAtMost(left), Self::CandidateAtMost(right))
            | (Self::CandidateAtLeast(left), Self::CandidateAtLeast(right))
            | (Self::ImprovementAtLeast(left), Self::ImprovementAtLeast(right))
            | (Self::RegressionAtMost(left), Self::RegressionAtMost(right)) => left == right,
            _ => false,
        }
    }
}

impl PerformanceStatistic {
    /// Exact executable equality of the requested and measured statistic.
    pub(super) const fn same(self, other: Self) -> (same: bool)
        ensures same == (self == other),
    {
        matches!((self, other),
            (Self::Mean, Self::Mean)
            | (Self::Median, Self::Median)
            | (Self::Minimum, Self::Minimum)
            | (Self::Maximum, Self::Maximum)
            | (Self::Percentile95, Self::Percentile95)
            | (Self::Percentile99, Self::Percentile99))
    }
}

impl Clone for PerformanceEvidence {
    fn clone(&self) -> (value: Self)
        ensures value.spec_binding().spec_same_content(&self.spec_binding()),
            value.spec_workload() == self.spec_workload(),
            value.spec_baseline() == self.spec_baseline(),
            value.spec_candidate() == self.spec_candidate(),
            value.spec_repetitions() == self.spec_repetitions(),
            value.spec_statistic() == self.spec_statistic(),
            value.spec_noise() == self.spec_noise(),
            value.spec_threshold() == self.spec_threshold(),
    {
        proof {
            reveal(PerformanceEvidence::spec_binding);
            reveal(PerformanceEvidence::spec_workload);
            reveal(PerformanceEvidence::spec_baseline);
            reveal(PerformanceEvidence::spec_candidate);
            reveal(PerformanceEvidence::spec_repetitions);
            reveal(PerformanceEvidence::spec_statistic);
            reveal(PerformanceEvidence::spec_noise);
            reveal(PerformanceEvidence::spec_threshold);
        }
        Self {
            binding: self.binding.clone(),
            workload_identity: self.workload_identity,
            baseline: self.baseline,
            candidate: self.candidate,
            repetitions: self.repetitions,
            statistic: self.statistic,
            noise_margin: self.noise_margin,
            public_threshold: self.public_threshold,
        }
    }
}

} // verus!
