//! Stable lower-boundary failure normalization.

use peritus_process::{ProcessError, RecoveryClass};
use peritus_tool_protocol::{
    BoundedText, FailureCategory, RecoveryRoute, ResponsibleSubsystem, ResultStatus, Retryability,
    ToolFailure,
};
use peritus_tool_router::DispatchFailure;

use crate::error::truncate_utf8;

pub fn process(error: &ProcessError) -> DispatchFailure {
    let (status, category, retry, recovery) = match error.recovery() {
        RecoveryClass::CorrectRequest => (
            ResultStatus::Failed,
            FailureCategory::Execution,
            Retryability::NewAction,
            RecoveryRoute::Reauthorize,
        ),
        RecoveryClass::Reauthorize | RecoveryClass::SelectBackend => (
            ResultStatus::Failed,
            FailureCategory::Authorization,
            Retryability::NewAction,
            RecoveryRoute::Reauthorize,
        ),
        RecoveryClass::RetryPreparation
        | RecoveryClass::CancelAndReap
        | RecoveryClass::ReopenAndReconcile
        | RecoveryClass::Quarantine => (
            ResultStatus::Indeterminate,
            FailureCategory::Indeterminate,
            Retryability::AfterRecovery,
            RecoveryRoute::ReconcileProcess,
        ),
        RecoveryClass::RetryPublication => (
            ResultStatus::Failed,
            FailureCategory::Artifact,
            Retryability::AfterRecovery,
            RecoveryRoute::RepublishArtifact,
        ),
        RecoveryClass::Terminal => (
            ResultStatus::Failed,
            FailureCategory::Execution,
            Retryability::Never,
            RecoveryRoute::None,
        ),
    };
    failure(
        status,
        category,
        error.code().as_str(),
        ResponsibleSubsystem::Process,
        retry,
        recovery,
        &error.to_string(),
    )
}

pub fn adapter(code: &str, detail: &str) -> DispatchFailure {
    failure(
        ResultStatus::Failed,
        FailureCategory::Protocol,
        code,
        ResponsibleSubsystem::Tool,
        Retryability::Never,
        RecoveryRoute::None,
        detail,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn failure(
    status: ResultStatus,
    category: FailureCategory,
    code: &str,
    subsystem: ResponsibleSubsystem,
    retryability: Retryability,
    recovery: RecoveryRoute,
    detail: &str,
) -> DispatchFailure {
    let value =
        ToolFailure::new(category, text(code), subsystem, retryability, recovery, text(detail));
    DispatchFailure::new(status, value).expect("non-success dispatch status")
}

fn text(value: &str) -> BoundedText {
    let mut safe = value.replace('\0', "\\0");
    if safe.is_empty() {
        safe.push_str("unspecified");
    }
    truncate_utf8(&mut safe, 16 * 1_024);
    BoundedText::new(safe).expect("sanitized bounded failure text")
}
