//! Mathematical performance thresholds and exact copies of their measured inputs.

use super::{PerformanceEvidence, PerformanceExpectation, PerformanceStatistic};

impl PerformanceExpectation {
    #[doc = " Exact executable equality of both threshold variant and magnitude."]
    pub(super) const fn same(self, other: Self) -> bool {
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
    #[doc = " Exact executable equality of the requested and measured statistic."]
    pub(super) const fn same(self, other: Self) -> bool {
        matches!(
            (self, other),
            (Self::Mean, Self::Mean)
                | (Self::Median, Self::Median)
                | (Self::Minimum, Self::Minimum)
                | (Self::Maximum, Self::Maximum)
                | (Self::Percentile95, Self::Percentile95)
                | (Self::Percentile99, Self::Percentile99)
        )
    }
}
impl Clone for PerformanceEvidence {
    fn clone(&self) -> Self {
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
