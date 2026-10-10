//! Shared terminal and caller-binding support for Git dispatchers.

use peritus_policy::AuthorityInstant;
use peritus_tool_protocol::{
    BoundedText, FailureCategory, RecoveryRoute, ResponsibleSubsystem, ResultStatus, Retryability,
    ToolFailure, ToolResult, ToolTiming, Truncation, TruncationMetadata,
};
use peritus_tool_router::{AuthorizedInvocation, DispatchFailure};
use peritus_workspace::{
    RecoveryClass as WorkspaceRecoveryClass, WorkspaceCallerBinding, WorkspaceError,
};

use crate::{GitToolError, GitToolErrorKind, RecoveryClass, RenderedOutput};

pub fn caller_binding(invocation: &AuthorizedInvocation) -> WorkspaceCallerBinding {
    let binding = invocation.binding();
    WorkspaceCallerBinding::new(
        invocation.action_id(),
        binding.actor_id(),
        binding.role(),
        binding.revision().workspace_id(),
        binding.environment_id(),
        binding.resource_id(),
        invocation.prepared().descriptor().name().clone(),
        invocation.prepared().descriptor_digest().get(),
        invocation.prepared_digest(),
    )
}

pub const fn minimum_result_capacity(prepared: &peritus_tool_protocol::PreparedToolCall) -> bool {
    let limits = prepared.call().limits();
    limits.output_bytes() >= 1_024 && limits.model_bytes() >= 128 && limits.human_bytes() >= 128
}

pub fn finish(
    prepared: &peritus_tool_protocol::PreparedToolCall,
    rendered: &RenderedOutput,
    started_at: AuthorityInstant,
    completed_at: AuthorityInstant,
) -> Result<ToolResult, DispatchFailure> {
    if rendered.structured().canonical_bytes().len() as u64
        > prepared.call().limits().output_bytes()
    {
        return Err(protocol_failure("structured result exceeds the selected call output bound"));
    }
    let timing = ToolTiming::new(started_at, completed_at)
        .map_err(|_| protocol_failure("dispatcher completion time is invalid"))?;
    ToolResult::success(
        prepared,
        rendered.structured().clone(),
        rendered.human().clone(),
        rendered.model().clone(),
        Vec::new(),
        timing,
        TruncationMetadata {
            output: if rendered.truncated() {
                Truncation::TailDropped
            } else {
                Truncation::Complete
            },
            model: Truncation::Complete,
            human: Truncation::Complete,
        },
        0,
    )
    .map_err(|_| protocol_failure("terminal Git result is invalid"))
}

pub fn tool_failure(error: &GitToolError) -> DispatchFailure {
    let category = match error.kind() {
        GitToolErrorKind::Git | GitToolErrorKind::Workspace => FailureCategory::Workspace,
        GitToolErrorKind::InvalidInput | GitToolErrorKind::Protocol => FailureCategory::Protocol,
    };
    failure(category, error.code(), error.detail(), error.recovery())
}

pub fn workspace_failure(error: &WorkspaceError) -> DispatchFailure {
    let recovery = match error.recovery() {
        WorkspaceRecoveryClass::CorrectRequest => RecoveryClass::CorrectInput,
        WorkspaceRecoveryClass::Reauthorize => RecoveryClass::Reauthorize,
        WorkspaceRecoveryClass::Reobserve => RecoveryClass::Reobserve,
        WorkspaceRecoveryClass::Reconcile => RecoveryClass::Reconcile,
        WorkspaceRecoveryClass::Quarantine => RecoveryClass::Quarantine,
    };
    failure(FailureCategory::Workspace, error.code().as_str(), error.detail(), recovery)
}

pub fn protocol_failure(detail: &'static str) -> DispatchFailure {
    failure(
        FailureCategory::Protocol,
        GitToolErrorKind::Protocol.code(),
        detail,
        RecoveryClass::CorrectInput,
    )
}

pub fn cancelled_before_effect() -> DispatchFailure {
    let cause = failure(
        FailureCategory::Cancelled,
        "git_cancelled",
        "Git execution cancelled before acquiring its target",
        RecoveryClass::CorrectInput,
    );
    DispatchFailure::new(ResultStatus::Cancelled, cause.failure().clone())
        .expect("cancelled is a non-success status")
}

pub fn indeterminate_failure(detail: &'static str) -> DispatchFailure {
    let cause = failure(
        FailureCategory::Indeterminate,
        "git_indeterminate",
        detail,
        RecoveryClass::Reconcile,
    );
    DispatchFailure::new(ResultStatus::Indeterminate, cause.failure().clone())
        .expect("indeterminate is a non-success status")
}

fn failure(
    category: FailureCategory,
    code: &'static str,
    detail: &str,
    recovery: RecoveryClass,
) -> DispatchFailure {
    let (retryability, route) = match recovery {
        RecoveryClass::CorrectInput => (Retryability::NewAction, RecoveryRoute::None),
        RecoveryClass::Reobserve | RecoveryClass::Reauthorize | RecoveryClass::Retry => {
            (Retryability::NewAction, RecoveryRoute::Reauthorize)
        }
        RecoveryClass::Quarantine => (Retryability::AfterRecovery, RecoveryRoute::HumanReview),
        RecoveryClass::Reconcile => {
            (Retryability::AfterRecovery, RecoveryRoute::ReconcileWorkspace)
        }
    };
    let failure = ToolFailure::new(
        category,
        bounded(code),
        ResponsibleSubsystem::Workspace,
        retryability,
        route,
        bounded(detail),
    );
    let status = if matches!(recovery, RecoveryClass::Reconcile | RecoveryClass::Quarantine) {
        ResultStatus::Indeterminate
    } else {
        ResultStatus::Failed
    };
    DispatchFailure::new(status, failure).expect("non-success dispatch failure is valid")
}

fn bounded(value: &str) -> BoundedText {
    BoundedText::new(value.to_owned()).expect("static Git failure text is bounded")
}

#[cfg(test)]
mod tests {
    #[test]
    fn quarantined_git_result_retains_its_human_recovery_route_and_cause() {
        let error = crate::GitToolError::new(
            crate::GitToolErrorKind::Git,
            crate::GitToolOperation::Candidate,
            crate::RecoveryClass::Quarantine,
            "exact retained snapshot differs from its receipt",
        );
        let failure = super::tool_failure(&error);
        assert_eq!(failure.status(), peritus_tool_protocol::ResultStatus::Indeterminate);
        assert_eq!(failure.failure().recovery(), peritus_tool_protocol::RecoveryRoute::HumanReview);
        assert_eq!(
            failure.failure().retryability(),
            peritus_tool_protocol::Retryability::AfterRecovery
        );
        assert_eq!(failure.failure().code().as_str(), error.code());
    }
}
