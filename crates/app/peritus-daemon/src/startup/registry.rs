//! Strict startup reconciliation of the public approval credential registry.

use std::{
    fs::{self, File},
    io::Read,
};

use peritus_approval::{
    CredentialRegistrySnapshot, MAX_CREDENTIAL_REGISTRY_PREIMAGE_BYTES, decode_credential_registry,
};
use peritus_journal::{
    CredentialRegistryInstall, CurrentCredentialRegistry, JournalError, JournalErrorKind,
    SqliteJournal,
};

use crate::{ApprovalRegistryDeclaration, DaemonError, DaemonErrorCode, DaemonRecovery};

#[cfg(test)]
mod tests;

struct ConfiguredRegistry {
    snapshot: CredentialRegistrySnapshot,
    payload: Vec<u8>,
    generation: u64,
}

pub(super) fn bootstrap(
    journal: &mut SqliteJournal,
    declaration: &ApprovalRegistryDeclaration,
) -> Result<(), DaemonError> {
    let configured = load_configured(declaration)?;
    match journal.current_credential_registry() {
        Ok(current) => reconcile_current(journal, configured, &current),
        Err(error) if error.kind() == JournalErrorKind::NotFound => {
            install(journal, None, configured)
        }
        Err(error) => Err(journal_error(error)),
    }
}

fn load_configured(
    declaration: &ApprovalRegistryDeclaration,
) -> Result<ConfiguredRegistry, DaemonError> {
    let path = declaration.payload_file();
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        input_source(
            "inspect approval credential registry",
            "approval registry payload file cannot be inspected",
            error,
        )
    })?;
    if !metadata.file_type().is_file() {
        return Err(input(
            "inspect approval credential registry",
            "approval registry payload must be a regular file, not a symlink or special file",
        ));
    }
    let canonical = fs::canonicalize(path).map_err(|error| {
        input_source(
            "canonicalize approval credential registry",
            "approval registry payload path cannot be canonicalized",
            error,
        )
    })?;
    if canonical.as_os_str() != path.as_os_str() {
        return Err(input(
            "canonicalize approval credential registry",
            "approval registry payload path contains an alias or symlink component",
        ));
    }
    let bound = u64::try_from(MAX_CREDENTIAL_REGISTRY_PREIMAGE_BYTES).map_err(|_| {
        input(
            "read approval credential registry",
            "compiled approval registry bound cannot be represented by this platform",
        )
    })?;
    if metadata.len() > bound {
        return Err(input(
            "read approval credential registry",
            "approval registry payload exceeds the compiled B1 bound",
        ));
    }
    let read_limit = bound.checked_add(1).ok_or_else(|| {
        input(
            "read approval credential registry",
            "compiled approval registry read bound overflowed",
        )
    })?;
    let file = File::open(path).map_err(|error| {
        input_source(
            "open approval credential registry",
            "approval registry payload file cannot be opened",
            error,
        )
    })?;
    let mut payload = Vec::new();
    file.take(read_limit).read_to_end(&mut payload).map_err(|error| {
        input_source(
            "read approval credential registry",
            "approval registry payload file cannot be read",
            error,
        )
    })?;
    if payload.len() > MAX_CREDENTIAL_REGISTRY_PREIMAGE_BYTES {
        return Err(input(
            "read approval credential registry",
            "approval registry payload changed beyond the compiled B1 bound while being read",
        ));
    }
    let snapshot = decode_configured(&payload)?;
    Ok(ConfiguredRegistry { snapshot, payload, generation: declaration.generation() })
}

fn reconcile_current(
    journal: &mut SqliteJournal,
    configured: ConfiguredRegistry,
    current: &CurrentCredentialRegistry,
) -> Result<(), DaemonError> {
    verify_stored(current)?;
    let configured_revision = configured.snapshot.revision().get();
    if configured_revision == current.revision()
        && configured.generation == current.generation()
        && configured.snapshot.digest().map_err(|_| configured_encoding_error())?
            == current.digest()
        && configured.payload.as_slice() == current.snapshot_payload().map_err(journal_error)?
    {
        return Ok(());
    }
    if current.revision().checked_add(1) == Some(configured_revision)
        && configured.generation > current.generation()
    {
        return install(journal, Some(current.revision()), configured);
    }
    Err(drift_error())
}

fn install(
    journal: &mut SqliteJournal,
    expected_revision: Option<u64>,
    configured: ConfiguredRegistry,
) -> Result<(), DaemonError> {
    let install = CredentialRegistryInstall::new(
        expected_revision,
        configured.generation,
        &configured.snapshot,
    )
    .map_err(|error| {
        if error.kind() == JournalErrorKind::InvalidInput {
            input(
                "plan approval credential registry",
                "configured registry is not an exact positive revision successor",
            )
        } else {
            journal_error(error)
        }
    })?;
    journal.commit_credential_registry(install).map(|_| ()).map_err(journal_error)
}

fn decode_configured(payload: &[u8]) -> Result<CredentialRegistrySnapshot, DaemonError> {
    let snapshot = decode_credential_registry(payload).map_err(|_| configured_encoding_error())?;
    let roundtrip = snapshot.canonical_bytes().map_err(|_| configured_encoding_error())?;
    if roundtrip != payload {
        return Err(configured_encoding_error());
    }
    Ok(snapshot)
}

fn verify_stored(current: &CurrentCredentialRegistry) -> Result<(), DaemonError> {
    let payload = current.snapshot_payload().map_err(journal_error)?;
    let snapshot = decode_credential_registry(payload).map_err(|_| stored_encoding_error())?;
    let roundtrip = snapshot.canonical_bytes().map_err(|_| stored_encoding_error())?;
    let digest = snapshot.digest().map_err(|_| stored_encoding_error())?;
    if roundtrip != payload
        || snapshot.revision().get() != current.revision()
        || digest != current.digest()
    {
        return Err(stored_encoding_error());
    }
    Ok(())
}

fn configured_encoding_error() -> DaemonError {
    input(
        "decode approval credential registry",
        "approval registry payload is not one exact canonical B1 CredentialRegistrySnapshot",
    )
}

fn stored_encoding_error() -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::Operator,
        "verify durable approval credential registry",
        "durable registry row is not an exact canonical B1 CredentialRegistrySnapshot",
    )
}

fn drift_error() -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::RecoveryRequired,
        DaemonRecovery::Reconcile,
        "reconcile approval credential registry",
        "configured registry must exactly match durable current state or be its exact revision successor with a greater generation",
    )
}

fn journal_error(error: JournalError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Storage,
        DaemonRecovery::Reconcile,
        error.operation(),
        error.to_string(),
        error,
    )
}

fn input(operation: &'static str, detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::InvalidInput,
        DaemonRecovery::CorrectRequest,
        operation,
        detail,
    )
}

fn input_source(
    operation: &'static str,
    detail: &'static str,
    error: std::io::Error,
) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::InvalidInput,
        DaemonRecovery::CorrectRequest,
        operation,
        detail,
        error,
    )
}
