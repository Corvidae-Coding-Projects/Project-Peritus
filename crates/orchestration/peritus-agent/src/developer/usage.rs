//! Aggregate normalized usage for the product developer loop.

use peritus_model_protocol::UsageCounters;

use super::DeveloperLoopError;

/// Aggregate provider-reported usage across independent responses in one developer loop.
///
/// Missing provider counters remain zero and `observations` records whether any usage report was
/// available. `total_tokens` prefers each response's explicit total and otherwise conservatively
/// derives input plus output plus provider-tool tokens without double-counting reasoning tokens.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DeveloperUsage {
    input_tokens: u64,
    cached_input_tokens: u64,
    cache_creation_input_tokens: u64,
    output_tokens: u64,
    reasoning_output_tokens: u64,
    tool_tokens: u64,
    total_tokens: u64,
    provider_cost_microunits: u64,
    observations: u32,
}

impl DeveloperUsage {
    pub(crate) fn observe(&mut self, counters: UsageCounters) -> Result<(), DeveloperLoopError> {
        let values = [
            counters.input_tokens(),
            counters.cached_input_tokens(),
            counters.cache_creation_input_tokens(),
            counters.output_tokens(),
            counters.reasoning_output_tokens(),
            counters.tool_tokens(),
            counters.provider_cost_microunits(),
        ];
        if values.iter().all(Option::is_none) && counters.total_tokens().is_none() {
            return Ok(());
        }
        let derived_total = counters
            .input_tokens()
            .unwrap_or(0)
            .checked_add(counters.output_tokens().unwrap_or(0))
            .and_then(|value| value.checked_add(counters.tool_tokens().unwrap_or(0)))
            .ok_or(DeveloperLoopError::LimitExceeded)?;
        self.input_tokens = add(self.input_tokens, counters.input_tokens())?;
        self.cached_input_tokens = add(self.cached_input_tokens, counters.cached_input_tokens())?;
        self.cache_creation_input_tokens =
            add(self.cache_creation_input_tokens, counters.cache_creation_input_tokens())?;
        self.output_tokens = add(self.output_tokens, counters.output_tokens())?;
        self.reasoning_output_tokens =
            add(self.reasoning_output_tokens, counters.reasoning_output_tokens())?;
        self.tool_tokens = add(self.tool_tokens, counters.tool_tokens())?;
        self.total_tokens = self
            .total_tokens
            .checked_add(counters.total_tokens().unwrap_or(derived_total))
            .ok_or(DeveloperLoopError::LimitExceeded)?;
        self.provider_cost_microunits =
            add(self.provider_cost_microunits, counters.provider_cost_microunits())?;
        self.observations =
            self.observations.checked_add(1).ok_or(DeveloperLoopError::LimitExceeded)?;
        Ok(())
    }

    /// Provider-reported input tokens across responses.
    #[must_use]
    pub const fn input_tokens(self) -> u64 {
        self.input_tokens
    }
    /// Provider-reported cache-read input tokens across responses.
    #[must_use]
    pub const fn cached_input_tokens(self) -> u64 {
        self.cached_input_tokens
    }
    /// Provider-reported cache-creation input tokens across responses.
    #[must_use]
    pub const fn cache_creation_input_tokens(self) -> u64 {
        self.cache_creation_input_tokens
    }
    /// Provider-reported output tokens across responses.
    #[must_use]
    pub const fn output_tokens(self) -> u64 {
        self.output_tokens
    }
    /// Provider-reported reasoning output tokens across responses.
    #[must_use]
    pub const fn reasoning_output_tokens(self) -> u64 {
        self.reasoning_output_tokens
    }
    /// Provider-reported server tool tokens across responses.
    #[must_use]
    pub const fn tool_tokens(self) -> u64 {
        self.tool_tokens
    }
    /// Explicit or conservatively derived aggregate response tokens.
    #[must_use]
    pub const fn total_tokens(self) -> u64 {
        self.total_tokens
    }
    /// Provider-estimated cost in integer microunits across responses.
    #[must_use]
    pub const fn provider_cost_microunits(self) -> u64 {
        self.provider_cost_microunits
    }
    /// Number of provider responses that supplied at least one usage counter.
    #[must_use]
    pub const fn observations(self) -> u32 {
        self.observations
    }
}

fn add(current: u64, next: Option<u64>) -> Result<u64, DeveloperLoopError> {
    checked_add(current, next.unwrap_or(0))
}

fn checked_add(left: u64, right: u64) -> Result<u64, DeveloperLoopError> {
    left.checked_add(right).ok_or(DeveloperLoopError::LimitExceeded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn independent_response_usage_adds_without_double_counting_reasoning() {
        let mut usage = DeveloperUsage::default();
        usage
            .observe(UsageCounters::new(
                Some(100),
                Some(40),
                None,
                Some(20),
                Some(5),
                Some(3),
                None,
                Some(7),
            ))
            .expect("first response usage");
        usage
            .observe(UsageCounters::new(
                Some(50),
                None,
                None,
                Some(10),
                None,
                None,
                Some(65),
                Some(2),
            ))
            .expect("second response usage");

        assert_eq!(usage.input_tokens(), 150);
        assert_eq!(usage.cached_input_tokens(), 40);
        assert_eq!(usage.output_tokens(), 30);
        assert_eq!(usage.reasoning_output_tokens(), 5);
        assert_eq!(usage.total_tokens(), 188);
        assert_eq!(usage.provider_cost_microunits(), 9);
        assert_eq!(usage.observations(), 2);
    }
}
