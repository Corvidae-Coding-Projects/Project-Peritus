//! Exact replacement of the current response's contribution to cumulative usage.

use super::{AccountingError, AccountingState};

#[doc = " The numeric provider usage fields retained by product accounting."]
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
    pub(crate) const fn empty() -> Self {
        Self {
            input_tokens: 0,
            cached_input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
            provider_cost_microunits: 0,
            observations: 0,
        }
    }
}
const fn replace(total: u64, previous: u64, current: u64) -> Result<u64, AccountingError> {
    let Some(remainder) = total.checked_sub(previous) else {
        return Err(AccountingError::CounterOverflow);
    };
    let Some(value) = remainder.checked_add(current) else {
        return Err(AccountingError::CounterOverflow);
    };
    Ok(value)
}
impl AccountingState {
    fn checked_usage(self, current: UsageSnapshot) -> Result<Self, AccountingError> {
        let mut next = self;
        next.progress.input_tokens = replace(
            self.progress.input_tokens,
            self.response_usage.input_tokens,
            current.input_tokens,
        )?;
        next.progress.cached_input_tokens = replace(
            self.progress.cached_input_tokens,
            self.response_usage.cached_input_tokens,
            current.cached_input_tokens,
        )?;
        next.progress.output_tokens = replace(
            self.progress.output_tokens,
            self.response_usage.output_tokens,
            current.output_tokens,
        )?;
        next.progress.total_tokens = replace(
            self.progress.total_tokens,
            self.response_usage.total_tokens,
            current.total_tokens,
        )?;
        next.progress.provider_cost_microunits = replace(
            self.progress.provider_cost_microunits,
            self.response_usage.provider_cost_microunits,
            current.provider_cost_microunits,
        )?;
        let Some(remainder) =
            self.progress.usage_observations.checked_sub(self.response_usage.observations)
        else {
            return Err(AccountingError::ObservationOverflow);
        };
        let Some(count) = remainder.checked_add(current.observations) else {
            return Err(AccountingError::ObservationOverflow);
        };
        next.progress.usage_observations = count;
        next.response_usage = current;
        Ok(next)
    }
    #[doc = " Replaces one response snapshot atomically; an arithmetic rejection preserves all state."]
    pub(crate) fn apply_usage(&mut self, current: UsageSnapshot) -> Result<(), AccountingError> {
        let next = self.checked_usage(current)?;
        *self = next;
        Ok(())
    }
}
