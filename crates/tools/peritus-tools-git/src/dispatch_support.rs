//! Shared terminal and caller-binding support for Git dispatchers.

use peritus_policy::AuthorityInstant;
use peritus_tool_protocol::{
    BoundedText, CancellationReason, FailureCategory, RecoveryRoute, ResponsibleSubsystem,
    ResultStatus, Retryability, ToolFailure, ToolResult, ToolTiming, Truncation,
    TruncationMetadata,
};
use peritus_tool_router::{AuthorizedInvocation, DispatchFailure};
use peritus_workspace::{
    ErrorCode as WorkspaceErrorCode, RecoveryClass as WorkspaceRecovery,
    WorkspaceCallerBinding, WorkspaceError,
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

pub fn minimum_result_capacity(
    prepared: &peritus_tool_protocol::PreparedToolCall,
    owned_observation: bool,
) -> bool {
    let limits = prepared.call().limits();
    if matches!(prepared.descriptor().name().as_str(), "git.candidate" | "git.rollback") {
        // Effectful operations perform exact terminal admission before consuming authority and
        // again after publishing their fixed-width durable operation reference.
        return true;
    }
    let minimum_output = match prepared.descriptor().name().as_str() {
        "git.diff" | "git.history" | "git.status" => 1_024,
        _ => 512,
    };
    limits.output_bytes() >= minimum_output
        && limits.model_bytes() >= 128
        && limits.human_bytes() >= 128
        && (!owned_observation || limits.progress_events() >= 2)
}

pub fn finish(
    prepared: &peritus_tool_protocol::PreparedToolCall,
    rendered: &RenderedOutput,
    started_at: AuthorityInstant,
    completed_at: AuthorityInstant,
    progress_count: u64,
) -> Result<ToolResult, DispatchFailure> {
    let encoded_bytes = u64::try_from(rendered.structured().canonical_bytes().len())
        .map_err(|_| protocol_failure("structured result size is not representable"))?;
    if encoded_bytes > prepared.call().limits().output_bytes() {
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
            output: rendered.output_truncation(),
            model: Truncation::Complete,
            human: Truncation::Complete,
        },
        progress_count,
    )
    .map_err(|_| protocol_failure("terminal Git result is invalid"))
}

pub fn terminal_failure(
    prepared: &peritus_tool_protocol::PreparedToolCall,
    started_at: AuthorityInstant,
    completed_at: AuthorityInstant,
    failure: &DispatchFailure,
    progress_count: u64,
) -> Result<ToolResult, DispatchFailure> {
    let timing = ToolTiming::new(started_at, completed_at)
        .map_err(|_| protocol_failure("Git failure timing is invalid"))?;
    ToolResult::failure(
        prepared,
        failure.status(),
        failure.failure().clone(),
        None,
        failure.failure().detail().clone(),
        failure.failure().detail().clone(),
        Vec::new(),
        timing,
        TruncationMetadata {
            output: Truncation::Complete,
            model: Truncation::Complete,
            human: Truncation::Complete,
        },
        progress_count,
    )
    .map_err(|_| protocol_failure("terminal Git failure is invalid"))
}

pub fn tool_failure(error: &GitToolError) -> DispatchFailure {
    let (category, subsystem) = match error.kind() {
        GitToolErrorKind::Git | GitToolErrorKind::Workspace => {
            (FailureCategory::Workspace, ResponsibleSubsystem::Workspace)
        }
        GitToolErrorKind::Unsupported => {
            (FailureCategory::Infrastructure, ResponsibleSubsystem::Tool)
        }
        GitToolErrorKind::InvalidInput | GitToolErrorKind::Protocol => {
            (FailureCategory::Protocol, ResponsibleSubsystem::Tool)
        }
    };
    failure(category, subsystem, error.code(), error.detail(), error.recovery())
}

pub fn workspace_failure(error: &WorkspaceError) -> DispatchFailure {
    let category = match error.code() {
        WorkspaceErrorCode::AuthorizationMismatch
        | WorkspaceErrorCode::MissingDispatch
        | WorkspaceErrorCode::ReceiptReused
        | WorkspaceErrorCode::StaleLease => FailureCategory::Authorization,
        WorkspaceErrorCode::Artifact => FailureCategory::Artifact,
        WorkspaceErrorCode::Indeterminate => FailureCategory::Indeterminate,
        _ => FailureCategory::Workspace,
    };
    let recovery = match error.recovery() {
        WorkspaceRecovery::CorrectRequest => RecoveryClass::CorrectInput,
        WorkspaceRecovery::Reauthorize => RecoveryClass::Reauthorize,
        WorkspaceRecovery::Reobserve => RecoveryClass::Reobserve,
        WorkspaceRecovery::Reconcile | WorkspaceRecovery::Quarantine => RecoveryClass::Reconcile,
    };
    failure(
        category,
        ResponsibleSubsystem::Workspace,
        error.code().as_str(),
        error.detail(),
        recovery,
    )
}

pub fn protocol_failure(detail: &'static str) -> DispatchFailure {
    failure(
        FailureCategory::Protocol,
        ResponsibleSubsystem::Protocol,
        GitToolErrorKind::Protocol.code(),
        detail,
        RecoveryClass::CorrectInput,
    )
}

pub fn unsupported_failure() -> DispatchFailure {
    failure(
        FailureCategory::Infrastructure,
        ResponsibleSubsystem::Tool,
        GitToolErrorKind::Unsupported.code(),
        "C1 has no authorized merge-delivery operation",
        RecoveryClass::SelectSupportedOperation,
    )
}

fn failure(
    category: FailureCategory,
    subsystem: ResponsibleSubsystem,
    code: &str,
    detail: &str,
    recovery: RecoveryClass,
) -> DispatchFailure {
    let (status, retryability, route) = match recovery {
        RecoveryClass::CorrectInput | RecoveryClass::SelectSupportedOperation => {
            (ResultStatus::Failed, Retryability::NewAction, RecoveryRoute::None)
        }
        RecoveryClass::Reauthorize => {
            (ResultStatus::Failed, Retryability::NewAction, RecoveryRoute::Reauthorize)
        }
        RecoveryClass::Reobserve => {
            (
                ResultStatus::Indeterminate,
                Retryability::AfterRecovery,
                RecoveryRoute::ReconcileWorkspace,
            )
        }
        RecoveryClass::Reconcile => {
            (
                ResultStatus::Indeterminate,
                Retryability::AfterRecovery,
                RecoveryRoute::ReconcileWorkspace,
            )
        }
    };
    let failure = ToolFailure::new(
        category,
        bounded(code),
        subsystem,
        retryability,
        route,
        bounded(detail),
    );
    DispatchFailure::new(status, failure)
        .expect("non-success static dispatch failure is valid")
}

pub fn cancellation_failure(reason: CancellationReason) -> DispatchFailure {
    let (status, category, code, detail) = match reason {
        CancellationReason::Deadline => (
            ResultStatus::TimedOut,
            FailureCategory::Timeout,
            "PERITUS-GIT-TOOL-DEADLINE",
            "immutable Git observation deadline elapsed before execution",
        ),
        CancellationReason::Requested
        | CancellationReason::Shutdown
        | CancellationReason::Recovery => (
            ResultStatus::Cancelled,
            FailureCategory::Cancelled,
            "PERITUS-GIT-TOOL-CANCELLED",
            "immutable Git observation was cancelled before execution",
        ),
    };
    DispatchFailure::new(
        status,
        ToolFailure::new(
            category,
            bounded(code),
            ResponsibleSubsystem::Tool,
            Retryability::Never,
            RecoveryRoute::None,
            bounded(detail),
        ),
    )
    .expect("Git observation cancellation is a non-success result")
}

fn bounded(value: &str) -> BoundedText {
    BoundedText::new(value.to_owned()).expect("static Git failure text is bounded")
}
