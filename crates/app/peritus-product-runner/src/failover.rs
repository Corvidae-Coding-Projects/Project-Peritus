//! Explicit provider-chain selection after ordinary role recovery is exhausted.

use std::sync::Arc;

use peritus_agent::{DeveloperLoopError, ModelDriveError};
use peritus_model_protocol::{Capability, FailureCategory};
use peritus_provider_core::{ModelProvider, ProviderCoreErrorKind};
use peritus_types::ProviderProfileId;

use crate::{ProductRunnerError, budget::RunAccounting, execution::ProductRunInput};

pub struct ProviderCursor<'a> {
    candidates: Vec<&'a dyn ModelProvider>,
    index: usize,
}

impl<'a> ProviderCursor<'a> {
    pub fn new(
        primary: &'a Arc<dyn ModelProvider>,
        fallbacks: &'a [Arc<dyn ModelProvider>],
    ) -> Self {
        let mut candidates = vec![primary.as_ref()];
        for provider in fallbacks {
            let profile = provider.profile();
            if profile.capabilities().supports(Capability::ToolCalls)
                && !candidates
                    .iter()
                    .any(|candidate| candidate.profile().profile_id() == profile.profile_id())
            {
                candidates.push(provider.as_ref());
            }
        }
        Self { candidates, index: 0 }
    }

    pub fn current(&self) -> &'a dyn ModelProvider {
        self.candidates[self.index]
    }

    pub fn advance(&mut self, error: &DeveloperLoopError) -> Option<ProviderSwitch> {
        let reason = failover_reason(error)?;
        let next = self.index.checked_add(1)?;
        let provider = *self.candidates.get(next)?;
        let previous = self.current().profile().profile_id();
        self.index = next;
        Some(ProviderSwitch { previous, next: provider.profile().profile_id(), reason })
    }

    pub fn advance_for_capability(&mut self, error: &ProductRunnerError) -> Option<ProviderSwitch> {
        if error.kind() != crate::ProductRunnerErrorKind::Provider
            || error.operation() != "attach workspace images"
        {
            return None;
        }
        self.advance_with_reason("capability_mismatch")
    }

    fn advance_with_reason(&mut self, reason: &'static str) -> Option<ProviderSwitch> {
        let next = self.index.checked_add(1)?;
        let provider = *self.candidates.get(next)?;
        let previous = self.current().profile().profile_id();
        self.index = next;
        Some(ProviderSwitch { previous, next: provider.profile().profile_id(), reason })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderSwitch {
    previous: ProviderProfileId,
    next: ProviderProfileId,
    reason: &'static str,
}

impl ProviderSwitch {
    pub const fn previous(self) -> ProviderProfileId {
        self.previous
    }

    pub const fn next(self) -> ProviderProfileId {
        self.next
    }

    pub const fn reason(self) -> &'static str {
        self.reason
    }
}

const fn failover_reason(error: &DeveloperLoopError) -> Option<&'static str> {
    match error {
        DeveloperLoopError::EmptyResponse => Some("empty_response"),
        DeveloperLoopError::ProviderTerminal { category, .. } => terminal_reason(*category),
        DeveloperLoopError::Model(ModelDriveError::Provider(error)) => match error.kind() {
            ProviderCoreErrorKind::InvalidCredential => Some("credential_unavailable"),
            ProviderCoreErrorKind::LimitExceeded => Some("provider_limit"),
            ProviderCoreErrorKind::Connect => Some("connection"),
            ProviderCoreErrorKind::MalformedStream => Some("malformed_stream"),
            ProviderCoreErrorKind::Configuration => Some("provider_configuration"),
            _ => None,
        },
        _ => None,
    }
}

pub fn record_switch(
    input: &ProductRunInput,
    role: &str,
    cycle: u32,
    accounting: &mut RunAccounting,
    switch: ProviderSwitch,
) -> Result<(), ProductRunnerError> {
    crate::trace::record_provider_switch(&input.trace_path, role, cycle, switch)?;
    accounting.record_provider_failover()
}

const fn terminal_reason(category: FailureCategory) -> Option<&'static str> {
    match category {
        FailureCategory::Authentication => Some("authentication"),
        FailureCategory::Permission => Some("permission"),
        FailureCategory::NotFound => Some("model_unavailable"),
        FailureCategory::RateLimited => Some("rate_limited"),
        FailureCategory::QuotaExhausted => Some("quota_exhausted"),
        FailureCategory::TransientProvider => Some("transient_provider"),
        FailureCategory::Transport => Some("transport"),
        FailureCategory::MalformedPayload => Some("malformed_payload"),
        FailureCategory::IncompleteStream => Some("incomplete_stream"),
        FailureCategory::Timeout => Some("timeout"),
        FailureCategory::Provider => Some("provider"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_terminals_allow_failover_but_policy_terminals_do_not() {
        assert_eq!(terminal_reason(FailureCategory::RateLimited), Some("rate_limited"));
        assert_eq!(terminal_reason(FailureCategory::QuotaExhausted), Some("quota_exhausted"));
        assert_eq!(terminal_reason(FailureCategory::Safety), None);
        assert_eq!(terminal_reason(FailureCategory::Refusal), None);
        assert_eq!(terminal_reason(FailureCategory::AmbiguousAcceptance), None);
        assert_eq!(terminal_reason(FailureCategory::Cancellation), None);
        assert_eq!(
            failover_reason(&DeveloperLoopError::Model(ModelDriveError::Provider(
                peritus_provider_core::ProviderCoreError::transport(
                    "send",
                    "submission outcome is unknown",
                ),
            ))),
            None
        );
    }
}
