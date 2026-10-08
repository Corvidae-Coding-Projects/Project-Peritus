//! C2 failure normalization for quality execution.

use peritus_process::{ProcessError, RecoveryClass};
use peritus_tool_protocol::{
    FailureCategory, RecoveryRoute, ResponsibleSubsystem, ResultStatus, Retryability,
};
use peritus_tool_router::DispatchFailure;

use crate::dispatcher::dispatch_failure;

pub fn artifact(error: &peritus_artifact_store::ArtifactStoreError) -> DispatchFailure {
    dispatch_failure(
        ResultStatus::Failed,
        FailureCategory::Artifact,
        error.code().as_str(),
        ResponsibleSubsystem::ArtifactStore,
        Retryability::AfterRecovery,
        RecoveryRoute::RepublishArtifact,
        &error.to_string(),
    )
}

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

/// Retains a completed effect for publication/projection retry, never a new process action.
pub fn settlement_projection(source: &DispatchFailure) -> DispatchFailure {
    let failure = source.failure();
    dispatch_failure(
        source.status(),
        failure.category(),
        failure.code().as_str(),
        failure.subsystem(),
        Retryability::AfterRecovery,
        if failure.recovery() == RecoveryRoute::RepublishArtifact {
            RecoveryRoute::RepublishArtifact
        } else {
            RecoveryRoute::ReconcileProcess
        },
        failure.detail().as_str(),
    )
}
