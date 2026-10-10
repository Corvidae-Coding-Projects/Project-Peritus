//! Atomic admission of numeric work events before completed-work ceiling checks.

use super::{AccountingState, UsageSnapshot};

#[doc = " Numeric work boundaries emitted by the ordinary product runner."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkEvent {
    #[doc = " A new provider response starts after its request counters fit."]
    ModelRequest { retry: bool },
    #[doc = " One application tool observation completed."]
    ToolCall,
    #[doc = " One context replacement completed."]
    Compaction,
    #[doc = " One role-level retry was scheduled."]
    RoleRetry,
    #[doc = " One provider switch was recorded."]
    ProviderFailover,
}
impl AccountingState {
    fn checked_work(self, event: WorkEvent) -> Option<Self> {
        let mut next = self;
        match event {
            WorkEvent::ModelRequest { retry } => {
                next.progress.model_requests = self.progress.model_requests.checked_add(1)?;
                if retry {
                    next.progress.retries = self.progress.retries.checked_add(1)?;
                }
                next.response_usage = UsageSnapshot::empty();
            }
            WorkEvent::ToolCall => {
                next.progress.tool_calls = self.progress.tool_calls.checked_add(1)?;
            }
            WorkEvent::Compaction => {
                next.progress.compactions = self.progress.compactions.checked_add(1)?;
            }
            WorkEvent::RoleRetry => {
                next.progress.retries = self.progress.retries.checked_add(1)?;
            }
            WorkEvent::ProviderFailover => {
                next.progress.provider_failovers =
                    self.progress.provider_failovers.checked_add(1)?;
            }
        }
        Some(next)
    }
    #[doc = " Publishes the complete arithmetic update or leaves all state unchanged."]
    pub(crate) fn apply_work(&mut self, event: WorkEvent) -> bool {
        let Some(next) = self.checked_work(event) else {
            return false;
        };
        *self = next;
        true
    }
}
