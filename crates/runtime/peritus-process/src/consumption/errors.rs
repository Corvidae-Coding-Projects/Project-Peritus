use crate::{ErrorCode, ProcessError, ProcessOperation, RecoveryClass};

pub(super) const fn overlap_error() -> ProcessError {
    ProcessError::new(
        ErrorCode::InvalidInput,
        ProcessOperation::OpenStore,
        RecoveryClass::CorrectRequest,
        "process registry and agent-visible workspace roots overlap",
    )
}

pub(super) const fn reused() -> ProcessError {
    ProcessError::new(
        ErrorCode::ReceiptReused,
        ProcessOperation::Authorize,
        RecoveryClass::Reauthorize,
        "action/process authority was already durably consumed",
    )
}

pub(super) const fn store_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::Persistence,
        ProcessOperation::Persist,
        RecoveryClass::ReopenAndReconcile,
        detail,
    )
}

pub(crate) fn store_cause(
    detail: &'static str,
    source: impl std::error::Error + Send + Sync + 'static,
) -> ProcessError {
    ProcessError::with_source(ErrorCode::Persistence, ProcessOperation::Persist,
        RecoveryClass::ReopenAndReconcile, detail, source)
}
