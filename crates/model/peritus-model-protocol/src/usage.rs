//! Optional nonnegative usage counters and monotonic cumulative accounting.

use crate::{CanonicalJson, ProtocolError, ProtocolErrorKind};

/// Provider usage scope; step-local values are never added to cumulative snapshots implicitly.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UsageScope {
    /// Independent usage for one response step.
    Step,
    /// High-water usage for the response so far.
    Cumulative,
    /// Provider-declared final high-water usage.
    Final,
}

/// Complete optional normalized usage vocabulary.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UsageCounters {
    input_tokens: Option<u64>,
    cached_input_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    reasoning_output_tokens: Option<u64>,
    tool_tokens: Option<u64>,
    total_tokens: Option<u64>,
    provider_cost_microunits: Option<u64>,
}

impl UsageCounters {
    /// Creates counters without synthesizing missing values.
    #[allow(
        clippy::too_many_arguments,
        reason = "constructor names the full normalized counter vocabulary"
    )]
    #[must_use]
    pub const fn new(
        input_tokens: Option<u64>,
        cached_input_tokens: Option<u64>,
        cache_creation_input_tokens: Option<u64>,
        output_tokens: Option<u64>,
        reasoning_output_tokens: Option<u64>,
        tool_tokens: Option<u64>,
        total_tokens: Option<u64>,
        provider_cost_microunits: Option<u64>,
    ) -> Self {
        Self {
            input_tokens,
            cached_input_tokens,
            cache_creation_input_tokens,
            output_tokens,
            reasoning_output_tokens,
            tool_tokens,
            total_tokens,
            provider_cost_microunits,
        }
    }

    /// Input tokens when reported.
    #[must_use]
    pub const fn input_tokens(self) -> Option<u64> {
        self.input_tokens
    }
    /// Cache-read input tokens when reported.
    #[must_use]
    pub const fn cached_input_tokens(self) -> Option<u64> {
        self.cached_input_tokens
    }
    /// Cache-creation input tokens when reported.
    #[must_use]
    pub const fn cache_creation_input_tokens(self) -> Option<u64> {
        self.cache_creation_input_tokens
    }
    /// Output tokens when reported.
    #[must_use]
    pub const fn output_tokens(self) -> Option<u64> {
        self.output_tokens
    }
    /// Reasoning output tokens when reported.
    #[must_use]
    pub const fn reasoning_output_tokens(self) -> Option<u64> {
        self.reasoning_output_tokens
    }
    /// Provider/server tool-use tokens when reported.
    #[must_use]
    pub const fn tool_tokens(self) -> Option<u64> {
        self.tool_tokens
    }
    /// Total tokens when explicitly reported.
    #[must_use]
    pub const fn total_tokens(self) -> Option<u64> {
        self.total_tokens
    }
    /// Provider-estimated cost in integer microunits when reported.
    #[must_use]
    pub const fn provider_cost_microunits(self) -> Option<u64> {
        self.provider_cost_microunits
    }

    const fn fields(self) -> [Option<u64>; 8] {
        [
            self.input_tokens,
            self.cached_input_tokens,
            self.cache_creation_input_tokens,
            self.output_tokens,
            self.reasoning_output_tokens,
            self.tool_tokens,
            self.total_tokens,
            self.provider_cost_microunits,
        ]
    }

    const fn from_fields(values: [Option<u64>; 8]) -> Self {
        Self::new(
            values[0], values[1], values[2], values[3], values[4], values[5], values[6], values[7],
        )
    }
}

/// One usage observation with optional bounded provider-native detail.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UsageObservation {
    scope: UsageScope,
    counters: UsageCounters,
    provider_detail: Option<CanonicalJson>,
}

impl UsageObservation {
    /// Creates a usage observation. Raw detail remains sensitive and bounded.
    #[must_use]
    pub const fn new(
        scope: UsageScope,
        counters: UsageCounters,
        provider_detail: Option<CanonicalJson>,
    ) -> Self {
        Self { scope, counters, provider_detail }
    }

    /// Returns the observation scope.
    #[must_use]
    pub const fn scope(&self) -> UsageScope {
        self.scope
    }
    /// Returns normalized counters.
    #[must_use]
    pub const fn counters(&self) -> UsageCounters {
        self.counters
    }
    /// Borrows bounded provider-native usage detail.
    #[must_use]
    pub const fn provider_detail(&self) -> Option<&CanonicalJson> {
        self.provider_detail.as_ref()
    }
}

/// Monotonic high-water tracker for cumulative/final provider observations.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UsageTracker {
    high_water: UsageCounters,
    saw_final: bool,
}

impl UsageTracker {
    /// Creates an empty tracker.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            high_water: UsageCounters::new(None, None, None, None, None, None, None, None),
            saw_final: false,
        }
    }

    /// Applies an observation and returns the current high water.
    ///
    /// Step-scoped counters are observed but do not mutate cumulative state.
    ///
    /// # Errors
    ///
    /// Rejects cumulative regression, an update after final, or more than one final snapshot.
    pub fn observe(
        &mut self,
        observation: &UsageObservation,
    ) -> Result<UsageCounters, ProtocolError> {
        if matches!(observation.scope, UsageScope::Step) {
            return Ok(self.high_water);
        }
        if self.saw_final {
            return Err(invalid("usage observation followed the final snapshot"));
        }
        let previous = self.high_water.fields();
        let next = observation.counters.fields();
        let mut merged = [None; 8];
        for index in 0..merged.len() {
            merged[index] = match (previous[index], next[index]) {
                (Some(old), Some(new))
                    if !crate::verified::usage_counter_monotonic(true, old, true, new) =>
                {
                    return Err(invalid("cumulative usage counter regressed"));
                }
                (Some(old), None) => Some(old),
                (_, Some(new)) => Some(new),
                (None, None) => None,
            };
        }
        self.high_water = UsageCounters::from_fields(merged);
        self.saw_final = matches!(observation.scope, UsageScope::Final);
        Ok(self.high_water)
    }

    /// Returns the current cumulative high water.
    #[must_use]
    pub const fn high_water(self) -> UsageCounters {
        self.high_water
    }

    /// Returns whether a final usage snapshot was observed.
    #[must_use]
    pub const fn saw_final(self) -> bool {
        self.saw_final
    }
}

fn invalid(detail: &'static str) -> ProtocolError {
    ProtocolError::at(ProtocolErrorKind::InvalidUsage, "usage", detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_stays_unknown_and_cumulative_regression_fails() {
        let mut tracker = UsageTracker::new();
        let first = UsageObservation::new(
            UsageScope::Cumulative,
            UsageCounters::new(Some(10), None, None, Some(2), None, None, Some(12), None),
            None,
        );
        tracker.observe(&first).expect("first observation");
        assert_eq!(tracker.high_water().cached_input_tokens(), None);
        let regression = UsageObservation::new(
            UsageScope::Cumulative,
            UsageCounters::new(Some(9), None, None, None, None, None, None, None),
            None,
        );
        assert_eq!(
            tracker.observe(&regression).expect_err("regression").kind(),
            ProtocolErrorKind::InvalidUsage
        );
    }
}
