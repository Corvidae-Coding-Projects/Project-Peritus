//! Product-run service and configuration failures.

use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProductRunServiceError {
    Control(peritus_product_runner::control::ControlError),
    Duplicate,
    NotFound,
    ProviderUnavailable,
    EffortUnsupported,
    WorkspaceUnavailable,
    GitRequired,
    InvalidState,
    InvalidMessage,
    Unavailable,
}

impl ProductRunServiceError {
    pub(crate) const fn response(self) -> peritus_app_protocol::AppResponsePayload {
        use peritus_app_protocol::{AppErrorCode as Code, AppProtocolError, AppResponsePayload};
        use peritus_product_runner::control::ControlError;
        let code = match self {
            Self::Duplicate
            | Self::InvalidState
            | Self::Control(ControlError::IdempotencyConflict) => Code::IdempotencyConflict,
            Self::NotFound | Self::Control(ControlError::NotFound) => Code::InvalidIdentifier,
            Self::ProviderUnavailable
            | Self::WorkspaceUnavailable
            | Self::Control(ControlError::StaleRevision) => Code::StaleRevision,
            Self::InvalidMessage | Self::Control(ControlError::InvalidInput) => {
                Code::MalformedFrame
            }
            Self::Unavailable => Code::Backpressure,
            Self::GitRequired | Self::EffortUnsupported => Code::MissingRequiredFeature,
            Self::Control(ControlError::Capacity) => Code::LimitExceeded,
            Self::Control(ControlError::ScopeMismatch) => Code::SessionMismatch,
            Self::Control(ControlError::UnsupportedSchema) => Code::UnsupportedSchema,
        };
        AppResponsePayload::Error(AppProtocolError::new(code, None))
    }
}
impl From<crate::product_control::ControlStoreError> for ProductRunServiceError {
    fn from(value: crate::product_control::ControlStoreError) -> Self {
        match value {
            crate::product_control::ControlStoreError::Control(error) => Self::Control(error),
            _ => Self::Unavailable,
        }
    }
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
