//! Typed error translation for the artifact authority boundary.

use peritus_types::{ActorId, SessionId};

use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

pub(super) fn require_owner(
    owner: ActorId,
    owner_session: SessionId,
    actor_id: ActorId,
    session_id: SessionId,
) -> Result<(), DaemonError> {
    if owner == actor_id && owner_session == session_id {
        Ok(())
    } else {
        Err(DaemonError::new(
            DaemonErrorCode::Unauthorized,
            DaemonRecovery::CorrectRequest,
            "access artifact transfer",
            "artifact transfer belongs to another actor or session",
        ))
    }
}

pub(super) fn transfer_error(error: peritus_app_protocol::ArtifactTransferError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::InvalidInput,
        DaemonRecovery::CorrectRequest,
        "apply artifact transfer",
        error.to_string(),
        error,
    )
}

pub(super) fn store_error(error: peritus_artifact_store::ArtifactStoreError) -> DaemonError {
    let (code, recovery) = match error.recovery_class() {
        peritus_artifact_store::RecoveryClass::CorrectRequest => {
            (DaemonErrorCode::InvalidInput, DaemonRecovery::CorrectRequest)
        }
        peritus_artifact_store::RecoveryClass::Retry => {
            (DaemonErrorCode::Storage, DaemonRecovery::Retry)
        }
        peritus_artifact_store::RecoveryClass::RecoverStore => {
            (DaemonErrorCode::RecoveryRequired, DaemonRecovery::Reconcile)
        }
        peritus_artifact_store::RecoveryClass::TerminalIntegrity => {
            (DaemonErrorCode::CorruptState, DaemonRecovery::ReadOnly)
        }
        _ => (DaemonErrorCode::Storage, DaemonRecovery::Reconcile),
    };
    DaemonError::with_source(code, recovery, "access artifact store", error.to_string(), error)
}

pub(super) fn journal_error(error: peritus_journal::JournalError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Storage,
        DaemonRecovery::Reconcile,
        error.operation(),
        error.to_string(),
        error,
    )
}

pub(super) fn invalid(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::InvalidInput,
        DaemonRecovery::CorrectRequest,
        "apply artifact transfer",
        detail,
    )
}

pub(super) fn resource_limit(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::ResourceLimit,
        DaemonRecovery::Retry,
        "apply artifact transfer",
        detail,
    )
}

pub(super) fn corrupt(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::ReadOnly,
        "apply artifact transfer",
        detail,
    )
}
