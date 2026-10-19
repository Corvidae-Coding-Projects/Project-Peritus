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
    /// Whether material progress may start another attempt after a provider failure.
    pub const fn may_continue_after_progress(error: &DeveloperLoopError) -> bool {
        same_provider_retry_reason(error).is_some()
    }

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
        if reason == "segment_boundary" {
            return "The preceding bounded invocation segment ended before the role produced its terminal result. Continue from the retained design, completed results and host-verified observations; refresh only changed inputs or the next target needed to resume, preserve completed work and evidence, and continue to the required terminal result without repeating finished effects.".to_owned();
        }
        let repair = if matches!(reason, "malformed_payload" | "malformed_stream") {
            " The provider output contract was not satisfied. Return one valid response using only the declared host tools and their exact argument schemas. If the adapter requires a structured envelope, put role-specific text or JSON inside its content field, put calls in tool_calls, and encode arguments_json exactly once; do not use native tools, Markdown fences, undeclared fields, or a second structured result. Do not replay prior side effects: inspect the existing results and recover any retained command handle first."
        } else {
            ""
        };
        format!(
            "The preceding provider invocation ended with recoverable `{reason}` after its bounded in-turn retries. Reuse host-verified observations and retained context. If grounding is missing, inspect only the required current target; do not restart repository discovery, preserve any useful existing work, and continue to the required terminal result. Later provider steps in this same invocation must continue from completed work, not repeat this startup sequence.{repair}"
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
