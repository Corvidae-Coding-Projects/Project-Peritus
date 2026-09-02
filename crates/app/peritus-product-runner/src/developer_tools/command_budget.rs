//! Live command allowances derived from the enclosing product-run horizon.

use std::time::{Duration, Instant};

use serde_json::Value;

use super::wire::object;

const MAX_COMPLETION_RESERVE_SECONDS: u64 = 300;

pub(super) struct CommandBudget {
    started: Instant,
    horizon: Duration,
    completion_reserve: Duration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CommandAllowance {
    pub(super) requested_seconds: u64,
    pub(super) timeout_seconds: u64,
    pub(super) deadline_limited: bool,
    pub(super) remaining_product_seconds: u64,
    pub(super) completion_reserve_seconds: u64,
}

impl CommandAllowance {
    pub(super) fn exhausted_result(self) -> Value {
        object(vec![
            ("success", Value::Bool(false)),
            ("exit_code", Value::Null),
            ("stdout", Value::String(String::new())),
            ("stderr", Value::String(String::new())),
            ("timed_out", Value::Bool(false)),
            ("requested_timeout_seconds", Value::from(self.requested_seconds)),
            ("timeout_seconds", Value::from(0_u64)),
            ("deadline_limited", Value::Bool(true)),
            ("remaining_product_seconds", Value::from(self.remaining_product_seconds)),
            ("completion_reserve_seconds", Value::from(self.completion_reserve_seconds)),
            (
                "recovery_hint",
                Value::String(
                    "The command was not started because only the product completion reserve \
                     remains. Use existing evidence, write the deliverable if needed, and finish \
                     with the best verification already available."
                        .to_owned(),
                ),
            ),
        ])
    }
}

impl CommandBudget {
    pub(super) fn new(horizon: Duration) -> Self {
        Self { started: Instant::now(), horizon, completion_reserve: completion_reserve(horizon) }
    }

    pub(super) fn allowance(&self, requested_seconds: u64) -> CommandAllowance {
        self.allowance_after(requested_seconds, self.started.elapsed())
    }

    fn allowance_after(&self, requested_seconds: u64, elapsed: Duration) -> CommandAllowance {
        let remaining = self.horizon.saturating_sub(elapsed);
        let available = remaining.saturating_sub(self.completion_reserve).as_secs();
        let timeout_seconds = requested_seconds.min(available);
        CommandAllowance {
            requested_seconds,
            timeout_seconds,
            deadline_limited: timeout_seconds < requested_seconds,
            remaining_product_seconds: remaining.as_secs(),
            completion_reserve_seconds: self.completion_reserve.as_secs(),
        }
    }

    pub(super) fn remaining_seconds(&self) -> u64 {
        self.horizon.saturating_sub(self.started.elapsed()).as_secs()
    }
}

fn completion_reserve(horizon: Duration) -> Duration {
    let horizon_seconds = horizon.as_secs();
    if horizon_seconds == 0 {
        return Duration::ZERO;
    }
    let proportional = (horizon_seconds / 5).max(1);
    Duration::from_secs(proportional.min(MAX_COMPLETION_RESERVE_SECONDS).min(horizon_seconds))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_command_keeps_its_requested_timeout() {
        let budget = CommandBudget::new(Duration::from_hours(1));

        let allowance = budget.allowance_after(120, Duration::from_secs(10));

        assert_eq!(allowance.timeout_seconds, 120);
        assert!(!allowance.deadline_limited);
        assert_eq!(allowance.remaining_product_seconds, 3_590);
        assert_eq!(allowance.completion_reserve_seconds, 300);
    }

    #[test]
    fn late_command_is_clamped_before_the_completion_reserve() {
        let budget = CommandBudget::new(Duration::from_mins(27));

        let allowance = budget.allowance_after(600, Duration::from_secs(1_240));

        assert_eq!(allowance.timeout_seconds, 80);
        assert!(allowance.deadline_limited);
        assert_eq!(allowance.remaining_product_seconds, 380);
        assert_eq!(allowance.completion_reserve_seconds, 300);
    }

    #[test]
    fn command_is_refused_once_only_the_completion_reserve_remains() {
        let budget = CommandBudget::new(Duration::from_secs(30));

        let allowance = budget.allowance_after(10, Duration::from_secs(24));

        assert_eq!(allowance.timeout_seconds, 0);
        assert!(allowance.deadline_limited);
        assert_eq!(allowance.remaining_product_seconds, 6);
        assert_eq!(allowance.completion_reserve_seconds, 6);
    }
}
