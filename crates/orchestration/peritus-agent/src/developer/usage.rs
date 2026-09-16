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

    #[test]
    fn rejected_usage_preserves_every_previously_accepted_counter() {
        let mut usage = DeveloperUsage::default();
        usage
            .observe(UsageCounters::new(
                None,
                None,
                None,
                None,
                None,
                None,
                Some(1),
                Some(u64::MAX),
            ))
            .expect("initial usage");
        let before = usage;

        assert!(matches!(
            usage.observe(UsageCounters::new(
                Some(4),
                Some(3),
                Some(2),
                Some(5),
                Some(1),
                Some(6),
                Some(15),
                Some(1),
            )),
            Err(DeveloperLoopError::LimitExceeded)
        ));
        assert_eq!(usage, before);
    }

    #[test]
    fn missing_usage_is_inert_but_an_explicit_zero_is_an_observation() {
        let mut usage = DeveloperUsage::default();
        usage.observe(UsageCounters::default()).expect("absent usage");
        assert_eq!(usage, DeveloperUsage::default());
        usage
            .observe(UsageCounters::new(None, None, None, None, None, None, Some(0), None))
            .expect("explicit zero usage");
        assert_eq!(usage.total_tokens(), 0);
        assert_eq!(usage.observations(), 1);
    }

    #[test]
    fn explicit_total_preserves_derived_counter_overflow_rejection() {
        let mut usage = DeveloperUsage::default();
        assert!(matches!(
            usage.observe(UsageCounters::new(
                Some(u64::MAX),
                None,
                None,
                Some(1),
                None,
                None,
                Some(0),
                None,
            )),
            Err(DeveloperLoopError::LimitExceeded)
        ));
        assert_eq!(usage, DeveloperUsage::default());
    }
}
