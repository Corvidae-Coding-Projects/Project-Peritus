//! Stable public response/error mapping for authenticated request dispatch.

use crate::{
    DaemonError, DaemonErrorCode, DaemonRecovery, product_run::ProductRunServiceError,
    terminal::TerminalBridgeError,
};
use peritus_app_protocol::{
    AppDiagnostic, AppErrorCode, AppMessage, AppProtocolError, AppProtocolLimits,
    AppRequestEnvelope, AppResponsePayload, OperationAcknowledgement, ResponsibleSubsystem,
    RetryDisposition, encode_app_message,
};

pub(super) fn product_run_observations(
    service: &crate::product_run::ProductRunService,
    query: peritus_app_protocol::ProductRunQuery,
) -> AppResponsePayload {
    match service.query_observations(query) {
        Ok(observations) => AppResponsePayload::ProductRunObservations(observations),
        Err(error) => product_run_error(error),
    }
}

pub(super) fn product_run_error(error: ProductRunServiceError) -> AppResponsePayload {
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
    AppResponsePayload::Error(AppProtocolError::new(code, daemon_diagnostic(error)))
}

pub(super) fn terminal_operation(
    request_id: peritus_app_protocol::RequestId,
    result: Result<(), TerminalBridgeError>,
) -> AppResponsePayload {
    match result {
        Ok(()) => AppResponsePayload::Acknowledged(OperationAcknowledgement::new(request_id)),
        Err(error) => terminal_error_payload(&error),
    }
}

pub(super) const fn terminal_error_payload(error: &TerminalBridgeError) -> AppResponsePayload {
    AppResponsePayload::Error(error.protocol_error())
}

pub(super) const fn public_error_code(error: &DaemonError) -> AppErrorCode {
    match error.code_kind() {
        DaemonErrorCode::InvalidInput => AppErrorCode::MalformedFrame,
        DaemonErrorCode::ResourceLimit => AppErrorCode::LimitExceeded,
        DaemonErrorCode::Unauthorized => AppErrorCode::ReadOnly,
        DaemonErrorCode::NotReady => AppErrorCode::NotReady,
        _ => AppErrorCode::Internal,
    }
}

pub(super) const fn subscription_error_code(error: &DaemonError) -> AppErrorCode {
    match error.code_kind() {
        DaemonErrorCode::InvalidInput => AppErrorCode::SubscriptionState,
        DaemonErrorCode::ResourceLimit => AppErrorCode::Backpressure,
        _ => public_error_code(error),
    }
}

pub(super) fn artifact_error_payload(error: &DaemonError) -> AppResponsePayload {
    let (code, retry, subsystem) = match error.code_kind() {
        DaemonErrorCode::InvalidInput => (
            AppErrorCode::ArtifactState,
            RetryDisposition::NewRequest,
            ResponsibleSubsystem::Artifact,
        ),
        DaemonErrorCode::ResourceLimit => (
            AppErrorCode::LimitExceeded,
            RetryDisposition::AfterRecovery,
            ResponsibleSubsystem::Artifact,
        ),
        DaemonErrorCode::Unauthorized => {
            (AppErrorCode::ReadOnly, RetryDisposition::AfterRecovery, ResponsibleSubsystem::Session)
        }
        DaemonErrorCode::NotReady => {
            (AppErrorCode::NotReady, RetryDisposition::AfterRecovery, ResponsibleSubsystem::Daemon)
        }
        _ => (
            AppErrorCode::Internal,
            recovery_retry(error.recovery()),
            ResponsibleSubsystem::Artifact,
        ),
    };
    AppResponsePayload::Error(AppProtocolError::classified(
        code,
        retry,
        subsystem,
        daemon_diagnostic(error),
    ))
}

pub(super) fn constrain_error_diagnostic(
    payload: AppResponsePayload,
    maximum: usize,
) -> AppResponsePayload {
    match payload {
        AppResponsePayload::Error(error) => {
            AppResponsePayload::Error(error.constrained_diagnostic(maximum))
        }
        other => other,
    }
}

const fn recovery_retry(recovery: DaemonRecovery) -> RetryDisposition {
    match recovery {
        DaemonRecovery::CorrectRequest => RetryDisposition::NewRequest,
        DaemonRecovery::Retry => RetryDisposition::SameRequest,
        DaemonRecovery::Reconcile | DaemonRecovery::ReadOnly | DaemonRecovery::Operator => {
            RetryDisposition::AfterRecovery
        }
    }
}

fn daemon_diagnostic(error: &DaemonError) -> Option<AppDiagnostic> {
    const MAX_BYTES: usize = 1_024;
    let mut value = format!("{}: {}", error.operation(), error.detail());
    if value.len() > MAX_BYTES {
        let mut end = MAX_BYTES - 3;
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        value.truncate(end);
        value.push_str("...");
    }
    AppDiagnostic::new(value, MAX_BYTES).ok()
}
