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
    horizon: Duration,
    remaining: Duration,
    phase: OpenEndedPhase,
) -> Result<(), ProductRunnerError> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_window_always_preserves_a_finalization_reserve() {
        let horizon = Duration::from_mins(10);
        assert_eq!(finalization_reserve(horizon), Duration::from_mins(1));
        assert_eq!(active_window(horizon), Duration::from_mins(9));
    }

    #[test]
    fn model_turns_stop_before_finalization_time_is_consumed() {
        let horizon = Duration::from_secs(100);
        assert!(
            require_phase_window(horizon, Duration::from_secs(11), OpenEndedPhase::Writer).is_ok()
        );
        assert!(
            require_phase_window(horizon, Duration::from_secs(10), OpenEndedPhase::Writer).is_err()
        );
    }
}
