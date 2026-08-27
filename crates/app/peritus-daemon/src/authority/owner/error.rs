//! Shared lifecycle, storage, and owner-channel error translation.

use peritus_journal::{JournalError, JournalErrorKind};

use crate::{DaemonError, DaemonErrorCode, DaemonLifecycle, DaemonRecovery};

pub(super) fn require_mutation(lifecycle: &DaemonLifecycle) -> Result<(), DaemonError> {
    if lifecycle.readiness().mutation_ready() {
        Ok(())
    } else {
        Err(not_ready("mutation intake is not ready"))
    }
}

pub(super) fn require_diagnostic(lifecycle: &DaemonLifecycle) -> Result<(), DaemonError> {
    if lifecycle.readiness().diagnostic_ready() {
        Ok(())
    } else {
        Err(not_ready("diagnostic intake is not ready"))
    }
}

pub(super) fn journal_error(error: JournalError) -> DaemonError {
    let (code, recovery) = match error.kind() {
        JournalErrorKind::InvalidInput
        | JournalErrorKind::EmptyBatch
        | JournalErrorKind::DuplicateIdentity
        | JournalErrorKind::NonCanonicalOrder => {
            (DaemonErrorCode::InvalidInput, DaemonRecovery::CorrectRequest)
        }
        JournalErrorKind::Busy | JournalErrorKind::Storage => {
            (DaemonErrorCode::Storage, DaemonRecovery::Retry)
        }
        JournalErrorKind::IndeterminateCommit => {
            (DaemonErrorCode::RecoveryRequired, DaemonRecovery::Reconcile)
        }
        JournalErrorKind::CorruptJournal | JournalErrorKind::UnsupportedSchema => {
            (DaemonErrorCode::CorruptState, DaemonRecovery::ReadOnly)
        }
        JournalErrorKind::SequenceOverflow
        | JournalErrorKind::StaleHead
        | JournalErrorKind::IdempotencyConflict
        | JournalErrorKind::MissingArtifact
        | JournalErrorKind::StaleAuthorityEpoch
        | JournalErrorKind::StaleRegistry
        | JournalErrorKind::ReadOnly
        | JournalErrorKind::NotFound => {
            (DaemonErrorCode::RecoveryRequired, DaemonRecovery::Reconcile)
        }
    };
    DaemonError::with_source(code, recovery, error.operation(), error.to_string(), error)
}

pub(super) fn owner_stopped() -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::Worker,
        DaemonRecovery::Reconcile,
        "communicate with authority owner",
        "authority owner is no longer available",
    )
}

fn not_ready(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::NotReady,
        DaemonRecovery::Retry,
        "admit authority request",
        detail,
    )
}
