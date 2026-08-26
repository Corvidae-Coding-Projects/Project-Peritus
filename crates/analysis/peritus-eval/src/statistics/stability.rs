//! Per-task ordered rollout stability analysis.

use crate::{EvaluationError, EvaluationErrorKind, EvaluationOperation, ProbabilityMillionths};

/// Closed task stability classification.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum StabilityClass {
    /// Every evaluated rollout passed.
    AlwaysPass,
    /// Every evaluated rollout failed.
    AlwaysFail,
    /// Mixed outcomes exceed the frozen instability threshold.
    Unstable,
    /// Mixed outcomes remain below the configured instability threshold.
    Mixed,
}

/// Complete ordered binary stability summary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StabilitySummary {
    passes: u32,
    failures: u32,
    transitions: u32,
    longest_pass_streak: u32,
    longest_failure_streak: u32,
    agreement: ProbabilityMillionths,
    class: StabilityClass,
}

/// Analyzes canonical ordinal-order valid task verdicts.
///
/// # Errors
/// Rejects empty input or an invalid threshold.
pub fn analyze_stability(
    outcomes: &[bool],
    instability_threshold_millionths: u32,
) -> Result<StabilitySummary, EvaluationError> {
    if outcomes.is_empty() || instability_threshold_millionths > 1_000_000 {
        return Err(crate::invalid(
            EvaluationErrorKind::Statistics,
            EvaluationOperation::Analyze,
            "stability inputs are empty or threshold exceeds one",
        ));
    }
    let passes = u32::try_from(outcomes.iter().filter(|value| **value).count()).map_err(bound)?;
    let failures = u32::try_from(outcomes.len()).map_err(bound)? - passes;
    let transitions = u32::try_from(outcomes.windows(2).filter(|pair| pair[0] != pair[1]).count())
        .map_err(bound)?;
    let (mut pass_streak, mut failure_streak, mut longest_pass, mut longest_failure) = (0, 0, 0, 0);
    for value in outcomes {
        if *value {
            pass_streak += 1;
            failure_streak = 0;
            longest_pass = longest_pass.max(pass_streak);
        } else {
            failure_streak += 1;
            pass_streak = 0;
            longest_failure = longest_failure.max(failure_streak);
        }
    }
    let majority = passes.max(failures);
    let total = passes + failures;
    let agreement_value =
        u32::try_from((u64::from(majority) * 1_000_000 + u64::from(total) / 2) / u64::from(total))
            .map_err(bound)?;
    let disagreement = 1_000_000 - agreement_value;
    let class = if failures == 0 {
        StabilityClass::AlwaysPass
    } else if passes == 0 {
        StabilityClass::AlwaysFail
    } else if disagreement >= instability_threshold_millionths {
        StabilityClass::Unstable
    } else {
        StabilityClass::Mixed
    };
    Ok(StabilitySummary {
        passes,
        failures,
        transitions,
        longest_pass_streak: longest_pass,
        longest_failure_streak: longest_failure,
        agreement: ProbabilityMillionths::new(agreement_value)?,
        class,
    })
}

impl StabilitySummary {
    /// Pass count.
    #[must_use]
    pub const fn passes(self) -> u32 {
        self.passes
    }
    /// Failure count.
    #[must_use]
    pub const fn failures(self) -> u32 {
        self.failures
    }
    /// Adjacent outcome transitions.
    #[must_use]
    pub const fn transitions(self) -> u32 {
        self.transitions
    }
    /// Longest pass streak.
    #[must_use]
    pub const fn longest_pass_streak(self) -> u32 {
        self.longest_pass_streak
    }
    /// Longest failure streak.
    #[must_use]
    pub const fn longest_failure_streak(self) -> u32 {
        self.longest_failure_streak
    }
    /// Majority agreement.
    #[must_use]
    pub const fn agreement(self) -> ProbabilityMillionths {
        self.agreement
    }
    /// Closed classification.
    #[must_use]
    pub const fn class(self) -> StabilityClass {
        self.class
    }
}

fn bound(_: impl core::fmt::Display) -> EvaluationError {
    crate::invalid(
        EvaluationErrorKind::LimitExceeded,
        EvaluationOperation::Analyze,
        "stability count exceeds supported bounds",
    )
}
