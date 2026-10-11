//! Phase-aware run admission with time reserved for honest finalization.

use std::time::Duration;

use crate::{ProductRunnerError, ProductRunnerErrorKind};

const MAX_FINALIZATION_RESERVE: Duration = Duration::from_mins(1);
const MIN_FINALIZATION_RESERVE: Duration = Duration::from_secs(1);

/// Open-ended work phases that may not consume the protected finalization reserve.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum OpenEndedPhase {
    Design,
    Writer,
    Reviewer,
    Fixer,
}

/// Portion of a run horizon available for design or model work.
#[must_use]
pub(super) fn active_window(horizon: Duration) -> Duration {
    horizon.saturating_sub(finalization_reserve(horizon))
}

/// Rejects a new open-ended turn once only the finalization reserve remains.
pub(super) fn require_phase_window(
    horizon: Option<Duration>,
    remaining: Option<Duration>,
    phase: OpenEndedPhase,
) -> Result<(), ProductRunnerError> {
    let (horizon, remaining) = match (horizon, remaining) {
        (None, None) => return Ok(()),
        (Some(horizon), Some(remaining)) => (horizon, remaining),
        _ => {
            return Err(ProductRunnerError::new(
                ProductRunnerErrorKind::InvalidPrecondition,
                "start open-ended product phase",
                "the configured run horizon and live accounting deadline disagree",
            ));
        }
    };
    if remaining <= finalization_reserve(horizon) {
        return Err(ProductRunnerError::new(
            ProductRunnerErrorKind::Budget,
            "start open-ended product phase",
            format!(
                "the {phase:?} phase was not started because the protected finalization reserve is active"
            ),
        ));
    }
    Ok(())
}

#[must_use]
pub(super) fn finalization_reserve(horizon: Duration) -> Duration {
    let proportional = horizon / 10;
    proportional.clamp(MIN_FINALIZATION_RESERVE.min(horizon), MAX_FINALIZATION_RESERVE.min(horizon))
}
