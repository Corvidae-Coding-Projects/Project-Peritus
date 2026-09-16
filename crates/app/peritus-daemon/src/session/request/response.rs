//! Stable public response/error mapping for authenticated request dispatch.

use crate::{
    DaemonError, DaemonErrorCode, DaemonRecovery,
    product_run::ProductRunServiceError,
    terminal::{TerminalBridgeError, TerminalBridgeErrorKind},
};
use peritus_app_protocol::{
    AppDiagnostic, AppErrorCode, AppMessage, AppProtocolError, AppProtocolLimits,
    AppRequestEnvelope, AppResponsePayload, OperationAcknowledgement, ResponsibleSubsystem,
    RetryDisposition, encode_app_message,
};

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
    AppResponsePayload::Error(AppProtocolError::new(code, daemon_diagnostic(error)))
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

#[cfg(test)]
mod tests {
    use super::*;
    use peritus_app_protocol::{
        AppMessage, AppResponseEnvelope, CorrelationId, ProtocolContext, ProtocolId,
        ProtocolVersion, RequestId, decode_app_message,
    };
    use peritus_types::SessionId;

    #[test]
    fn negotiated_limit_constrains_error_without_dropping_the_connection_frame() {
        let production = AppProtocolLimits::PRODUCTION;
        let limits = AppProtocolLimits::new(
            production.codec(),
            production.max_versions(),
            production.max_features(),
            production.max_idempotency_entries(),
            production.max_topics(),
            production.max_in_flight_events(),
            production.max_artifact_chunk_bytes(),
            production.max_prompt_choices(),
            production.max_terminal_chunk_bytes(),
            24,
            production.max_remaining_work_items(),
        )
        .unwrap();
        let source = DaemonError::new(
            DaemonErrorCode::Storage,
            DaemonRecovery::Reconcile,
            "persist conversation state",
            "a diagnostic that is deliberately longer than twenty-four bytes",
        );
        let payload = constrain_error_diagnostic(
            artifact_error_payload(&source),
            limits.max_diagnostic_bytes(),
        );
        let context = ProtocolContext::new(
            ProtocolId::new([1; 16]).unwrap(),
            ProtocolVersion::new(1, 0).unwrap(),
            SessionId::new([2; 16]).unwrap(),
        );
        let message = AppMessage::Response(AppResponseEnvelope::new(
            context,
            RequestId::new([3; 16]).unwrap(),
            CorrelationId::new([4; 16]).unwrap(),
            payload,
        ));

        let encoded = encode_app_message(&message, limits).expect("bounded response encodes");
        let AppMessage::Response(response) = decode_app_message(&encoded, limits).unwrap() else {
            panic!("response");
        };
        let AppResponsePayload::Error(error) = response.payload() else {
            panic!("error response");
        };
        assert!(error.diagnostic().unwrap().as_str().len() <= 24);
        assert_eq!(error.code(), AppErrorCode::Internal);
        assert_eq!(error.subsystem(), ResponsibleSubsystem::Artifact);
    }

    #[test]
    fn artifact_registration_errors_are_not_subscription_failures() {
        let duplicate = DaemonError::new(
            DaemonErrorCode::InvalidInput,
            DaemonRecovery::CorrectRequest,
            "register connection artifact transfer",
            "artifact transfer identity is already active on this connection",
        );
        let AppResponsePayload::Error(error) = artifact_error_payload(&duplicate) else {
            panic!("error response");
        };
        assert_eq!(error.code(), AppErrorCode::ArtifactState);
        assert_eq!(error.retry(), RetryDisposition::NewRequest);
        assert_eq!(error.subsystem(), ResponsibleSubsystem::Artifact);
    }
}
