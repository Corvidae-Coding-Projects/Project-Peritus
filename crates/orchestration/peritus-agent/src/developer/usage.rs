//! Ordinary provider-counter adaptation for the shared verified usage state.

use peritus_model_protocol::UsageCounters;

use super::DeveloperLoopError;
use crate::usage::{DeveloperUsage, UsageObservation};

impl DeveloperUsage {
    /// Adds one response's normalized high-water counters without counting reasoning twice.
    ///
    /// # Errors
    /// Returns a limit failure without changing any counters if an aggregate counter overflows.
    pub fn observe(&mut self, counters: UsageCounters) -> Result<(), DeveloperLoopError> {
        let observation = UsageObservation {
            input_tokens: counters.input_tokens(),
            cached_input_tokens: counters.cached_input_tokens(),
            cache_creation_input_tokens: counters.cache_creation_input_tokens(),
            output_tokens: counters.output_tokens(),
            reasoning_output_tokens: counters.reasoning_output_tokens(),
            tool_tokens: counters.tool_tokens(),
            total_tokens: counters.total_tokens(),
            provider_cost_microunits: counters.provider_cost_microunits(),
        };
        if self.apply_observation(&observation) {
            Ok(())
        } else {
            Err(DeveloperLoopError::LimitExceeded)
        }
    }
}
