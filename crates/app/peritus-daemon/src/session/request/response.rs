//! Stable public response/error mapping for authenticated request dispatch.

use crate::{
    DaemonError, DaemonErrorCode, DaemonRecovery,
    product_run::ProductRunServiceError,
    terminal::{TerminalBridgeError, TerminalBridgeErrorKind},
};
use peritus_app_protocol::{
    AppErrorCode, AppMessage, AppProtocolError, AppProtocolLimits, AppRequestEnvelope,
    AppResponsePayload, OperationAcknowledgement, encode_app_message,
};

pub(super) const fn product_run_error(error: ProductRunServiceError) -> AppResponsePayload {
    error.response()
}

pub(super) fn product_run_projection(
    result: Result<AppResponsePayload, ProductRunServiceError>,
) -> AppResponsePayload {
    match result {
        Ok(payload) => payload,
        Err(error) => product_run_error(error),
    }
}

pub(super) fn product_run_collection(
    result: Result<AppResponsePayload, ProductRunServiceError>,
) -> AppResponsePayload {
    match result {
        Ok(payload) => payload,
        Err(error) => product_run_error(error),
    }
}

pub(super) const fn acknowledged(request: &AppRequestEnvelope) -> AppResponsePayload {
    AppResponsePayload::Acknowledged(OperationAcknowledgement::new(request.request_id()))
}

pub(super) fn canonical_request_frame(
    request: &AppRequestEnvelope,
    limits: AppProtocolLimits,
) -> Result<Vec<u8>, DaemonError> {
    encode_app_message(&AppMessage::Request(request.clone()), limits).map_err(|error| {
        DaemonError::with_source(
            DaemonErrorCode::InvalidInput,
            DaemonRecovery::CorrectRequest,
            "encode prompt request settlement",
            "accepted prompt request cannot be canonically re-encoded",
            error,
        )
    })
}

pub(super) fn prompt_error_payload(error: &DaemonError) -> AppResponsePayload {
    let code = match error.operation() {
        "authorize prompt ownership" => AppErrorCode::SessionMismatch,
        "validate prompt freshness" => AppErrorCode::PromptStale,
        "match prompt target" => AppErrorCode::PromptMismatch,
        _ if error.code_kind() == DaemonErrorCode::Unauthorized => AppErrorCode::PromptMismatch,
        _ if error.code_kind() == DaemonErrorCode::RecoveryRequired => AppErrorCode::PromptStale,
        _ => public_error_code(error),
    };
    AppResponsePayload::Error(AppProtocolError::new(code, None))
}

pub(super) const fn terminal_operation(
    request_id: peritus_app_protocol::RequestId,
    result: Result<(), TerminalBridgeError>,
) -> AppResponsePayload {
    match result {
        Ok(()) => AppResponsePayload::Acknowledged(OperationAcknowledgement::new(request_id)),
        Err(error) => terminal_error_payload(&error),
    }
}

pub(super) const fn terminal_error_payload(error: &TerminalBridgeError) -> AppResponsePayload {
    let code = match error.kind() {
        TerminalBridgeErrorKind::Capacity => AppErrorCode::LimitExceeded,
        TerminalBridgeErrorKind::Backpressure => AppErrorCode::Backpressure,
        TerminalBridgeErrorKind::OwnershipMismatch => AppErrorCode::SessionMismatch,
        TerminalBridgeErrorKind::InvalidLimit => AppErrorCode::Internal,
        TerminalBridgeErrorKind::ProcessNotRegistered
        | TerminalBridgeErrorKind::RegistrationConflict
        | TerminalBridgeErrorKind::NotPty
        | TerminalBridgeErrorKind::BirthIdentityUnavailable
        | TerminalBridgeErrorKind::ProcessIdentityMismatch
        | TerminalBridgeErrorKind::ReplayUnavailable
        | TerminalBridgeErrorKind::Protocol
        | TerminalBridgeErrorKind::Process
        | TerminalBridgeErrorKind::ProcessNotLive => AppErrorCode::TerminalState,
    };
    AppResponsePayload::Error(AppProtocolError::new(code, None))
}

pub(super) const fn public_error_code(error: &DaemonError) -> AppErrorCode {
    match error.code_kind() {
        DaemonErrorCode::InvalidInput => AppErrorCode::SubscriptionState,
        DaemonErrorCode::ResourceLimit => AppErrorCode::Backpressure,
        DaemonErrorCode::Unauthorized => AppErrorCode::ReadOnly,
        DaemonErrorCode::NotReady => AppErrorCode::NotReady,
        _ => AppErrorCode::Internal,
    }
}

pub(super) const fn daemon_error_payload(error: &DaemonError) -> AppResponsePayload {
    AppResponsePayload::Error(AppProtocolError::new(public_error_code(error), None))
}
