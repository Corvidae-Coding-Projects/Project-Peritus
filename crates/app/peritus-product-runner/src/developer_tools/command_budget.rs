//! Live command deadlines derived from caller-selected command and product limits.

use std::time::{Duration, Instant};

use serde_json::Value;

use super::wire::object;

pub(super) struct CommandBudget {
    started: Instant,
    horizon: Option<Duration>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CommandDeadlineSource {
    Untimed,
    RequestedCommand,
    Product,
}

impl CommandDeadlineSource {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Untimed => "none",
            Self::RequestedCommand => "requested_command",
            Self::Product => "product_deadline",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CommandAllowance {
    pub(super) requested_seconds: Option<u64>,
    pub(super) timeout: Option<Duration>,
    pub(super) deadline_source: CommandDeadlineSource,
    pub(super) remaining_product: Option<Duration>,
}

impl CommandAllowance {
    pub(super) fn starts_execution(self) -> bool {
        self.timeout.is_none_or(|timeout| timeout.as_millis() > 0)
    }

    pub(super) const fn deadline_limited(self) -> bool {
        matches!(self.deadline_source, CommandDeadlineSource::Product)
    }

    pub(super) fn timeout_millis(self) -> Option<u64> {
        self.timeout.and_then(representable_millis)
    }

    pub(super) fn exhausted_result(self) -> Value {
        let recovery_hint = if self.remaining_product.is_some_and(Duration::is_zero) {
            "The command was not started because the caller-selected product deadline elapsed. \
             Active execution has stopped; finalization retains unfinished obligations for an \
             authorized resume."
        } else {
            "The command was not started because the caller-selected product deadline has less \
             than one representable millisecond remaining. Active execution has stopped; \
             finalization retains unfinished obligations for an authorized resume."
        };
        object(vec![
            ("success", Value::Bool(false)),
            ("exit_code", Value::Null),
            ("stdout", Value::String(String::new())),
            ("stderr", Value::String(String::new())),
            ("timed_out", Value::Bool(false)),
            ("requested_timeout_seconds", self.requested_seconds.map_or(Value::Null, Value::from)),
            (
                "timeout_seconds",
                self.timeout.map_or(Value::Null, |timeout| Value::from(timeout.as_secs())),
            ),
            (
                "timeout_millis",
                self.timeout_millis().map_or(Value::Null, Value::from),
            ),
            (
                "deadline_source",
                Value::String(self.deadline_source.label().to_owned()),
            ),
            ("deadline_limited", Value::Bool(self.deadline_limited())),
            (
                "remaining_product_seconds",
                self.remaining_product
                    .map_or(Value::Null, |remaining| Value::from(remaining.as_secs())),
            ),
            (
                "remaining_product_millis",
                self.remaining_product
                    .and_then(representable_millis)
                    .map_or(Value::Null, Value::from),
            ),
            ("recovery_hint", Value::String(recovery_hint.to_owned())),
        ])
    }
}

impl CommandBudget {
    pub(super) fn new(horizon: Option<Duration>) -> Self {
        Self { started: Instant::now(), horizon }
    }

    pub(super) fn allowance(&self, requested_seconds: Option<u64>) -> CommandAllowance {
        self.allowance_after(requested_seconds, self.started.elapsed())
    }

    fn allowance_after(
        &self,
        requested_seconds: Option<u64>,
        elapsed: Duration,
    ) -> CommandAllowance {
        let remaining = self.horizon.map(|horizon| horizon.saturating_sub(elapsed));
        let requested = requested_seconds.map(Duration::from_secs);
        let (timeout, deadline_source) = match (remaining, requested) {
            (Some(remaining), Some(requested)) if remaining < requested => {
                (Some(remaining), CommandDeadlineSource::Product)
            }
            (_, Some(requested)) => {
                (Some(requested), CommandDeadlineSource::RequestedCommand)
            }
            (Some(remaining), None) => (Some(remaining), CommandDeadlineSource::Product),
            (None, None) => (None, CommandDeadlineSource::Untimed),
        };
        CommandAllowance {
            requested_seconds,
            timeout,
            deadline_source,
            remaining_product: remaining,
        }
    }

    pub(super) fn remaining(&self) -> Option<Duration> {
        self.horizon.map(|horizon| horizon.saturating_sub(self.started.elapsed()))
    }
}

fn representable_millis(duration: Duration) -> Option<u64> {
    u64::try_from(duration.as_millis()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_command_keeps_its_requested_timeout() {
        let budget = CommandBudget::new(Some(Duration::from_hours(1)));

        let allowance = budget.allowance_after(Some(120), Duration::from_secs(10));

        assert_eq!(allowance.timeout_seconds, Some(120));
        assert!(!allowance.deadline_limited);
        assert_eq!(allowance.remaining_product_seconds, Some(3_590));
        assert_eq!(allowance.completion_reserve_seconds, 300);
    }

    #[test]
    fn late_command_is_clamped_before_the_completion_reserve() {
        let budget = CommandBudget::new(Some(Duration::from_mins(27)));

        let allowance = budget.allowance_after(Some(600), Duration::from_secs(1_240));

        assert_eq!(allowance.timeout_seconds, Some(80));
        assert!(allowance.deadline_limited);
        assert_eq!(allowance.remaining_product_seconds, Some(380));
        assert_eq!(allowance.completion_reserve_seconds, 300);
    }

    #[test]
    fn command_is_refused_once_only_the_completion_reserve_remains() {
        let budget = CommandBudget::new(Some(Duration::from_secs(30)));

        let allowance = budget.allowance_after(Some(10), Duration::from_secs(24));

        assert_eq!(allowance.timeout_seconds, Some(0));
        assert!(allowance.deadline_limited);
        assert_eq!(allowance.remaining_product_seconds, Some(6));
        assert_eq!(allowance.completion_reserve_seconds, 6);
    }

    #[test]
    fn unbounded_run_preserves_the_requested_command_timeout() {
        let budget = CommandBudget::new(None);

        let allowance = budget.allowance_after(Some(600), Duration::from_hours(24));

        assert_eq!(allowance.timeout_seconds, Some(600));
        assert!(!allowance.deadline_limited);
        assert_eq!(allowance.remaining_product_seconds, None);
        assert_eq!(allowance.completion_reserve_seconds, 0);
    }
}
