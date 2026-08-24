//! Shared terminal and caller-binding support for Git dispatchers.

use peritus_policy::AuthorityInstant;
use peritus_tool_protocol::{
    BoundedText, FailureCategory, RecoveryRoute, ResponsibleSubsystem, ResultStatus, Retryability,
    ToolFailure, ToolResult, ToolTiming, Truncation, TruncationMetadata,
};
use peritus_tool_router::{AuthorizedInvocation, DispatchFailure};
use peritus_workspace::WorkspaceCallerBinding;

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
    limits.output_bytes() >= 512 && limits.model_bytes() >= 128 && limits.human_bytes() >= 128
}

pub fn finish(
    prepared: &peritus_tool_protocol::PreparedToolCall,
    rendered: &RenderedOutput,
    completed_at: AuthorityInstant,
) -> Result<ToolResult, DispatchFailure> {
    if rendered.structured().canonical_bytes().len() as u64
        > prepared.call().limits().output_bytes()
    {
        return Err(protocol_failure("structured result exceeds the selected call output bound"));
    }
    let timing = ToolTiming::new(completed_at, completed_at)
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
        GitToolErrorKind::Unsupported => FailureCategory::Infrastructure,
        GitToolErrorKind::InvalidInput | GitToolErrorKind::Protocol => FailureCategory::Protocol,
    };
    failure(category, error.kind().code(), error.detail(), error.recovery())
}

pub fn workspace_failure(detail: &'static str) -> DispatchFailure {
    failure(
        FailureCategory::Workspace,
        GitToolErrorKind::Workspace.code(),
        detail,
        RecoveryClass::Reconcile,
    )
}

pub fn protocol_failure(detail: &'static str) -> DispatchFailure {
    failure(
        FailureCategory::Protocol,
        GitToolErrorKind::Protocol.code(),
        detail,
        RecoveryClass::CorrectInput,
    )
}

pub fn unsupported_failure() -> DispatchFailure {
    failure(
        FailureCategory::Infrastructure,
        GitToolErrorKind::Unsupported.code(),
        "C1 has no authorized merge-delivery operation",
        RecoveryClass::SelectSupportedOperation,
    )
}

fn failure(
    category: FailureCategory,
    code: &'static str,
    detail: &'static str,
    recovery: RecoveryClass,
) -> DispatchFailure {
    let (retryability, route) = match recovery {
        RecoveryClass::CorrectInput | RecoveryClass::SelectSupportedOperation => {
            (Retryability::NewAction, RecoveryRoute::None)
        }
        RecoveryClass::Reobserve | RecoveryClass::Reauthorize => {
            (Retryability::NewAction, RecoveryRoute::Reauthorize)
        }
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
    DispatchFailure::new(ResultStatus::Failed, failure)
        .expect("non-success static dispatch failure is valid")
}

fn bounded(value: &str) -> BoundedText {
    BoundedText::new(value.to_owned()).expect("static Git failure text is bounded")
}
