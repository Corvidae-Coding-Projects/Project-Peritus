//! Deterministic fixed-point pass@k estimator.

use crate::{EvaluationError, EvaluationErrorKind, EvaluationOperation};

const INTERNAL_SCALE: u128 = 1_000_000_000_000_000_000;

/// Probability in closed integer millionths.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProbabilityMillionths(u32);

impl ProbabilityMillionths {
    /// Creates a probability at or below one million.
    ///
    /// # Errors
    /// Rejects values above one.
    pub const fn new(value: u32) -> Result<Self, EvaluationError> {
        if value > 1_000_000 {
            Err(crate::invalid(
                EvaluationErrorKind::Statistics,
                EvaluationOperation::Analyze,
                "probability millionths exceed one",
            ))
        } else {
            Ok(Self(value))
        }
    }
    /// Returns integer millionths.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// One pass@k estimate retaining exact raw inputs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PassAtK {
    total: u32,
    successes: u32,
    k: u16,
    estimate: ProbabilityMillionths,
}

impl PassAtK {
    /// Total included rollouts.
    #[must_use]
    pub const fn total(self) -> u32 {
        self.total
    }
    /// Successful included rollouts.
    #[must_use]
    pub const fn successes(self) -> u32 {
        self.successes
    }
    /// Requested k.
    #[must_use]
    pub const fn k(self) -> u16 {
        self.k
    }
    /// Deterministic estimate in millionths.
    #[must_use]
    pub const fn estimate(self) -> ProbabilityMillionths {
        self.estimate
    }
}

/// Computes `1 - C(n-c,k)/C(n,k)` using checked high-resolution fixed-point products.
///
/// # Errors
/// Rejects zero/oversized k, zero total, or successes above total.
pub fn pass_at_k(total: u32, successes: u32, k: u16) -> Result<PassAtK, EvaluationError> {
    if total == 0 || successes > total || k == 0 || u32::from(k) > total {
        return Err(crate::invalid(
            EvaluationErrorKind::Statistics,
            EvaluationOperation::Analyze,
            "pass@k inputs violate n/c/k preconditions",
        ));
    }
    let estimate = if successes == 0 {
        0
    } else if total - successes < u32::from(k) {
        1_000_000
    } else {
        let mut failure = INTERNAL_SCALE;
        for index in 0..u32::from(k) {
            let numerator = u128::from(total - successes - index);
            let denominator = u128::from(total - index);
            failure = failure
                .checked_mul(numerator)
                .ok_or_else(arithmetic)?
                .checked_add(denominator / 2)
                .ok_or_else(arithmetic)?
                / denominator;
        }
        let pass = INTERNAL_SCALE.checked_sub(failure).ok_or_else(arithmetic)?;
        u32::try_from(
            pass.checked_mul(1_000_000)
                .ok_or_else(arithmetic)?
                .checked_add(INTERNAL_SCALE / 2)
                .ok_or_else(arithmetic)?
                / INTERNAL_SCALE,
        )
        .map_err(|_| arithmetic())?
    };
    Ok(PassAtK { total, successes, k, estimate: ProbabilityMillionths::new(estimate)? })
}

const fn arithmetic() -> EvaluationError {
    crate::invalid(
        EvaluationErrorKind::Statistics,
        EvaluationOperation::Analyze,
        "pass@k checked arithmetic overflowed",
    )
}
