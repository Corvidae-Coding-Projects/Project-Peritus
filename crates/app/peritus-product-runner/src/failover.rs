//! Explicit provider-chain selection after ordinary role recovery is exhausted.

use std::sync::Arc;

use peritus_agent::{DeveloperLoopError, ModelDriveError};
use peritus_model_protocol::{Capability, FailureCategory};
use peritus_provider_core::{ModelProvider, ProviderCoreErrorKind};
use peritus_types::ProviderProfileId;

use crate::{ProductRunnerError, budget::RunAccounting, execution::ProductRunInput};

const MAX_SAME_PROVIDER_ROLE_INVOCATIONS: u8 = 3;

/// Bounded fresh-invocation recovery after one role exhausts its in-turn provider retries.
#[derive(Default)]
pub struct RoleRecovery {
    failed_invocations: u8,
}

impl RoleRecovery {
    /// Returns a stable reason when the role may start another grounded invocation.
    pub fn retry(&mut self, error: &DeveloperLoopError) -> Option<&'static str> {
        let reason = same_provider_retry_reason(error)?;
        self.failed_invocations = self.failed_invocations.saturating_add(1);
        (self.failed_invocations < MAX_SAME_PROVIDER_ROLE_INVOCATIONS).then_some(reason)
    }

    /// Starts a fresh recovery budget after progress or a usable provider response.
    pub const fn reset(&mut self) {
        self.failed_invocations = 0;
    }

    /// Builds the correction that starts a fresh repository-grounded invocation.
    pub fn correction(reason: &str) -> String {
        format!(
            "The preceding provider invocation ended with recoverable `{reason}` after its bounded in-turn retries. Start a fresh invocation from the exact current workspace: call `workspace_list`, read the authoritative inputs and current targets, preserve any useful existing work, and continue to the required terminal result."
        )
    }
}

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

    pub fn advance_past_open_circuit(
        &mut self,
        accounting: &RunAccounting,
    ) -> Option<ProviderSwitch> {
        let previous = self.current().profile().profile_id();
        if !accounting.provider_circuit_open(previous) {
            return None;
        }
        let next = self.candidates.iter().enumerate().skip(self.index + 1).find_map(
            |(index, provider)| {
                (!accounting.provider_circuit_open(provider.profile().profile_id()))
                    .then_some(index)
            },
        )?;
        self.index = next;
        Some(ProviderSwitch {
            previous,
            next: self.current().profile().profile_id(),
            reason: "open_circuit",
        })
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

const fn same_provider_retry_reason(error: &DeveloperLoopError) -> Option<&'static str> {
    match error {
        DeveloperLoopError::EmptyResponse => Some("empty_response"),
        DeveloperLoopError::ProviderTerminal { category, .. } => {
            same_provider_terminal_reason(*category)
        }
        DeveloperLoopError::Model(ModelDriveError::Provider(error)) => match error.kind() {
            ProviderCoreErrorKind::Connect => Some("connection"),
            ProviderCoreErrorKind::MalformedStream => Some("malformed_stream"),
            _ => None,
        },
        _ => None,
    }
}

const fn same_provider_terminal_reason(category: FailureCategory) -> Option<&'static str> {
    match category {
        FailureCategory::RateLimited => Some("rate_limited"),
        FailureCategory::TransientProvider => Some("transient_provider"),
        FailureCategory::Transport => Some("transport"),
        FailureCategory::MalformedPayload => Some("malformed_payload"),
        FailureCategory::IncompleteStream => Some("incomplete_stream"),
        FailureCategory::Timeout => Some("timeout"),
        FailureCategory::Provider => Some("provider"),
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
    if opens_circuit(switch.reason()) {
        accounting.open_provider_circuit(switch.previous());
    }
    accounting.record_provider_failover()
}

pub fn bypass_open_circuit(
    input: &ProductRunInput,
    role: &str,
    cycle: u32,
    accounting: &mut RunAccounting,
    providers: &mut ProviderCursor<'_>,
) -> Result<(), ProductRunnerError> {
    if let Some(switch) = providers.advance_past_open_circuit(accounting) {
        record_switch(input, role, cycle, accounting, switch)?;
    }
    Ok(())
}

pub fn record_provider_success(
    accounting: &mut RunAccounting,
    providers: &ProviderCursor<'_>,
    recovery: &mut RoleRecovery,
) {
    recovery.reset();
    accounting.close_provider_circuit(providers.current().profile().profile_id());
}

fn opens_circuit(reason: &str) -> bool {
    !matches!(reason, "capability_mismatch" | "open_circuit")
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

    #[test]
    fn same_provider_role_recovery_is_transient_and_finite() {
        let mut recovery = RoleRecovery::default();
        assert_eq!(recovery.retry(&DeveloperLoopError::EmptyResponse), Some("empty_response"));
        assert_eq!(recovery.retry(&DeveloperLoopError::EmptyResponse), Some("empty_response"));
        assert_eq!(recovery.retry(&DeveloperLoopError::EmptyResponse), None);

        recovery.reset();
        let interrupted = DeveloperLoopError::ProviderTerminal {
            provider: "fixture".to_owned(),
            category: FailureCategory::IncompleteStream,
            diagnostic_code: "fixture.interrupted".to_owned(),
        };
        assert_eq!(recovery.retry(&interrupted), Some("incomplete_stream"));
        assert_eq!(recovery.retry(&interrupted), Some("incomplete_stream"));
        assert_eq!(recovery.retry(&interrupted), None);

        recovery.reset();
        assert_eq!(
            recovery.retry(&DeveloperLoopError::ProviderTerminal {
                provider: "fixture".to_owned(),
                category: FailureCategory::Safety,
                diagnostic_code: "fixture.safety".to_owned(),
            }),
            None
        );
        assert_eq!(
            recovery.retry(&DeveloperLoopError::ProviderTerminal {
                provider: "fixture".to_owned(),
                category: FailureCategory::Timeout,
                diagnostic_code: "fixture.timeout".to_owned(),
            }),
            Some("timeout")
        );
        recovery.reset();
        assert_eq!(
            recovery.retry(&DeveloperLoopError::ProviderTerminal {
                provider: "fixture".to_owned(),
                category: FailureCategory::AmbiguousAcceptance,
                diagnostic_code: "fixture.ambiguous".to_owned(),
            }),
            None
        );
    }

    #[test]
    fn capability_and_circuit_bypasses_do_not_reopen_provider_circuits() {
        assert!(!opens_circuit("capability_mismatch"));
        assert!(!opens_circuit("open_circuit"));
        assert!(opens_circuit("authentication"));
        assert!(opens_circuit("empty_response"));
    }
}
