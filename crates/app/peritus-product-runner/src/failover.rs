//! Explicit provider-chain selection after ordinary role recovery is exhausted.

use std::sync::Arc;

use peritus_agent::{DeveloperLoopError, ModelDriveError};
use peritus_model_protocol::{
    Capability, FailureCategory, OutcomeCertainty, Retryability,
};
use peritus_provider_core::{ModelProvider, ProviderCoreErrorKind};
use peritus_types::ProviderProfileId;

use crate::{ProductRunnerError, budget::RunAccounting, execution::ProductRunInput};

/// Same-provider recovery under the role's durable local context and native session namespace.
#[derive(Default)]
pub struct RoleRecovery;

impl RoleRecovery {
    /// Returns a stable reason when the role may reconnect under retained context.
    pub fn retry(&self, error: &DeveloperLoopError) -> Option<&'static str> {
        same_provider_retry_reason(error)
    }

    /// Compatibility hook; retained recovery has no cumulative invocation budget.
    pub const fn reset(&mut self) {
    }

    /// Builds the correction that resumes retained context or transfers it explicitly.
    pub fn correction(reason: &str) -> String {
        if reason == "segment_boundary" {
            return "The preceding invocation segment ended before the role produced its terminal result. Continue the same task and role from the retained host context. Prior assistant/tool exchanges and completed effects remain authoritative at their recorded revisions; resume outstanding work without replaying finished effects or repeating startup inspection solely because the segment changed.".to_owned();
        }
        if reason == "empty_response" {
            return "The preceding provider request reached a terminal response without usable text or tool output. Continue the same task and role under the same provider profile, native session namespace, retained host transcript, and completed tool evidence. Resume outstanding work without repeating startup inspection or settled effects solely because a new invocation was required.".to_owned();
        }
        if reason == "provider_transfer" {
            return "The host explicitly transferred ownership of this role to another configured provider after durably retaining the prior request disposition. Continue the same task and role from the retained host transcript, completed tool evidence, and pending obligations. Treat provider-native state as route-specific. Never assume an admitted or ambiguously accepted request was discarded; the host permits transfer only at an acceptance-safe boundary. Do not repeat settled effects or startup inspection solely because the route changed.".to_owned();
        }
        format!(
            "The preceding provider connection ended with definitely-unaccepted `{reason}`. Reconnect under the same task, role, provider profile, native session namespace, and retained host transcript. Continue from completed tool evidence and outstanding obligations; do not repeat startup inspection or settled effects solely because the transport reconnected."
        )
    }

    /// Preserves an unsent host correction while transferring the role to a configured provider.
    pub fn transfer_correction(pending: Option<&str>) -> String {
        let mut transfer = Self::correction("provider_transfer");
        if let Some(pending) = pending {
            transfer.push_str("\n\nThe following host correction remains an outstanding obligation:\n");
            transfer.push_str(pending);
        }
        transfer
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

    /// Reconsiders the caller-selected primary after the caller observes a new authority binding.
    /// The durable retry trace, rather than this route cursor, retains every failed attempt.
    pub fn reopen(&mut self) {
        self.index = 0;
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

fn failover_reason(error: &DeveloperLoopError) -> Option<&'static str> {
    match error {
        DeveloperLoopError::ProviderFailure(failure)
            if failure.certainty() == OutcomeCertainty::DefinitelyNotAccepted =>
        {
            match failure.retryability() {
                Retryability::SafeNewRequest | Retryability::Never => {
                    terminal_reason(failure.category())
                }
                Retryability::ExactResumeOnly | Retryability::CallerDecision => None,
            }
        }
        DeveloperLoopError::ProviderFailure(_) | DeveloperLoopError::ProviderTerminal { .. } => {
            None
        }
        DeveloperLoopError::Model(ModelDriveError::Provider(error)) => match error.kind() {
            ProviderCoreErrorKind::InvalidCredential => Some("credential_unavailable"),
            ProviderCoreErrorKind::LimitExceeded => Some("provider_limit"),
            ProviderCoreErrorKind::Connect => Some("connection"),
            ProviderCoreErrorKind::Configuration => Some("provider_configuration"),
            _ => None,
        },
        _ => None,
    }
}

fn same_provider_retry_reason(error: &DeveloperLoopError) -> Option<&'static str> {
    match error {
        DeveloperLoopError::ProviderFailure(failure)
            if failure.certainty() == OutcomeCertainty::DefinitelyNotAccepted
                && failure.retryability() == Retryability::SafeNewRequest =>
        {
            same_provider_terminal_reason(failure.category())
        }
        DeveloperLoopError::ProviderFailure(_) | DeveloperLoopError::ProviderTerminal { .. } => {
            None
        }
        DeveloperLoopError::Model(ModelDriveError::Provider(error)) => match error.kind() {
            ProviderCoreErrorKind::Connect => Some("connection"),
            _ => None,
        },
        DeveloperLoopError::EmptyResponse => Some("empty_response"),
        _ => None,
    }
}

/// Whether a newer authority binding must wait for reconciliation of the completed invocation.
/// Only an explicit terminal or a definitely-unaccepted/pre-dispatch failure makes a new request
/// safe. Durable trace, context, tool, and ambiguous transport failures retain their exact owner.
pub fn requires_reconciliation_before_new_request(error: &DeveloperLoopError) -> bool {
    match error {
        DeveloperLoopError::ProviderFailure(failure) => !matches!(
            failure.certainty(),
            OutcomeCertainty::DefinitelyNotAccepted | OutcomeCertainty::Terminal
        ),
        DeveloperLoopError::Model(ModelDriveError::Provider(error)) => !matches!(
            error.kind(),
            ProviderCoreErrorKind::InvalidEndpoint
                | ProviderCoreErrorKind::InvalidCredential
                | ProviderCoreErrorKind::InvalidRequest
                | ProviderCoreErrorKind::InvalidHttp
                | ProviderCoreErrorKind::LimitExceeded
                | ProviderCoreErrorKind::Connect
                | ProviderCoreErrorKind::InvalidRetry
                | ProviderCoreErrorKind::Configuration
                | ProviderCoreErrorKind::UnsupportedCapability
                | ProviderCoreErrorKind::Unavailable
        ),
        DeveloperLoopError::ProviderTerminal { .. } => true,
        DeveloperLoopError::Refused
        | DeveloperLoopError::LimitExceeded
        | DeveloperLoopError::SegmentExhausted
        | DeveloperLoopError::SegmentContinuation
        | DeveloperLoopError::EmptyResponse => false,
        _ => true,
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
    accounting.record_provider_failover()
}

pub fn record_provider_success(recovery: &mut RoleRecovery) {
    recovery.reset();
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
    fn recovery_never_overrides_integrity_or_policy() {
        let recovery = RoleRecovery;
        assert_eq!(recovery.retry(&DeveloperLoopError::EmptyResponse), Some("empty_response"));
        for error in [
            DeveloperLoopError::LimitExceeded,
            DeveloperLoopError::SegmentExhausted,
            DeveloperLoopError::Cancelled,
            DeveloperLoopError::Refused,
            DeveloperLoopError::Trace("fixture".to_owned()),
            DeveloperLoopError::Tool("fixture".to_owned()),
            DeveloperLoopError::Tool("inspection-no-progress".to_owned()),
            DeveloperLoopError::Context("fixture".to_owned()),
        ] {
            assert_eq!(recovery.retry(&error), None);
        }
        for category in [
            FailureCategory::Safety,
            FailureCategory::Refusal,
            FailureCategory::Cancellation,
            FailureCategory::AmbiguousAcceptance,
            FailureCategory::Permission,
            FailureCategory::Authentication,
        ] {
            assert_eq!(
                recovery.retry(&DeveloperLoopError::ProviderTerminal {
                    provider: "fixture".to_owned(),
                    category,
                    diagnostic_code: "fixture.stop".to_owned(),
                    http_status: None,
                }),
                None
            );
        }
    }

    #[test]
    fn segment_correction_resumes_current_work_without_repeating_effects() {
        let correction = RoleRecovery::correction("segment_boundary");
        assert!(correction.contains("invocation segment ended"));
        assert!(correction.contains("retained host context"));
        assert!(correction.contains("without replaying finished effects"));
    }

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
    fn same_provider_role_recovery_has_no_invocation_budget() {
        let recovery = RoleRecovery;
        for _ in 0..8 {
            assert_eq!(recovery.retry(&DeveloperLoopError::EmptyResponse), Some("empty_response"));
        }

        let interrupted = DeveloperLoopError::ProviderTerminal {
            provider: "fixture".to_owned(),
            category: FailureCategory::IncompleteStream,
            diagnostic_code: "fixture.interrupted".to_owned(),
            http_status: None,
        };
        assert_eq!(recovery.retry(&interrupted), None);
        assert_eq!(
            recovery.retry(&DeveloperLoopError::ProviderTerminal {
                provider: "fixture".to_owned(),
                category: FailureCategory::Safety,
                diagnostic_code: "fixture.safety".to_owned(),
                http_status: None,
            }),
            None
        );
        assert_eq!(
            recovery.retry(&DeveloperLoopError::ProviderTerminal {
                provider: "fixture".to_owned(),
                category: FailureCategory::Timeout,
                diagnostic_code: "fixture.timeout".to_owned(),
                http_status: None,
            }),
            None
        );
        assert_eq!(
            recovery.retry(&DeveloperLoopError::ProviderTerminal {
                provider: "fixture".to_owned(),
                category: FailureCategory::AmbiguousAcceptance,
                diagnostic_code: "fixture.ambiguous".to_owned(),
                http_status: None,
            }),
            None
        );
    }

    #[test]
    fn provider_transfer_preserves_an_unsent_obligation() {
        let correction = RoleRecovery::transfer_correction(Some("retain this exact obligation"));
        assert!(correction.contains("explicitly transferred ownership"));
        assert!(correction.contains("acceptance-safe boundary"));
        assert!(correction.contains("retain this exact obligation"));
    }
}
