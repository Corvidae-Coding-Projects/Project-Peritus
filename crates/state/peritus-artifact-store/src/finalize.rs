//! Narrow file hashing, verification, and atomic no-replace publication.

use std::{
    fs,
    fs::File,
    io::{Read, Write},
    path::Path,
};

use sha2::{Digest, Sha256};

use crate::{
    ArtifactDigest, ArtifactStoreError, ErrorCode, Publication, RecoveryClass, StoreOperation,
    path::{io, sync_directory},
};

pub fn synchronize_temporary(file: &mut File) -> Result<(), ArtifactStoreError> {
    file.flush().map_err(|error| io(StoreOperation::FlushTemporary, error))?;
    file.sync_all().map_err(|error| io(StoreOperation::Synchronize, error))
}

pub fn publish(
    temporary: &Path,
    destination: &Path,
    destination_parent: &Path,
    temporary_parent: &Path,
    digest: ArtifactDigest,
    size: u64,
) -> Result<Publication, ArtifactStoreError> {
    let publication = match fs::hard_link(temporary, destination) {
        Ok(()) => Publication::New,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            verify_exact(destination, digest, size, ExistingFailure::Corruption)?;
            Publication::Existing
        }
        Err(error) => return Err(io(StoreOperation::Publish, error)),
    };
    sync_directory(destination_parent)?;
    fs::remove_file(temporary).map_err(|error| io(StoreOperation::Remove, error))?;
    sync_directory(temporary_parent)?;
    Ok(publication)
}

pub fn verify_finalized(
    path: &Path,
    digest: ArtifactDigest,
    size: u64,
) -> Result<(), ArtifactStoreError> {
    verify_exact(path, digest, size, ExistingFailure::Corruption)
}

pub fn inspect_file(
    path: &Path,
    expected_digest: ArtifactDigest,
) -> Result<u64, ArtifactStoreError> {
    let (actual_digest, size) = hash_file(path)?;
    if actual_digest != expected_digest {
        return Err(corrupt("stored object digest disagrees with its derived path"));
    }
    Ok(size)
}

fn verify_exact(
    path: &Path,
    expected_digest: ArtifactDigest,
    expected_size: u64,
    failure: ExistingFailure,
) -> Result<(), ArtifactStoreError> {
    let (actual_digest, actual_size) = hash_file(path)?;
    if actual_size != expected_size || actual_digest != expected_digest {
        return Err(match failure {
            ExistingFailure::Corruption => {
                corrupt("existing destination content does not match its digest and size")
            }
        });
    }
    Ok(())
}

fn hash_file(path: &Path) -> Result<(ArtifactDigest, u64), ArtifactStoreError> {
    let mut file = File::open(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            ArtifactStoreError::message(
                ErrorCode::MissingArtifact,
                RecoveryClass::RecoverStore,
                "artifact file is missing",
            )
        } else {
            io(StoreOperation::InspectObject, error)
        }
    })?;
    let mut hasher = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read =
            file.read(&mut buffer).map_err(|error| io(StoreOperation::InspectObject, error))?;
        if read == 0 {
            break;
        }
        size =
            size.checked_add(u64::try_from(read).map_err(|_| overflow())?).ok_or_else(overflow)?;
        hasher.update(&buffer[..read]);
    }
    Ok((ArtifactDigest::new(hasher.finalize().into()), size))
}

#[derive(Clone, Copy)]
enum ExistingFailure {
    Corruption,
}

const fn corrupt(message: &'static str) -> ArtifactStoreError {
    ArtifactStoreError::message(ErrorCode::CorruptObject, RecoveryClass::TerminalIntegrity, message)
}

const fn overflow() -> ArtifactStoreError {
    ArtifactStoreError::message(
        ErrorCode::ArithmeticOverflow,
        RecoveryClass::TerminalIntegrity,
        "artifact byte count overflowed",
    )
}
