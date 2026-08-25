//! Closed consistency rules for quality terminal status and typed failure metadata.

use peritus_tool_protocol::{
    FailureCategory, RecoveryRoute, ResultStatus, Retryability, ToolFailure,
};

use super::DecodedOutcome;

pub(super) fn terminal_contract_consistent(
    status: ResultStatus,
    failure: Option<&ToolFailure>,
    outcome: Option<DecodedOutcome>,
) -> bool {
    match (status, failure, outcome) {
        (ResultStatus::Succeeded, None, Some(DecodedOutcome::Passed)) => true,
        (
            ResultStatus::Failed,
            Some(failure),
            Some(DecodedOutcome::PredicateFailed | DecodedOutcome::UnsuccessfulExit),
        ) => exact_failure(
            failure,
            FailureCategory::Execution,
            Retryability::Never,
            RecoveryRoute::None,
        ),
        (ResultStatus::Failed, Some(failure), Some(DecodedOutcome::InvalidResult)) => {
            exact_failure(
                failure,
                FailureCategory::Infrastructure,
                Retryability::NewAction,
                RecoveryRoute::Reauthorize,
            )
        }
        (ResultStatus::Failed, Some(failure), Some(DecodedOutcome::Infrastructure)) => {
            exact_failure(
                failure,
                FailureCategory::Infrastructure,
                Retryability::AfterRecovery,
                RecoveryRoute::ReconcileProcess,
            )
        }
        (ResultStatus::Cancelled, Some(failure), Some(DecodedOutcome::Infrastructure)) => {
            exact_failure(
                failure,
                FailureCategory::Cancelled,
                Retryability::NewAction,
                RecoveryRoute::Reauthorize,
            )
        }
        (ResultStatus::TimedOut, Some(failure), Some(DecodedOutcome::Infrastructure)) => {
            exact_failure(
                failure,
                FailureCategory::Timeout,
                Retryability::NewAction,
                RecoveryRoute::Reauthorize,
            )
        }
        (ResultStatus::Indeterminate, Some(failure), Some(DecodedOutcome::Infrastructure)) => {
            exact_failure(
                failure,
                FailureCategory::Indeterminate,
                Retryability::AfterRecovery,
                RecoveryRoute::ReconcileProcess,
            )
        }
        _ => false,
    }
}

fn exact_failure(
    failure: &ToolFailure,
    category: FailureCategory,
    retryability: Retryability,
    recovery: RecoveryRoute,
) -> bool {
    failure.category() == category
        && failure.retryability() == retryability
        && failure.recovery() == recovery
}
