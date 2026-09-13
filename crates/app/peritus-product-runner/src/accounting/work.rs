//! Atomic admission of numeric work events before completed-work ceiling checks.

use super::{AccountingState, UsageSnapshot};
use vstd::prelude::*;

verus! {

/// Numeric work boundaries emitted by the ordinary product runner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkEvent {
    /// A new provider response starts after its request counters fit.
    ModelRequest { retry: bool },
    /// One application tool observation completed.
    ToolCall,
    /// One context replacement completed.
    Compaction,
    /// One role-level retry was scheduled.
    RoleRetry,
    /// One provider switch was recorded.
    ProviderFailover,
}

impl AccountingState {
    pub(crate) closed spec fn work_fits(self, event: WorkEvent) -> bool {
        match event {
            WorkEvent::ModelRequest { retry } => self.progress.model_requests < u32::MAX
                && (!retry || self.progress.retries < u32::MAX),
            WorkEvent::ToolCall => self.progress.tool_calls < u32::MAX,
            WorkEvent::Compaction => self.progress.compactions < u32::MAX,
            WorkEvent::RoleRetry => self.progress.retries < u32::MAX,
            WorkEvent::ProviderFailover => self.progress.provider_failovers < u32::MAX,
        }
    }

    pub(crate) closed spec fn is_work_update_of(self, before: Self, event: WorkEvent) -> bool {
        self.progress.model_requests as int == before.progress.model_requests as int + (match event { WorkEvent::ModelRequest { .. } => 1int, _ => 0int })
        && self.progress.tool_calls as int == before.progress.tool_calls as int + (match event { WorkEvent::ToolCall => 1int, _ => 0int })
        && self.progress.retries as int == before.progress.retries as int + (match event { WorkEvent::ModelRequest { retry: true } | WorkEvent::RoleRetry => 1int, _ => 0int })
        && self.progress.provider_failovers as int == before.progress.provider_failovers as int + (match event { WorkEvent::ProviderFailover => 1int, _ => 0int })
        && self.progress.compactions as int == before.progress.compactions as int + (match event { WorkEvent::Compaction => 1int, _ => 0int })
        && self.progress.input_tokens == before.progress.input_tokens
        && self.progress.cached_input_tokens == before.progress.cached_input_tokens
        && self.progress.output_tokens == before.progress.output_tokens
        && self.progress.total_tokens == before.progress.total_tokens
        && self.progress.provider_cost_microunits == before.progress.provider_cost_microunits
        && self.progress.usage_observations == before.progress.usage_observations
        && self.progress.elapsed_millis == before.progress.elapsed_millis
        && self.progress.workspace_bytes == before.progress.workspace_bytes
        && self.progress.workspace_growth_bytes == before.progress.workspace_growth_bytes
        && self.progress.peak_rss_bytes == before.progress.peak_rss_bytes
        && match event {
            WorkEvent::ModelRequest { .. } => {
                self.response_usage.input_tokens == 0
                    && self.response_usage.cached_input_tokens == 0
                    && self.response_usage.output_tokens == 0
                    && self.response_usage.total_tokens == 0
                    && self.response_usage.provider_cost_microunits == 0
                    && self.response_usage.observations == 0
            },
            _ => self.response_usage == before.response_usage,
        }
    }

    fn checked_work(self, event: WorkEvent) -> (result: Option<Self>)
        ensures
            result.is_some() == self.work_fits(event),
            match result { Some(next) => next.is_work_update_of(self, event), None => true },
    {
        let mut next = self;
        match event {
            WorkEvent::ModelRequest { retry } => {
                next.progress.model_requests = self.progress.model_requests.checked_add(1)?;
                if retry {
                    next.progress.retries = self.progress.retries.checked_add(1)?;
                }
                next.response_usage = UsageSnapshot::empty();
            },
            WorkEvent::ToolCall => {
                next.progress.tool_calls = self.progress.tool_calls.checked_add(1)?;
            },
            WorkEvent::Compaction => {
                next.progress.compactions = self.progress.compactions.checked_add(1)?;
            },
            WorkEvent::RoleRetry => {
                next.progress.retries = self.progress.retries.checked_add(1)?;
            },
            WorkEvent::ProviderFailover => {
                next.progress.provider_failovers = self.progress.provider_failovers.checked_add(1)?;
            },
        }
        Some(next)
    }

    /// Publishes the complete arithmetic update or leaves all state unchanged.
    pub(crate) fn apply_work(&mut self, event: WorkEvent) -> (applied: bool)
        ensures
            applied == old(self).work_fits(event),
            applied ==> final(self).is_work_update_of(*old(self), event),
            !applied ==> *final(self) == *old(self),
    {
        let Some(next) = self.checked_work(event) else {
            return false;
        };
        *self = next;
        true
    }
}

} // verus!
