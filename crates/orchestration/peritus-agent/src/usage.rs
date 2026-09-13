//! Shared numeric usage state and exact overflow-rejecting accumulation.

use vstd::prelude::*;

verus! {

/// Aggregate provider-reported usage across independent responses in one developer loop.
///
/// Missing counters contribute zero. Each nonempty observation uses its explicit total when
/// present, otherwise input plus output plus provider-tool tokens; reasoning is not counted twice.
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

/// Exact optional scalar projection of one normalized provider usage observation.
pub struct UsageObservation {
    pub(crate) input_tokens: Option<u64>,
    pub(crate) cached_input_tokens: Option<u64>,
    pub(crate) cache_creation_input_tokens: Option<u64>,
    pub(crate) output_tokens: Option<u64>,
    pub(crate) reasoning_output_tokens: Option<u64>,
    pub(crate) tool_tokens: Option<u64>,
    pub(crate) total_tokens: Option<u64>,
    pub(crate) provider_cost_microunits: Option<u64>,
}

impl UsageObservation {
    pub(crate) closed spec fn present(&self) -> bool {
        self.input_tokens.is_some() ||
        self.cached_input_tokens.is_some() ||
        self.cache_creation_input_tokens.is_some() ||
        self.output_tokens.is_some() ||
        self.reasoning_output_tokens.is_some() ||
        self.tool_tokens.is_some() ||
        self.total_tokens.is_some() ||
        self.provider_cost_microunits.is_some()
    }

    pub(crate) closed spec fn derived_total(&self) -> int {
        optional_value(self.input_tokens) + optional_value(self.output_tokens)
            + optional_value(self.tool_tokens)
    }

    pub(crate) closed spec fn total(&self) -> int {
        match self.total_tokens {
            Some(total) => total as int,
            None => self.derived_total(),
        }
    }
}

spec fn optional_value(value: Option<u64>) -> int {
    match value {
        Some(value) => value as int,
        None => 0,
    }
}

impl DeveloperUsage {
    /// Mathematical projection of the retained input tokens.
    pub closed spec fn spec_input_tokens(self) -> u64 {
        self.input_tokens
    }

    /// Mathematical projection of the retained cached input tokens.
    pub closed spec fn spec_cached_input_tokens(self) -> u64 {
        self.cached_input_tokens
    }

    /// Mathematical projection of the retained cache creation input tokens.
    pub closed spec fn spec_cache_creation_input_tokens(self) -> u64 {
        self.cache_creation_input_tokens
    }

    /// Mathematical projection of the retained output tokens.
    pub closed spec fn spec_output_tokens(self) -> u64 {
        self.output_tokens
    }

    /// Mathematical projection of the retained reasoning output tokens.
    pub closed spec fn spec_reasoning_output_tokens(self) -> u64 {
        self.reasoning_output_tokens
    }

    /// Mathematical projection of the retained tool tokens.
    pub closed spec fn spec_tool_tokens(self) -> u64 {
        self.tool_tokens
    }

    /// Mathematical projection of the retained total tokens.
    pub closed spec fn spec_total_tokens(self) -> u64 {
        self.total_tokens
    }

    /// Mathematical projection of the retained provider cost microunits.
    pub closed spec fn spec_provider_cost_microunits(self) -> u64 {
        self.provider_cost_microunits
    }

    /// Mathematical projection of the retained observations.
    pub closed spec fn spec_observations(self) -> u32 {
        self.observations
    }

    pub(crate) closed spec fn accepts(&self, observation: &UsageObservation) -> bool {
        !observation.present() || (
            observation.derived_total() <= u64::MAX as int
            && self.input_tokens as int + optional_value(observation.input_tokens) <= u64::MAX as int
            && self.cached_input_tokens as int + optional_value(observation.cached_input_tokens) <= u64::MAX as int
            && self.cache_creation_input_tokens as int + optional_value(observation.cache_creation_input_tokens) <= u64::MAX as int
            && self.output_tokens as int + optional_value(observation.output_tokens) <= u64::MAX as int
            && self.reasoning_output_tokens as int + optional_value(observation.reasoning_output_tokens) <= u64::MAX as int
            && self.tool_tokens as int + optional_value(observation.tool_tokens) <= u64::MAX as int
            && self.total_tokens as int + observation.total() <= u64::MAX as int
            && self.provider_cost_microunits as int + optional_value(observation.provider_cost_microunits) <= u64::MAX as int
            && self.observations < u32::MAX
        )
    }

    pub(crate) closed spec fn is_update_of(
        &self,
        before: &Self,
        observation: &UsageObservation,
    ) -> bool {
        if !observation.present() {
            *self == *before
        } else {
            self.input_tokens as int == before.input_tokens as int + optional_value(observation.input_tokens)
            && self.cached_input_tokens as int == before.cached_input_tokens as int + optional_value(observation.cached_input_tokens)
            && self.cache_creation_input_tokens as int == before.cache_creation_input_tokens as int + optional_value(observation.cache_creation_input_tokens)
            && self.output_tokens as int == before.output_tokens as int + optional_value(observation.output_tokens)
            && self.reasoning_output_tokens as int == before.reasoning_output_tokens as int + optional_value(observation.reasoning_output_tokens)
            && self.tool_tokens as int == before.tool_tokens as int + optional_value(observation.tool_tokens)
            && self.total_tokens as int == before.total_tokens as int + observation.total()
            && self.provider_cost_microunits as int == before.provider_cost_microunits as int + optional_value(observation.provider_cost_microunits)
            && self.observations as int == before.observations as int + 1
        }
    }

    /// Publishes a complete accepted observation, leaving every counter unchanged on rejection.
    pub(crate) fn apply_observation(
        &mut self,
        observation: &UsageObservation,
    ) -> (applied: bool)
        ensures
            applied == old(self).accepts(observation),
            applied ==> final(self).is_update_of(old(self), observation),
            !applied ==> *final(self) == *old(self),
    {
        let Some(next) = self.checked_observation(observation) else {
            return false;
        };
        *self = next;
        true
    }

    /// Computes the complete next state; no partial state escapes an overflow rejection.
    pub(crate) fn checked_observation(
        self,
        observation: &UsageObservation,
    ) -> (next: Option<Self>)
        ensures
            next.is_some() == self.accepts(observation),
            match next {
                Some(after) => after.is_update_of(&self, observation),
                None => true,
            },
    {
        let present = observation.input_tokens.is_some()
            || observation.cached_input_tokens.is_some()
            || observation.cache_creation_input_tokens.is_some()
            || observation.output_tokens.is_some()
            || observation.reasoning_output_tokens.is_some()
            || observation.tool_tokens.is_some()
            || observation.total_tokens.is_some()
            || observation.provider_cost_microunits.is_some();
        if !present {
            return Some(self);
        }
        // Preserve the existing validation of derived counters even with an explicit total.
        let input_output = observation.input_tokens.unwrap_or(0)
            .checked_add(observation.output_tokens.unwrap_or(0))?;
        let derived_total = input_output.checked_add(observation.tool_tokens.unwrap_or(0))?;
        let input_tokens = self.input_tokens.checked_add(observation.input_tokens.unwrap_or(0))?;
        let cached_input_tokens = self.cached_input_tokens.checked_add(observation.cached_input_tokens.unwrap_or(0))?;
        let cache_creation_input_tokens = self.cache_creation_input_tokens.checked_add(observation.cache_creation_input_tokens.unwrap_or(0))?;
        let output_tokens = self.output_tokens.checked_add(observation.output_tokens.unwrap_or(0))?;
        let reasoning_output_tokens = self.reasoning_output_tokens.checked_add(observation.reasoning_output_tokens.unwrap_or(0))?;
        let tool_tokens = self.tool_tokens.checked_add(observation.tool_tokens.unwrap_or(0))?;
        let total_tokens = self.total_tokens.checked_add(observation.total_tokens.unwrap_or(derived_total))?;
        let provider_cost_microunits = self.provider_cost_microunits.checked_add(observation.provider_cost_microunits.unwrap_or(0))?;
        let observations = self.observations.checked_add(1)?;
        Some(Self {
            input_tokens,
            cached_input_tokens,
            cache_creation_input_tokens,
            output_tokens,
            reasoning_output_tokens,
            tool_tokens,
            total_tokens,
            provider_cost_microunits,
            observations,
        })
    }

    /// Provider-reported input tokens across responses.
    #[must_use]
    pub const fn input_tokens(self) -> (value: u64)
        ensures value == self.spec_input_tokens(),
    {
        self.input_tokens
    }

    /// Provider-reported cache-read input tokens across responses.
    #[must_use]
    pub const fn cached_input_tokens(self) -> (value: u64)
        ensures value == self.spec_cached_input_tokens(),
    {
        self.cached_input_tokens
    }

    /// Provider-reported cache-creation input tokens across responses.
    #[must_use]
    pub const fn cache_creation_input_tokens(self) -> (value: u64)
        ensures value == self.spec_cache_creation_input_tokens(),
    {
        self.cache_creation_input_tokens
    }

    /// Provider-reported output tokens across responses.
    #[must_use]
    pub const fn output_tokens(self) -> (value: u64)
        ensures value == self.spec_output_tokens(),
    {
        self.output_tokens
    }

    /// Provider-reported reasoning output tokens across responses.
    #[must_use]
    pub const fn reasoning_output_tokens(self) -> (value: u64)
        ensures value == self.spec_reasoning_output_tokens(),
    {
        self.reasoning_output_tokens
    }

    /// Provider-reported server tool tokens across responses.
    #[must_use]
    pub const fn tool_tokens(self) -> (value: u64)
        ensures value == self.spec_tool_tokens(),
    {
        self.tool_tokens
    }

    /// Explicit or conservatively derived aggregate response tokens.
    #[must_use]
    pub const fn total_tokens(self) -> (value: u64)
        ensures value == self.spec_total_tokens(),
    {
        self.total_tokens
    }

    /// Provider-estimated cost in integer microunits across responses.
    #[must_use]
    pub const fn provider_cost_microunits(self) -> (value: u64)
        ensures value == self.spec_provider_cost_microunits(),
    {
        self.provider_cost_microunits
    }

    /// Number of provider observations that supplied at least one usage counter.
    #[must_use]
    pub const fn observations(self) -> (value: u32)
        ensures value == self.spec_observations(),
    {
        self.observations
    }
}

} // verus!
