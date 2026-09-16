//! Exact replacement of the current response's contribution to cumulative usage.

use super::{AccountingError, AccountingState};
use vstd::prelude::*;

verus! {

/// The numeric provider usage fields retained by product accounting.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UsageSnapshot {
    pub(crate) input_tokens: u64,
    pub(crate) cached_input_tokens: u64,
    pub(crate) output_tokens: u64,
    pub(crate) total_tokens: u64,
    pub(crate) provider_cost_microunits: u64,
    pub(crate) observations: u32,
}

impl UsageSnapshot {
    pub(crate) const fn empty() -> (snapshot: Self)
        ensures snapshot.input_tokens == 0,
            snapshot.cached_input_tokens == 0,
            snapshot.output_tokens == 0,
            snapshot.total_tokens == 0,
            snapshot.provider_cost_microunits == 0,
            snapshot.observations == 0,
    {
        Self { input_tokens: 0, cached_input_tokens: 0, output_tokens: 0, total_tokens: 0, provider_cost_microunits: 0, observations: 0 }
    }
}

spec fn replacement_fits(total: u64, previous: u64, current: u64) -> bool {
    previous <= total && total as int - previous as int + current as int <= u64::MAX as int
}

const fn replace(total: u64, previous: u64, current: u64) -> (result: Result<u64, AccountingError>)
    ensures
        result.is_ok() == replacement_fits(total, previous, current),
        match result {
            Ok(value) => value as int == total as int - previous as int + current as int,
            Err(error) => error == AccountingError::CounterOverflow,
        },
{
    let Some(remainder) = total.checked_sub(previous) else {
        return Err(AccountingError::CounterOverflow);
    };
    let Some(value) = remainder.checked_add(current) else {
        return Err(AccountingError::CounterOverflow);
    };
    Ok(value)
}

impl AccountingState {
    pub(crate) closed spec fn numeric_usage_fits(self, current: UsageSnapshot) -> bool {
        replacement_fits(self.progress.input_tokens, self.response_usage.input_tokens, current.input_tokens)
            && replacement_fits(self.progress.cached_input_tokens, self.response_usage.cached_input_tokens, current.cached_input_tokens)
            && replacement_fits(self.progress.output_tokens, self.response_usage.output_tokens, current.output_tokens)
            && replacement_fits(self.progress.total_tokens, self.response_usage.total_tokens, current.total_tokens)
            && replacement_fits(self.progress.provider_cost_microunits, self.response_usage.provider_cost_microunits, current.provider_cost_microunits)
    }

    pub(crate) closed spec fn observation_count_fits(self, current: UsageSnapshot) -> bool {
        self.response_usage.observations <= self.progress.usage_observations
            && self.progress.usage_observations as int - self.response_usage.observations as int
                + current.observations as int <= u32::MAX as int
    }

    pub(crate) closed spec fn usage_fits(self, current: UsageSnapshot) -> bool {
        self.numeric_usage_fits(current) && self.observation_count_fits(current)
    }

    pub(crate) closed spec fn is_usage_update_of(
        self, before: Self, current: UsageSnapshot,
    ) -> bool {
        self.response_usage == current
            && self.progress.input_tokens as int == before.progress.input_tokens as int
                - before.response_usage.input_tokens as int + current.input_tokens as int
            && self.progress.cached_input_tokens as int == before.progress.cached_input_tokens as int
                - before.response_usage.cached_input_tokens as int + current.cached_input_tokens as int
            && self.progress.output_tokens as int == before.progress.output_tokens as int
                - before.response_usage.output_tokens as int + current.output_tokens as int
            && self.progress.total_tokens as int == before.progress.total_tokens as int
                - before.response_usage.total_tokens as int + current.total_tokens as int
            && self.progress.provider_cost_microunits as int == before.progress.provider_cost_microunits as int
                - before.response_usage.provider_cost_microunits as int + current.provider_cost_microunits as int
            && self.progress.usage_observations as int == before.progress.usage_observations as int
                - before.response_usage.observations as int + current.observations as int
            && self.progress.model_requests == before.progress.model_requests
            && self.progress.tool_calls == before.progress.tool_calls
            && self.progress.retries == before.progress.retries
            && self.progress.provider_failovers == before.progress.provider_failovers
            && self.progress.compactions == before.progress.compactions
            && self.progress.elapsed_millis == before.progress.elapsed_millis
            && self.progress.workspace_bytes == before.progress.workspace_bytes
            && self.progress.workspace_growth_bytes == before.progress.workspace_growth_bytes
            && self.progress.peak_rss_bytes == before.progress.peak_rss_bytes
    }

    fn checked_usage(self, current: UsageSnapshot) -> (result: Result<Self, AccountingError>)
        ensures
            result.is_ok() == self.usage_fits(current),
            match result {
                Ok(next) => next.is_usage_update_of(self, current),
                Err(AccountingError::CounterOverflow) => !self.numeric_usage_fits(current),
                Err(AccountingError::ObservationOverflow) => self.numeric_usage_fits(current)
                    && !self.observation_count_fits(current),
            },
    {
        let mut next = self;
        next.progress.input_tokens = replace(self.progress.input_tokens, self.response_usage.input_tokens, current.input_tokens)?;
        next.progress.cached_input_tokens = replace(self.progress.cached_input_tokens, self.response_usage.cached_input_tokens, current.cached_input_tokens)?;
        next.progress.output_tokens = replace(self.progress.output_tokens, self.response_usage.output_tokens, current.output_tokens)?;
        next.progress.total_tokens = replace(self.progress.total_tokens, self.response_usage.total_tokens, current.total_tokens)?;
        next.progress.provider_cost_microunits = replace(self.progress.provider_cost_microunits, self.response_usage.provider_cost_microunits, current.provider_cost_microunits)?;
        let Some(remainder) = self.progress.usage_observations.checked_sub(self.response_usage.observations) else {
            return Err(AccountingError::ObservationOverflow);
        };
        let Some(count) = remainder.checked_add(current.observations) else {
            return Err(AccountingError::ObservationOverflow);
        };
        next.progress.usage_observations = count;
        next.response_usage = current;
        Ok(next)
    }

    /// Replaces one response snapshot atomically; an arithmetic rejection preserves all state.
    pub(crate) fn apply_usage(&mut self, current: UsageSnapshot) -> (result: Result<(), AccountingError>)
        ensures
            result.is_ok() == old(self).usage_fits(current),
            result.is_ok() ==> final(self).is_usage_update_of(*old(self), current),
            result.is_err() ==> *final(self) == *old(self),
            match result {
                Ok(()) => true,
                Err(AccountingError::CounterOverflow) => !old(self).numeric_usage_fits(current),
                Err(AccountingError::ObservationOverflow) => old(self).numeric_usage_fits(current)
                    && !old(self).observation_count_fits(current),
            },
    {
        let next = self.checked_usage(current)?;
        *self = next;
        Ok(())
    }
}

} // verus!
