//! C2 failure normalization for quality execution.

use peritus_process::{ProcessError, RecoveryClass};
use peritus_tool_protocol::{
    FailureCategory, RecoveryRoute, ResponsibleSubsystem, ResultStatus, Retryability,
};
use peritus_tool_router::DispatchFailure;

use crate::dispatcher::dispatch_failure;

pub fn process(error: &ProcessError) -> DispatchFailure {
    let (status, category, retryability, recovery) = match error.recovery() {
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
        RecoveryClass::RetryPublication => (
            ResultStatus::Failed,
            FailureCategory::Artifact,
            Retryability::AfterRecovery,
            RecoveryRoute::RepublishArtifact,
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
        RecoveryClass::Terminal => (
            ResultStatus::Failed,
            FailureCategory::Execution,
            Retryability::Never,
            RecoveryRoute::None,
        ),
    };
    dispatch_failure(
        status,
        category,
        error.code().as_str(),
        ResponsibleSubsystem::Process,
        retryability,
        recovery,
        &error.to_string(),
    )
}
