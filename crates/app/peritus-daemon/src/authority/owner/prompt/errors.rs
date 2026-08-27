//! Stable daemon error mapping for prompt ownership operations.

use crate::prompt::{PromptBrokerError, PromptBrokerErrorKind};
use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

pub(super) fn prompt_error(error: PromptBrokerError) -> DaemonError {
    let (code, recovery) = match error.kind() {
        PromptBrokerErrorKind::InvalidLimit
        | PromptBrokerErrorKind::ConflictingRegistration
        | PromptBrokerErrorKind::Protocol
        | PromptBrokerErrorKind::ApprovalChallenge
        | PromptBrokerErrorKind::ApprovalAuthorityMissing => {
            (DaemonErrorCode::InvalidInput, DaemonRecovery::CorrectRequest)
        }
        PromptBrokerErrorKind::CapacityExceeded => {
            (DaemonErrorCode::ResourceLimit, DaemonRecovery::Retry)
        }
        PromptBrokerErrorKind::ListingLimitExceeded => {
            (DaemonErrorCode::ResourceLimit, DaemonRecovery::CorrectRequest)
        }
        PromptBrokerErrorKind::ActorMismatch
        | PromptBrokerErrorKind::SessionMismatch
        | PromptBrokerErrorKind::ApprovalAuthentication => {
            (DaemonErrorCode::Unauthorized, DaemonRecovery::CorrectRequest)
        }
        PromptBrokerErrorKind::StillAwaiting => (DaemonErrorCode::NotReady, DaemonRecovery::Retry),
        PromptBrokerErrorKind::DuplicateRegistration
        | PromptBrokerErrorKind::NotFound
        | PromptBrokerErrorKind::BindingMismatch
        | PromptBrokerErrorKind::StaleRevision
        | PromptBrokerErrorKind::StaleCancellationGeneration
        | PromptBrokerErrorKind::Cancelled
        | PromptBrokerErrorKind::DuplicateResponse
        | PromptBrokerErrorKind::ConflictingResponse
        | PromptBrokerErrorKind::StaleCredentialRegistry
        | PromptBrokerErrorKind::StaleAuthorityEpoch => {
            (DaemonErrorCode::RecoveryRequired, DaemonRecovery::Reconcile)
        }
    };
    DaemonError::with_source(
        code,
        recovery,
        "operate on prompt ownership",
        error.to_string(),
        error,
    )
}

pub(super) fn protocol_encoding_error(
    error: peritus_app_protocol::AppProtocolError,
) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::InvalidInput,
        DaemonRecovery::CorrectRequest,
        "encode durable prompt binding",
        "prompt binding is not canonical within production limits",
        error,
    )
}

pub(super) fn approval_registry_error(error: peritus_approval::ApprovalError) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::ReadOnly,
        "decode current approval registry",
        format!("current durable approval registry payload is invalid ({})", error.code()),
    )
}

pub(super) fn missing_authority_epoch() -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::RecoveryRequired,
        DaemonRecovery::Reconcile,
        "authenticate prompt approval",
        "durable authority epoch is not initialized",
    )
}

pub(super) fn prompt_mismatch(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::InvalidInput,
        DaemonRecovery::CorrectRequest,
        "match prompt target",
        detail,
    )
}

pub(super) fn prompt_stale(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::RecoveryRequired,
        DaemonRecovery::Reconcile,
        "validate prompt freshness",
        detail,
    )
}

pub(super) fn ownership_error(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::Unauthorized,
        DaemonRecovery::CorrectRequest,
        "authorize prompt ownership",
        detail,
    )
}
