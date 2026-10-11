//! Exact secret-artifact validation and protected native-handle transfer.
use super::{channel_error, ensure_continues, io_channel_error, protected_handle_error};
use crate::{
    ProtectedSecretHandle, SecretHandleDestination, WindowsError, WindowsErrorKind,
    secret_reference_digest,
};
use peritus_process::NativeProtectedHandle;
use peritus_sandbox::{SecretDelivery, SecretRequirement};
use peritus_secrets::{DeliveryArtifact, SecretDeliverySession};
use std::{fs::File, io::Cursor};

const MAX_SECRET_BYTES: u64 = 1_048_576;

pub(super) fn stage_secret_handles(
    session: &SecretDeliverySession,
    requirements: &[SecretRequirement],
    should_continue: &dyn Fn() -> bool,
) -> Result<(Vec<ProtectedSecretHandle>, Vec<NativeProtectedHandle>), WindowsError> {
    if session.artifacts().len() != requirements.len()
        || session.leases().len() != requirements.len()
    {
        return Err(channel_error(
            WindowsErrorKind::Secret,
            "prepared secret artifacts, leases, and requirements differ",
        ));
    }
    let mut descriptors = Vec::with_capacity(requirements.len());
    let mut handles = Vec::with_capacity(requirements.len());
    for (index, ((requirement, artifact), lease)) in
        requirements.iter().zip(session.artifacts()).zip(session.leases()).enumerate()
    {
        if lease.reference() != requirement.reference()
            || lease.delivery() != requirement.delivery()
        {
            return Err(channel_error(
                WindowsErrorKind::Secret,
                "prepared secret lease differs from checked delivery",
            ));
        }
        validate_artifact(requirement.delivery(), artifact)?;
        let protected =
            stage_artifact(format!("windows-secret-{index}"), artifact, should_continue)?;
        descriptors.push(ProtectedSecretHandle::new(
            protected.raw_handle(),
            secret_reference_digest(requirement.reference()),
            SecretHandleDestination::from(requirement.delivery()),
        )?);
        handles.push(protected);
    }
    Ok((crate::canonical_handles(descriptors)?, handles))
}

fn stage_artifact(
    label: String,
    artifact: &DeliveryArtifact,
    should_continue: &dyn Fn() -> bool,
) -> Result<NativeProtectedHandle, WindowsError> {
    if let Some(handle) = artifact.expose_environment(|_, bytes| {
        stage_reader(label.clone(), Cursor::new(bytes), bytes.len(), should_continue)
    }) {
        return handle;
    }
    if let Some(handle) = artifact.expose_brokered(|_, bytes| {
        stage_reader(label.clone(), Cursor::new(bytes), bytes.len(), should_continue)
    }) {
        return handle;
    }
    let (path, _) = artifact.file_paths().ok_or_else(|| {
        channel_error(WindowsErrorKind::Secret, "secret artifact has no representable destination")
    })?;
    let (mut file, opened) = open_staged_secret(path)?;
    let total = usize::try_from(opened.len()).map_err(|_| {
        channel_error(WindowsErrorKind::Secret, "staged secret length is not representable")
    })?;
    let handle = stage_reader(label, &mut file, total, should_continue)?;
    let after = file.metadata().map_err(|source| {
        io_channel_error("staged secret identity cannot be rechecked", &source)
    })?;
    if !same_file_identity(&opened, &after) || opened.len() != after.len() {
        return Err(channel_error(
            WindowsErrorKind::PreparationMismatch,
            "staged secret identity changed during protected transfer",
        ));
    }
    Ok(handle)
}

#[cfg(not(target_os = "windows"))]
fn open_staged_secret(path: &std::path::Path) -> Result<(File, std::fs::Metadata), WindowsError> {
    let selected = std::fs::symlink_metadata(path)
        .map_err(|source| io_channel_error("staged secret path cannot be inspected", &source))?;
    if !selected.is_file() || selected.file_type().is_symlink() {
        return Err(channel_error(
            WindowsErrorKind::PreparationMismatch,
            "staged secret path changed kind before protected transfer",
        ));
    }
    let file = File::open(path)
        .map_err(|source| io_channel_error("staged secret file cannot be opened", &source))?;
    let opened = file
        .metadata()
        .map_err(|source| io_channel_error("opened staged secret cannot be inspected", &source))?;
    if !same_file_identity(&selected, &opened) {
        return Err(channel_error(
            WindowsErrorKind::PreparationMismatch,
            "staged secret identity changed before protected transfer",
        ));
    }
    Ok((file, opened))
}

fn stage_reader(
    label: String,
    reader: impl std::io::Read,
    total: usize,
    should_continue: &dyn Fn() -> bool,
) -> Result<NativeProtectedHandle, WindowsError> {
    let total = u64::try_from(total).unwrap_or(u64::MAX);
    if total == 0 || total > MAX_SECRET_BYTES {
        return Err(channel_error(
            WindowsErrorKind::Secret,
            "secret exceeds the protected delivery bound",
        ));
    }
    ensure_continues(should_continue)?;
    let mut bytes = Vec::new();
    let mut reader = reader.take(MAX_SECRET_BYTES + 1);
    std::io::Read::read_to_end(&mut reader, &mut bytes)
        .map_err(|source| io_channel_error("secret artifact cannot be read", &source))?;
    ensure_continues(should_continue)?;
    let handle = NativeProtectedHandle::from_bytes(label, bytes)
        .map_err(|source| protected_handle_error(&source))?;
    if u64::try_from(handle.payload_len().unwrap_or(0)).unwrap_or(u64::MAX) != total {
        return Err(channel_error(
            WindowsErrorKind::PreparationMismatch,
            "secret artifact changed while it was streamed",
        ));
    }
    Ok(handle)
}

#[cfg(unix)]
fn same_file_identity(left: &std::fs::Metadata, right: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt as _;

    left.dev() == right.dev()
        && left.ino() == right.ino()
        && left.nlink() == 1
        && right.nlink() == 1
}

#[cfg(windows)]
fn same_file_identity(left: &std::fs::Metadata, right: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;

    // Both observations are from the same open handle, held without write/delete sharing.
    left.file_attributes() == right.file_attributes()
        && left.creation_time() == right.creation_time()
        && left.last_write_time() == right.last_write_time()
        && left.file_size() == right.file_size()
}

#[cfg(not(any(unix, windows)))]
fn same_file_identity(left: &std::fs::Metadata, right: &std::fs::Metadata) -> bool {
    left.len() == right.len()
}

fn validate_artifact(
    expected: &SecretDelivery,
    artifact: &DeliveryArtifact,
) -> Result<(), WindowsError> {
    let exact = match expected {
        SecretDelivery::Environment(name) => {
            artifact.expose_environment(|actual, _| actual == name).unwrap_or(false)
        }
        SecretDelivery::File(path) => {
            artifact.file_paths().is_some_and(|(_, actual)| actual == path)
        }
        SecretDelivery::BrokeredHandle(label) => {
            artifact.expose_brokered(|actual, _| actual == label).unwrap_or(false)
        }
    };
    if exact {
        Ok(())
    } else {
        Err(channel_error(
            WindowsErrorKind::Secret,
            "prepared secret artifact differs from checked delivery",
        ))
    }
}

#[cfg(target_os = "windows")]
fn open_staged_secret(path: &std::path::Path) -> Result<(File, std::fs::Metadata), WindowsError> {
    use std::os::windows::fs::{MetadataExt as _, OpenOptionsExt as _};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
    };
    let file = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|source| {
            io_channel_error("staged secret file cannot be opened exclusively", &source)
        })?;
    let metadata = file
        .metadata()
        .map_err(|source| io_channel_error("staged secret cannot be inspected", &source))?;
    if !metadata.is_file() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(channel_error(
            WindowsErrorKind::Path,
            "staged secret is not an ordinary non-reparse file",
        ));
    }
    Ok((file, metadata))
}
