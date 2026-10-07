//! Protected resource staging and installed-path revalidation.

use std::{io::Read, path::PathBuf};

use peritus_network::{ManagedProxy, NetworkError, NetworkErrorKind};
use peritus_process::{NativeProtectedHandle, ProcessError};
use peritus_sandbox::{SecretDelivery, SecretRequirement};
use peritus_secrets::{
    DeliveryArtifact, SecretDeliverySession, SecretError, SecretErrorKind,
};
use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};

use crate::{
    MacosError, MacosErrorKind, MacosOperation, ProtectedProxyRoute, ProtectedSecretHandle,
    PreparationCleanup, PreparationProgress, ProxyRoute, RecoveryAction,
    SecretHandleDestination, error,
};

pub(super) struct PreparationOwners {
    proxy: Option<ManagedProxy>,
    secrets: SecretDeliverySession,
}

impl PreparationOwners {
    pub(super) const fn new(secrets: SecretDeliverySession) -> Self {
        Self { proxy: None, secrets }
    }

    pub(super) fn set_proxy(&mut self, proxy: ManagedProxy) {
        self.proxy = Some(proxy);
    }

    pub(super) const fn proxy(&self) -> Option<&ManagedProxy> {
        self.proxy.as_ref()
    }

    pub(super) const fn secrets(&self) -> &SecretDeliverySession {
        &self.secrets
    }

    pub(super) fn cleanup(&mut self, original: MacosError) -> MacosError {
        let secrets = self.secrets.release().err();
        let proxy = self.proxy.take().and_then(|proxy| proxy.shutdown().err());
        let original_cleanup = original.preparation_cleanup();
        let cleanup = PreparationCleanup::new(
            original_cleanup.support_join(),
            original_cleanup.proxy_shutdown() || proxy.is_some(),
            original_cleanup.secret_release() || secrets.is_some(),
        );
        if cleanup.is_complete() {
            return original;
        }
        let mut error = MacosError::new(
            MacosErrorKind::CleanupIncomplete,
            MacosOperation::Prepare,
            RecoveryAction::RetryCleanup,
            format!(
                "native preparation failed with {}; started resource cleanup requires reconciliation",
                original.code()
            ),
        )
        .with_cleanup(cleanup);
        if let Some(source) = secrets.as_ref().map(error::secret_source) {
            error = error.with_source(source);
        } else if let Some(source) = proxy.as_ref().map(error::network_source) {
            error = error.with_source(source);
        }
        error
    }

    pub(super) fn into_parts(self) -> (Option<ManagedProxy>, SecretDeliverySession) {
        (self.proxy, self.secrets)
    }
}

pub(super) fn stage_proxy_handle(proxy: &ManagedProxy) -> Result<ProtectedProxyRoute, MacosError> {
    let handle = proxy.routing_token().expose_bytes(|bytes| {
        NativeProtectedHandle::from_bytes("peritus-macos-proxy-routing-v1", bytes.to_vec())
    });
    ProtectedProxyRoute::new(
        proxy.endpoint().socket_addr(),
        handle.map_err(protected_handle_error)?,
    )
}

pub(super) fn stage_secret_handles<C, P>(
    session: &SecretDeliverySession,
    requirements: &[SecretRequirement],
    should_continue: &C,
    observe_progress: &P,
) -> Result<Vec<ProtectedSecretHandle>, MacosError>
where
    C: Fn() -> bool + ?Sized,
    P: Fn(PreparationProgress) + ?Sized,
{
    if session.artifacts().len() != session.leases().len()
        || session.artifacts().len() != requirements.len()
    {
        return Err(secret_binding_error(
            "prepared secret artifacts, leases, and checked requirements differ",
        ));
    }
    let mut handles = Vec::with_capacity(session.artifacts().len());
    let secret_count = u32::try_from(requirements.len()).map_err(|_| {
        error::limited(MacosOperation::Prepare, "secret count is outside progress range")
    })?;
    for (index, ((requirement, artifact), lease)) in requirements
        .iter()
        .zip(session.artifacts())
        .zip(session.leases())
        .enumerate()
    {
        if lease.reference() != requirement.reference()
            || lease.delivery() != requirement.delivery()
        {
            return Err(secret_binding_error(
                "prepared secret lease differs from the checked requirement",
            ));
        }
        let destination = SecretHandleDestination::from(requirement.delivery());
        validate_artifact_destination(artifact, requirement.delivery())?;
        let label = format!("peritus-macos-secret-v1-{index}");
        let secret_index = u32::try_from(index).map_err(|_| {
            error::limited(MacosOperation::Prepare, "secret index is outside progress range")
        })?;
        let native = match artifact {
            DeliveryArtifact::Environment { material, .. }
            | DeliveryArtifact::Brokered { material, .. } => material.expose(|bytes| {
                stage_secret_reader(
                    label,
                    std::io::Cursor::new(bytes),
                    u64::try_from(bytes.len()).unwrap_or(u64::MAX),
                    secret_index,
                    secret_count,
                    should_continue,
                    observe_progress,
                )
            })?,
            DeliveryArtifact::File { staging_path, .. } => {
                let (file, total_bytes) = open_staged_secret(staging_path)?;
                stage_secret_reader(
                    label,
                    file,
                    total_bytes,
                    secret_index,
                    secret_count,
                    should_continue,
                    observe_progress,
                )?
            }
        };
        handles.push(ProtectedSecretHandle::new(native, lease.reference(), destination)?);
    }
    crate::canonical_secret_handles(handles)
}

fn open_staged_secret(path: &std::path::Path) -> Result<(std::fs::File, u64), MacosError> {
    let selected = std::fs::symlink_metadata(path)
        .map_err(|source| error::io_error(MacosOperation::Prepare, &source))?;
    if !selected.is_file() || selected.file_type().is_symlink() {
        return Err(error::mismatch(
            MacosErrorKind::PreparationMismatch,
            "private staged secret path changed kind before protected transfer",
        ));
    }
    let file = std::fs::File::open(path)
        .map_err(|source| error::io_error(MacosOperation::Prepare, &source))?;
    let opened = file
        .metadata()
        .map_err(|source| error::io_error(MacosOperation::Prepare, &source))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;

        if selected.dev() != opened.dev()
            || selected.ino() != opened.ino()
            || selected.nlink() != 1
            || opened.nlink() != 1
            || selected.mode() & 0o077 != 0
        {
            return Err(error::mismatch(
                MacosErrorKind::PreparationMismatch,
                "private staged secret identity changed before protected transfer",
            ));
        }
    }
    Ok((file, opened.len()))
}

fn stage_secret_reader<C, P>(
    label: String,
    reader: impl Read,
    total_bytes: u64,
    secret_index: u32,
    secret_count: u32,
    should_continue: &C,
    observe_progress: &P,
) -> Result<NativeProtectedHandle, MacosError>
where
    C: Fn() -> bool + ?Sized,
    P: Fn(PreparationProgress) + ?Sized,
{
    if total_bytes == 0 || u32::try_from(total_bytes).is_err() {
        return Err(error::limited(
            MacosOperation::Prepare,
            "prepared secret length is outside helper manifest representation",
        ));
    }
    observe_progress(PreparationProgress::SecretStaging {
        secret_index,
        secret_count,
        bytes_staged: 0,
        total_bytes,
    });
    let handle = NativeProtectedHandle::from_reader(
        label,
        reader,
        should_continue,
        |bytes_staged| {
            observe_progress(PreparationProgress::SecretStaging {
                secret_index,
                secret_count,
                bytes_staged: u64::try_from(bytes_staged).unwrap_or(u64::MAX),
                total_bytes,
            });
        },
    )
    .map_err(protected_handle_error)?;
    if u64::try_from(handle.payload_len().unwrap_or(0)).unwrap_or(u64::MAX) != total_bytes {
        return Err(error::mismatch(
            MacosErrorKind::PreparationMismatch,
            "prepared secret artifact changed while it was being streamed",
        ));
    }
    Ok(handle)
}

fn validate_artifact_destination(
    artifact: &DeliveryArtifact,
    expected: &SecretDelivery,
) -> Result<(), MacosError> {
    let exact = match (artifact, expected) {
        (DeliveryArtifact::Environment { name, .. }, SecretDelivery::Environment(expected)) => {
            name == expected
        }
        (DeliveryArtifact::File { sandbox_path, .. }, SecretDelivery::File(expected)) => {
            sandbox_path == expected
        }
        (DeliveryArtifact::Brokered { label, .. }, SecretDelivery::BrokeredHandle(expected)) => {
            label == expected
        }
        _ => false,
    };
    if exact {
        Ok(())
    } else {
        Err(secret_binding_error(
            "prepared secret artifact destination differs from the checked requirement",
        ))
    }
}

pub(super) fn proxy_identity_digest(route: ProxyRoute) -> Sha256Digest {
    let mut bytes = b"peritus.macos.proxy-route.v1\0".to_vec();
    bytes.extend_from_slice(route.endpoint().to_string().as_bytes());
    bytes.extend_from_slice(&route.routing_handle().to_be_bytes());
    peritus_codec::sha256(&bytes)
}

pub(super) fn validate_secret_file_destinations(
    secrets: &[ProtectedSecretHandle],
) -> Result<(), MacosError> {
    const RESERVED: [&str; 4] = [
        "PERITUS_NATIVE_PTY_SLAVE_V1",
        "PERITUS_NATIVE_PROXY_ENDPOINT_V1",
        "PERITUS_NATIVE_PROXY_TOKEN_HANDLE_V1",
        "PERITUS_NATIVE_SECRET_HANDLES_V1",
    ];
    for secret in secrets {
        match secret.destination() {
            SecretHandleDestination::Environment(name) if RESERVED.contains(&name.as_str()) => {
                return Err(error::invalid(
                    MacosOperation::Prepare,
                    "secret environment destination collides with native protocol state",
                ));
            }
            SecretHandleDestination::File(path) => validate_file_destination(path.as_str())?,
            SecretHandleDestination::Environment(_) | SecretHandleDestination::Brokered(_) => {}
        }
    }
    Ok(())
}

pub(super) fn preflight_secret_destinations(
    requirements: &[SecretRequirement],
) -> Result<(), MacosError> {
    const RESERVED: [&str; 4] = [
        "PERITUS_NATIVE_PTY_SLAVE_V1",
        "PERITUS_NATIVE_PROXY_ENDPOINT_V1",
        "PERITUS_NATIVE_PROXY_TOKEN_HANDLE_V1",
        "PERITUS_NATIVE_SECRET_HANDLES_V1",
    ];
    for requirement in requirements {
        match requirement.delivery() {
            SecretDelivery::Environment(name) if RESERVED.contains(&name.as_str()) => {
                return Err(error::invalid(
                    MacosOperation::Prepare,
                    "secret environment destination collides with native protocol state",
                ));
            }
            SecretDelivery::File(path) => validate_file_destination(path.as_str())?,
            SecretDelivery::Environment(_) | SecretDelivery::BrokeredHandle(_) => {}
        }
    }
    Ok(())
}

fn validate_file_destination(value: &str) -> Result<(), MacosError> {
    let destination = std::path::Path::new(value);
    let parent = destination.parent().ok_or_else(|| {
        error::invalid(MacosOperation::Prepare, "secret file has no parent directory")
    })?;
    let canonical_parent = std::fs::canonicalize(parent)
        .map_err(|source| error::io_error(MacosOperation::Prepare, &source))?;
    if canonical_parent != parent {
        return Err(error::mismatch(
            MacosErrorKind::PreparationMismatch,
            "secret file parent changed or contains an alias",
        ));
    }
    match std::fs::symlink_metadata(destination) {
        Ok(_) => Err(error::mismatch(
            MacosErrorKind::PreparationMismatch,
            "secret file destination already exists",
        )),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(error::io_error(MacosOperation::Prepare, &source)),
    }
}

pub(super) fn helper_digest<C, P>(
    path: &std::path::Path,
    should_continue: &C,
    observe_progress: &P,
) -> Result<Sha256Digest, MacosError>
where
    C: Fn() -> bool + ?Sized,
    P: Fn(PreparationProgress) + ?Sized,
{
    let mut file = std::fs::File::open(path)
        .map_err(|source| error::io_error(MacosOperation::Prepare, &source))?;
    let total_bytes = file
        .metadata()
        .map_err(|source| error::io_error(MacosOperation::Prepare, &source))?
        .len();
    observe_progress(PreparationProgress::HelperIntegrity { bytes_hashed: 0, total_bytes });
    let mut digest = Sha256::new();
    let mut bytes_hashed = 0_u64;
    let mut buffer = [0_u8; 64 * 1_024];
    loop {
        if !should_continue() {
            return Err(MacosError::new(
                MacosErrorKind::SupervisorFailure,
                MacosOperation::Prepare,
                RecoveryAction::CancelAndReap,
                "native preparation was cancelled by its caller",
            ));
        }
        let count = file
            .read(&mut buffer)
            .map_err(|source| error::io_error(MacosOperation::Prepare, &source))?;
        if count == 0 {
            if bytes_hashed == 0 {
                return Err(error::limited(
                    MacosOperation::Prepare,
                    "installed helper is empty",
                ));
            }
            if bytes_hashed != total_bytes {
                return Err(error::mismatch(
                    MacosErrorKind::PreparationMismatch,
                    "installed helper length changed during integrity verification",
                ));
            }
            return Ok(Sha256Digest::new(digest.finalize().into()));
        }
        bytes_hashed = bytes_hashed
            .checked_add(u64::try_from(count).unwrap_or(u64::MAX))
            .ok_or_else(|| {
                error::limited(MacosOperation::Prepare, "installed helper length overflowed")
            })?;
        digest.update(&buffer[..count]);
        observe_progress(PreparationProgress::HelperIntegrity { bytes_hashed, total_bytes });
    }
}

pub(super) fn canonical_protected_roots(paths: &[PathBuf]) -> Result<Vec<PathBuf>, MacosError> {
    let mut resolved = Vec::with_capacity(paths.len());
    for path in paths {
        let metadata = std::fs::symlink_metadata(path)
            .map_err(|source| error::io_error(MacosOperation::Prepare, &source))?;
        if metadata.file_type().is_symlink() {
            return Err(error::mismatch(
                MacosErrorKind::PreparationMismatch,
                "protected metadata root became a symbolic link",
            ));
        }
        let canonical = std::fs::canonicalize(path)
            .map_err(|source| error::io_error(MacosOperation::Prepare, &source))?;
        if &canonical != path {
            return Err(error::mismatch(
                MacosErrorKind::PreparationMismatch,
                "protected metadata root changed after configuration",
            ));
        }
        resolved.push(canonical);
    }
    resolved.sort();
    resolved.dedup();
    Ok(resolved)
}

pub(super) fn validate_default_metadata_aliases(
    workspace: &std::path::Path,
) -> Result<(), MacosError> {
    for name in [".git", ".peritus"] {
        match std::fs::symlink_metadata(workspace.join(name)) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(error::mismatch(
                    MacosErrorKind::PreparationMismatch,
                    "protected workspace metadata became a symbolic link",
                ));
            }
            Ok(_) => {}
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => return Err(error::io_error(MacosOperation::Prepare, &source)),
        }
    }
    Ok(())
}

pub(super) fn validate_unchanged_executable(
    path: &std::path::Path,
    label: &str,
) -> Result<(), MacosError> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|source| error::io_error(MacosOperation::Prepare, &source))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(MacosError::new(
            MacosErrorKind::UnsupportedHost,
            MacosOperation::Prepare,
            RecoveryAction::RepairHelper,
            format!("checked {label} executable is unavailable or aliased"),
        ));
    }
    let canonical = std::fs::canonicalize(path)
        .map_err(|source| error::io_error(MacosOperation::Prepare, &source))?;
    if canonical != path {
        return Err(error::mismatch(
            MacosErrorKind::PreparationMismatch,
            format!("checked {label} executable path changed after probe"),
        ));
    }
    Ok(())
}

pub(super) fn proxy_prepare_error(source: NetworkError) -> MacosError {
    let (kind, recovery) = if source.kind() == NetworkErrorKind::IncompleteTeardown {
        (MacosErrorKind::CleanupIncomplete, RecoveryAction::RetryCleanup)
    } else {
        (MacosErrorKind::SupervisorFailure, RecoveryAction::Reauthorize)
    };
    let error = MacosError::new(
        kind,
        MacosOperation::Prepare,
        recovery,
        format!("managed proxy preparation failed ({})", source.kind().code()),
    )
    .with_source(error::network_source(&source));
    if source.kind() == NetworkErrorKind::IncompleteTeardown {
        error.with_cleanup(PreparationCleanup::new(false, true, false))
    } else {
        error
    }
}

pub(super) fn secret_prepare_error(source: SecretError) -> MacosError {
    let (kind, recovery) = match source.kind() {
        SecretErrorKind::Cleanup => {
            (MacosErrorKind::CleanupIncomplete, RecoveryAction::RetryCleanup)
        }
        SecretErrorKind::Cancelled => {
            (MacosErrorKind::SupervisorFailure, RecoveryAction::CancelAndReap)
        }
        _ => (MacosErrorKind::HelperFailure, RecoveryAction::Reauthorize),
    };
    let error = MacosError::new(
        kind,
        MacosOperation::Prepare,
        recovery,
        format!("exact secret delivery preparation failed ({})", source.kind().code()),
    )
    .with_source(error::secret_source(&source));
    if source.kind() == SecretErrorKind::Cleanup {
        error.with_cleanup(PreparationCleanup::new(false, false, true))
    } else {
        error
    }
}

fn protected_handle_error(source: ProcessError) -> MacosError {
    let kind = if source.code() == peritus_process::ErrorCode::Supervisor {
        MacosErrorKind::SupervisorFailure
    } else {
        MacosErrorKind::HelperFailure
    };
    MacosError::new(
        kind,
        MacosOperation::Prepare,
        RecoveryAction::CancelAndReap,
        format!("protected anonymous handle staging failed ({})", source.code().as_str()),
    )
    .with_source(error::process_source(&source))
}

fn secret_binding_error(detail: &'static str) -> MacosError {
    MacosError::new(
        MacosErrorKind::PreparationMismatch,
        MacosOperation::Prepare,
        RecoveryAction::Reauthorize,
        detail,
    )
}
