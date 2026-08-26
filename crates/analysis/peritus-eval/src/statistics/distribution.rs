//! Checked integer distribution summaries with nearest-rank percentiles.

use crate::{EvaluationError, EvaluationErrorKind, EvaluationOperation};

/// Raw integer distribution summary retaining all key order statistics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DistributionSummary {
    count: u32,
    missing: u32,
    total: u64,
    minimum: u64,
    maximum: u64,
    mean: u64,
    p50: u64,
    p95: u64,
    p99: u64,
}

impl DistributionSummary {
    /// Summarizes known values and explicit missing count.
    ///
    /// # Errors
    /// Rejects empty known values, count overflow, or sum overflow.
    pub fn new(mut values: Vec<u64>, missing: u32) -> Result<Self, EvaluationError> {
        if values.is_empty() || values.len() > usize::try_from(u32::MAX).unwrap_or(usize::MAX) {
            return Err(crate::invalid(
                EvaluationErrorKind::Statistics,
                EvaluationOperation::Analyze,
                "distribution has no known values or exceeds count bounds",
            ));
        }
        values.sort_unstable();
        let total = values
            .iter()
            .try_fold(0_u64, |sum, value| sum.checked_add(*value))
            .ok_or_else(|| {
                crate::invalid(
                    EvaluationErrorKind::Statistics,
                    EvaluationOperation::Analyze,
                    "distribution total overflowed",
                )
            })?;
        let count = u32::try_from(values.len()).map_err(|_| {
            crate::invalid(
                EvaluationErrorKind::LimitExceeded,
                EvaluationOperation::Analyze,
                "distribution count exceeds u32",
            )
        })?;
        let percentile = |numerator: usize| -> u64 {
            let rank = values.len().saturating_mul(numerator).div_ceil(100).max(1);
            values[rank.saturating_sub(1).min(values.len() - 1)]
        };
        Ok(Self {
            count,
            missing,
            total,
            minimum: values[0],
            maximum: values[values.len() - 1],
            mean: total / u64::from(count),
            p50: percentile(50),
            p95: percentile(95),
            p99: percentile(99),
        })
    }
    /// Known count.
    #[must_use]
    pub const fn count(self) -> u32 {
        self.count
    }
    /// Missing count.
    #[must_use]
    pub const fn missing(self) -> u32 {
        self.missing
    }
    /// Checked total.
    #[must_use]
    pub const fn total(self) -> u64 {
        self.total
    }
    /// Minimum.
    #[must_use]
    pub const fn minimum(self) -> u64 {
        self.minimum
    }
    /// Maximum.
    #[must_use]
    pub const fn maximum(self) -> u64 {
        self.maximum
    }
    /// Integer mean.
    #[must_use]
    pub const fn mean(self) -> u64 {
        self.mean
    }
    /// Nearest-rank p50.
    #[must_use]
    pub const fn p50(self) -> u64 {
        self.p50
    }
    /// Nearest-rank p95.
    #[must_use]
    pub const fn p95(self) -> u64 {
        self.p95
    }
    /// Nearest-rank p99.
    #[must_use]
    pub const fn p99(self) -> u64 {
        self.p99
    }
}
