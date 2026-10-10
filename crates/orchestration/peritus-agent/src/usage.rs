//! Shared numeric usage state and exact overflow-rejecting accumulation.

#[doc = " Aggregate provider-reported usage across independent responses in one developer loop."]
#[doc = ""]
#[doc = " Missing counters contribute zero. Each nonempty observation uses its explicit total when"]
#[doc = " present, otherwise input plus output plus provider-tool tokens; reasoning is not counted twice."]
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
#[doc = " Exact optional scalar projection of one normalized provider usage observation."]
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
impl UsageObservation {}
impl DeveloperUsage {
    #[doc = " Publishes a complete accepted observation, leaving every counter unchanged on rejection."]
    pub(crate) fn apply_observation(&mut self, observation: &UsageObservation) -> bool {
        let Some(next) = self.checked_observation(observation) else {
            return false;
        };
        *self = next;
        true
    }
    #[doc = " Computes the complete next state; no partial state escapes an overflow rejection."]
    pub(crate) fn checked_observation(self, observation: &UsageObservation) -> Option<Self> {
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
        let input_output = observation
            .input_tokens
            .unwrap_or(0)
            .checked_add(observation.output_tokens.unwrap_or(0))?;
        let derived_total = input_output.checked_add(observation.tool_tokens.unwrap_or(0))?;
        let input_tokens = self.input_tokens.checked_add(observation.input_tokens.unwrap_or(0))?;
        let cached_input_tokens =
            self.cached_input_tokens.checked_add(observation.cached_input_tokens.unwrap_or(0))?;
        let cache_creation_input_tokens = self
            .cache_creation_input_tokens
            .checked_add(observation.cache_creation_input_tokens.unwrap_or(0))?;
        let output_tokens =
            self.output_tokens.checked_add(observation.output_tokens.unwrap_or(0))?;
        let reasoning_output_tokens = self
            .reasoning_output_tokens
            .checked_add(observation.reasoning_output_tokens.unwrap_or(0))?;
        let tool_tokens = self.tool_tokens.checked_add(observation.tool_tokens.unwrap_or(0))?;
        let total_tokens =
            self.total_tokens.checked_add(observation.total_tokens.unwrap_or(derived_total))?;
        let provider_cost_microunits = self
            .provider_cost_microunits
            .checked_add(observation.provider_cost_microunits.unwrap_or(0))?;
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
    #[doc = " Provider-reported input tokens across responses."]
    #[must_use]
    pub const fn input_tokens(self) -> u64 {
        self.input_tokens
    }
    #[doc = " Provider-reported cache-read input tokens across responses."]
    #[must_use]
    pub const fn cached_input_tokens(self) -> u64 {
        self.cached_input_tokens
    }
    #[doc = " Provider-reported cache-creation input tokens across responses."]
    #[must_use]
    pub const fn cache_creation_input_tokens(self) -> u64 {
        self.cache_creation_input_tokens
    }
    #[doc = " Provider-reported output tokens across responses."]
    #[must_use]
    pub const fn output_tokens(self) -> u64 {
        self.output_tokens
    }
    #[doc = " Provider-reported reasoning output tokens across responses."]
    #[must_use]
    pub const fn reasoning_output_tokens(self) -> u64 {
        self.reasoning_output_tokens
    }
    #[doc = " Provider-reported server tool tokens across responses."]
    #[must_use]
    pub const fn tool_tokens(self) -> u64 {
        self.tool_tokens
    }
    #[doc = " Explicit or conservatively derived aggregate response tokens."]
    #[must_use]
    pub const fn total_tokens(self) -> u64 {
        self.total_tokens
    }
    #[doc = " Provider-estimated cost in integer microunits across responses."]
    #[must_use]
    pub const fn provider_cost_microunits(self) -> u64 {
        self.provider_cost_microunits
    }
    #[doc = " Number of provider observations that supplied at least one usage counter."]
    #[must_use]
    pub const fn observations(self) -> u32 {
        self.observations
    }
}
