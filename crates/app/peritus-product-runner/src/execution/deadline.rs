//! Phase admission against the caller-selected deadline only.

use std::time::Duration;

use crate::{ProductRunnerError, ProductRunnerErrorKind};

/// Open-ended work phases governed by the same explicit run deadline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum OpenEndedPhase {
    Design,
    Writer,
    Reviewer,
    Fixer,
}

/// Rejects a new phase only when its explicit deadline has actually elapsed.
/// Settlement of already owned work runs separately from phase admission.
pub(super) fn require_phase_window(
    horizon: Option<Duration>,
    remaining: Option<Duration>,
    phase: OpenEndedPhase,
) -> Result<(), ProductRunnerError> {
    let remaining = match (horizon, remaining) {
        (None, None) => return Ok(()),
        (Some(_), Some(remaining)) => remaining,
        _ => {
            return Err(ProductRunnerError::new(
                ProductRunnerErrorKind::InvalidPrecondition,
                "start open-ended product phase",
                "the configured run horizon and live accounting deadline disagree",
            ));
        }
    };
    if remaining.is_zero() {
        return Err(ProductRunnerError::new(
            ProductRunnerErrorKind::Budget,
            "start open-ended product phase",
            format!(
                "the {phase:?} phase was not started because the explicitly selected run deadline elapsed"
            ),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_positive_explicit_window_admits_every_phase() {
        let horizon = Duration::from_secs(600);
        for phase in [
            OpenEndedPhase::Design,
            OpenEndedPhase::Writer,
            OpenEndedPhase::Reviewer,
            OpenEndedPhase::Fixer,
        ] {
            assert!(require_phase_window(Some(horizon), Some(horizon), phase).is_ok());
            assert!(
                require_phase_window(Some(horizon), Some(Duration::from_nanos(1)), phase).is_ok(),
            );
        }
    }

    #[test]
    fn only_an_elapsed_explicit_deadline_rejects_a_phase() {
        let horizon = Duration::from_secs(100);
        for phase in [
            OpenEndedPhase::Design,
            OpenEndedPhase::Writer,
            OpenEndedPhase::Reviewer,
            OpenEndedPhase::Fixer,
        ] {
            assert!(require_phase_window(Some(horizon), Some(Duration::ZERO), phase).is_err());
        }
    }

    #[test]
    fn untimed_runs_are_admitted_and_deadline_state_must_match() {
        let horizon = Duration::from_secs(100);
        assert!(require_phase_window(None, None, OpenEndedPhase::Writer).is_ok());
        assert!(
            require_phase_window(Some(horizon), None, OpenEndedPhase::Writer).is_err(),
            "a mismatched horizon must fail closed",
        );
        assert!(
            require_phase_window(None, Some(horizon), OpenEndedPhase::Writer).is_err(),
            "live deadline accounting without a configured horizon must fail closed",
        );
    }
}
