//! Frozen 95% Wilson intervals for raw binomial proportions.

use crate::{EvaluationError, EvaluationErrorKind, EvaluationOperation, ProbabilityMillionths};

/// One raw-binomial Wilson interval; never pass@k uncertainty.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WilsonInterval {
    successes: u32,
    total: u32,
    confidence_millionths: u32,
    lower: ProbabilityMillionths,
    upper: ProbabilityMillionths,
}

impl WilsonInterval {
    /// Computes the frozen 95% interval and quantizes only the final values.
    ///
    /// # Errors
    /// Rejects zero total or successes above total.
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "Wilson transcendental arithmetic is ephemeral; checked millionths are authoritative"
    )]
    pub fn ninety_five(successes: u32, total: u32) -> Result<Self, EvaluationError> {
        if total == 0 || successes > total {
            return Err(crate::invalid(
                EvaluationErrorKind::Statistics,
                EvaluationOperation::Analyze,
                "Wilson interval inputs violate binomial preconditions",
            ));
        }
        let n = f64::from(total);
        let p = f64::from(successes) / n;
        let z = 1.959_963_984_540_054_f64;
        let z2 = z * z;
        let denominator = 1.0 + z2 / n;
        let center = (p + z2 / (2.0 * n)) / denominator;
        let margin = z * (p.mul_add(1.0 - p, z2 / (4.0 * n)) / n).sqrt() / denominator;
        let quantize = |value: f64| -> Result<ProbabilityMillionths, EvaluationError> {
            let bounded = value.clamp(0.0, 1.0);
            ProbabilityMillionths::new((bounded * 1_000_000.0).round() as u32)
        };
        Ok(Self {
            successes,
            total,
            confidence_millionths: 950_000,
            lower: quantize(center - margin)?,
            upper: quantize(center + margin)?,
        })
    }
    /// Successful observations.
    #[must_use]
    pub const fn successes(self) -> u32 {
        self.successes
    }
    /// Total observations.
    #[must_use]
    pub const fn total(self) -> u32 {
        self.total
    }
    /// Frozen confidence level.
    #[must_use]
    pub const fn confidence_millionths(self) -> u32 {
        self.confidence_millionths
    }
    /// Lower bound.
    #[must_use]
    pub const fn lower(self) -> ProbabilityMillionths {
        self.lower
    }
    /// Upper bound.
    #[must_use]
    pub const fn upper(self) -> ProbabilityMillionths {
        self.upper
    }
}
