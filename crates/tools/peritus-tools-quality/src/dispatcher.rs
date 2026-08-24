//! Router-permit-only quality dispatchers.

mod discover;
mod run;

use peritus_tool_protocol::{
    FailureCategory, RecoveryRoute, ResponsibleSubsystem, ResultStatus, Retryability, ToolFailure,
};
use peritus_tool_router::DispatchFailure;

pub use discover::QualityDiscoverDispatcher;
pub use run::QualityRunDispatcher;

use crate::render::text;

pub fn dispatch_failure(
    status: ResultStatus,
    category: FailureCategory,
    code: &str,
    subsystem: ResponsibleSubsystem,
    retryability: Retryability,
    recovery: RecoveryRoute,
    detail: &str,
) -> DispatchFailure {
    let failure =
        ToolFailure::new(category, text(code), subsystem, retryability, recovery, text(detail));
    DispatchFailure::new(status, failure).expect("quality dispatch uses a non-success status")
}

pub fn adapter_failure(code: &str, detail: &str) -> DispatchFailure {
    dispatch_failure(
        ResultStatus::Failed,
        FailureCategory::Protocol,
        code,
        ResponsibleSubsystem::Tool,
        Retryability::Never,
        RecoveryRoute::None,
        detail,
    )
}
