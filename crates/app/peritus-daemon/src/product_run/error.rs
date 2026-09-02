//! Product-run service and configuration failures.

use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProductRunServiceError {
    Duplicate,
    NotFound,
    ProviderUnavailable,
    WorkspaceUnavailable,
    InvalidState,
    InvalidMessage,
    Unavailable,
}

pub(super) fn filesystem(error: std::io::Error) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Storage,
        DaemonRecovery::Reconcile,
        "access product-run state",
        "product-run state is unavailable",
        error,
    )
}

pub(super) fn invalid(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::InvalidInput,
        DaemonRecovery::CorrectRequest,
        "configure product runs",
        detail,
    )
}
