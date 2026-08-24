//! Protected resource staging and installed-path revalidation.

use std::{io::Read, path::PathBuf};

use peritus_network::ManagedProxy;
use peritus_process::NativeProtectedHandle;
use peritus_secrets::{DeliveryArtifact, SecretDeliverySession};
use peritus_types::Sha256Digest;

use crate::{
    MacosError, MacosErrorKind, MacosOperation, ProtectedProxyRoute, ProtectedSecretHandle,
    ProxyRoute, RecoveryAction, SecretHandleDestination, error,
};

const MAX_HELPER_BYTES: u64 = 16 * 1_024 * 1_024;

pub(super) fn stage_proxy_handle(proxy: &ManagedProxy) -> Result<ProtectedProxyRoute, MacosError> {
    let handle = proxy.routing_token().expose_bytes(|bytes| {
        NativeProtectedHandle::from_bytes("peritus-macos-proxy-routing-v1", bytes.to_vec())
    });
    ProtectedProxyRoute::new(
        proxy.endpoint().socket_addr(),
        handle.map_err(|_| protected_handle_error())?,
    )
}

pub(super) fn stage_secret_handles(
    session: &SecretDeliverySession,
) -> Result<Vec<ProtectedSecretHandle>, MacosError> {
    if session.artifacts().len() != session.leases().len() {
        return Err(secret_prepare_error());
    }
    let mut handles = Vec::with_capacity(session.artifacts().len());
    for (index, (artifact, lease)) in session.artifacts().iter().zip(session.leases()).enumerate() {
        let (payload, destination) = match artifact {
            DeliveryArtifact::Environment { name, material } => (
                material.expose(<[u8]>::to_vec),
                SecretHandleDestination::Environment(name.clone()),
            ),
            DeliveryArtifact::File { sandbox_path, staging_path } => (
                read_bounded_secret_file(staging_path)?,
                SecretHandleDestination::File(sandbox_path.clone()),
            ),
            DeliveryArtifact::Brokered { label, material } => {
                (material.expose(<[u8]>::to_vec), SecretHandleDestination::Brokered(label.clone()))
            }
        };
        let native =
            NativeProtectedHandle::from_bytes(format!("peritus-macos-secret-v1-{index}"), payload)
                .map_err(|_| protected_handle_error())?;
        handles.push(ProtectedSecretHandle::new(native, lease.reference(), destination)?);
    }
    crate::canonical_secret_handles(handles)
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

pub(super) fn read_bounded_helper(path: &std::path::Path) -> Result<Vec<u8>, MacosError> {
    read_bounded(path, MAX_HELPER_BYTES, "installed helper is empty or exceeds its byte bound")
}

fn read_bounded_secret_file(path: &std::path::Path) -> Result<Vec<u8>, MacosError> {
    read_bounded(path, 1_024 * 1_024, "prepared secret is empty or exceeds its byte bound")
}

fn read_bounded(
    path: &std::path::Path,
    maximum: u64,
    detail: &'static str,
) -> Result<Vec<u8>, MacosError> {
    let file = std::fs::File::open(path)
        .map_err(|source| error::io_error(MacosOperation::Prepare, &source))?;
    let mut bytes = Vec::new();
    file.take(maximum.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|source| error::io_error(MacosOperation::Prepare, &source))?;
    if bytes.is_empty() || u64::try_from(bytes.len()).unwrap_or(u64::MAX) > maximum {
        return Err(error::limited(MacosOperation::Prepare, detail));
    }
    Ok(bytes)
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

pub(super) fn proxy_prepare_error() -> MacosError {
    MacosError::new(
        MacosErrorKind::SupervisorFailure,
        MacosOperation::Prepare,
        RecoveryAction::RetryCleanup,
        "managed proxy preparation failed",
    )
}

pub(super) fn secret_prepare_error() -> MacosError {
    MacosError::new(
        MacosErrorKind::HelperFailure,
        MacosOperation::Prepare,
        RecoveryAction::Reauthorize,
        "exact secret delivery preparation failed",
    )
}

fn protected_handle_error() -> MacosError {
    MacosError::new(
        MacosErrorKind::HelperFailure,
        MacosOperation::Prepare,
        RecoveryAction::Reauthorize,
        "protected anonymous handle staging failed",
    )
}
