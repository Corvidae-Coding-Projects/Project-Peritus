//! Real content-addressed artifact recovery on both sides of durable publication.

use std::fs;
use std::path::{Path, PathBuf};

use peritus_artifact_store::{
    ArtifactDigest, ArtifactStore, ArtifactStoreError, ArtifactWriteHandle, EncryptionMetadata,
    MediaType, ReferenceOwner, StoreConfig, WriteRequest,
};
use peritus_codec::sha256;
use peritus_types::{EventId, Sha256Digest};

use crate::{DaemonConfig, DaemonError, DaemonErrorCode, DaemonRecovery};

use super::{acquire_instance, journal_error, open_journal};

const PAYLOAD: &[u8] = b"peritus/h1/blob-commit-qualification/v1\n";

/// Checkpoint that keeps a fully written temporary object alive until the process is killed.
pub struct BlobBeforeCheckpoint {
    digest: String,
    bytes: u64,
    temporary_files: u64,
    _writer: ArtifactWriteHandle,
}

impl BlobBeforeCheckpoint {
    pub(crate) fn digest(&self) -> &str {
        &self.digest
    }

    pub(crate) const fn bytes(&self) -> u64 {
        self.bytes
    }

    pub(crate) const fn temporary_files(&self) -> u64 {
        self.temporary_files
    }
}

/// Durable checkpoint after exact bytes, metadata, and their owner reference are published.
pub struct BlobAfterCheckpoint {
    digest: String,
    bytes: u64,
}

impl BlobAfterCheckpoint {
    pub(crate) fn digest(&self) -> &str {
        &self.digest
    }

    pub(crate) const fn bytes(&self) -> u64 {
        self.bytes
    }
}

/// Direct artifact-store facts after restart.
pub struct BlobQualification {
    digest: String,
    bytes: u64,
    journal_verified: bool,
    finalized: bool,
    referenced: bool,
    temporary_files: u64,
    object_files: u64,
}

impl BlobQualification {
    pub(crate) fn digest(&self) -> &str {
        &self.digest
    }

    pub(crate) const fn bytes(&self) -> u64 {
        self.bytes
    }

    pub(crate) const fn journal_verified(&self) -> bool {
        self.journal_verified
    }

    pub(crate) const fn finalized(&self) -> bool {
        self.finalized
    }

    pub(crate) const fn referenced(&self) -> bool {
        self.referenced
    }

    pub(crate) const fn temporary_files(&self) -> u64 {
        self.temporary_files
    }

    pub(crate) const fn object_files(&self) -> u64 {
        self.object_files
    }
}

/// Writes the exact bytes through the production owned writer without finalizing them.
pub fn stage_blob_before_crash(config: &DaemonConfig) -> Result<BlobBeforeCheckpoint, DaemonError> {
    let (store, digest, request) = open_store(config)?;
    let mut writer = store.begin_owned_write(request).map_err(store_error)?;
    writer.write_chunk(PAYLOAD).map_err(store_error)?;
    let temporary_files = count_files(&config.paths().artifact_root().join("temporary"))?;
    if writer.bytes_written() != PAYLOAD.len() as u64 || temporary_files != 1 {
        return Err(blob_error("artifact writer did not retain the exact qualification bytes"));
    }
    Ok(BlobBeforeCheckpoint {
        digest: digest.to_hex(),
        bytes: PAYLOAD.len() as u64,
        temporary_files,
        _writer: writer,
    })
}

/// Reopens the production store and proves its recovery removed the abandoned temporary object.
pub fn recover_blob_before_crash(config: &DaemonConfig) -> Result<BlobQualification, DaemonError> {
    let (mut store, digest, _) = open_store(config)?;
    let journal_verified = verify_empty_journal(config)?;
    let finalized = store.metadata(digest).map_err(store_error)?.is_some();
    let referenced = store.reference_roots().map_err(store_error)?.contains(&digest);
    let temporary_files = count_files(&config.paths().artifact_root().join("temporary"))?;
    let object_files = count_files_recursive(&config.paths().artifact_root().join("objects"))?;
    let recovery = store.recover().map_err(store_error)?;
    if finalized
        || referenced
        || temporary_files != 0
        || object_files != 0
        || recovery.removed_temporary_files() != 0
    {
        return Err(blob_error("pre-commit artifact state survived restart recovery"));
    }
    Ok(BlobQualification {
        digest: digest.to_hex(),
        bytes: PAYLOAD.len() as u64,
        journal_verified,
        finalized,
        referenced,
        temporary_files,
        object_files,
    })
}

/// Publishes exact bytes, durable metadata, and an owner reference before the process is killed.
pub fn stage_blob_after_crash(config: &DaemonConfig) -> Result<BlobAfterCheckpoint, DaemonError> {
    let (store, digest, request) = open_store(config)?;
    let mut writer = store.begin_owned_write(request).map_err(store_error)?;
    writer.write_chunk(PAYLOAD).map_err(store_error)?;
    let finalized = store.complete_write(writer).map_err(store_error)?;
    store.add_reference(owner(), digest).map_err(store_error)?;
    let verified = store.verify(digest).map_err(store_error)?;
    let referenced = store.reference_roots().map_err(store_error)?.contains(&digest);
    if finalized.digest() != digest
        || finalized.size() != PAYLOAD.len() as u64
        || verified.size() != finalized.size()
        || !referenced
    {
        return Err(blob_error("finalized artifact identity differs from the staged bytes"));
    }
    Ok(BlobAfterCheckpoint { digest: digest.to_hex(), bytes: finalized.size() })
}

/// Reopens the production store and verifies the exact finalized bytes and durable owner root.
pub fn recover_blob_after_crash(config: &DaemonConfig) -> Result<BlobQualification, DaemonError> {
    let (store, digest, _) = open_store(config)?;
    let journal_verified = verify_empty_journal(config)?;
    let metadata = store.verify(digest).map_err(store_error)?;
    let bytes = store.read(digest, PAYLOAD.len() as u64).map_err(store_error)?;
    let referenced = store.reference_roots().map_err(store_error)?.contains(&digest);
    let temporary_files = count_files(&config.paths().artifact_root().join("temporary"))?;
    let object_files = count_files_recursive(&config.paths().artifact_root().join("objects"))?;
    if metadata.size() != PAYLOAD.len() as u64
        || bytes != PAYLOAD
        || !referenced
        || temporary_files != 0
        || object_files != 1
    {
        return Err(blob_error("committed artifact was not recovered with its exact owner root"));
    }
    Ok(BlobQualification {
        digest: digest.to_hex(),
        bytes: metadata.size(),
        journal_verified,
        finalized: true,
        referenced,
        temporary_files,
        object_files,
    })
}

fn open_store(
    config: &DaemonConfig,
) -> Result<(ArtifactStore, ArtifactDigest, WriteRequest), DaemonError> {
    let store_id = config.store_identity()?;
    let _instance = acquire_instance(config, store_id)?;
    let mut journal = open_journal(config, store_id)?;
    journal.integrity_scan().map_err(journal_error)?;
    let digest = ArtifactDigest::from_sha256(sha256(PAYLOAD));
    let request = WriteRequest::new(
        digest,
        PAYLOAD.len() as u64,
        PAYLOAD.len() as u64,
        MediaType::new("application/octet-stream").map_err(store_error)?,
        EncryptionMetadata::unencrypted(),
        event_id(digest.sha256())?,
    );
    let store_config = StoreConfig::new(
        config.paths().artifact_root(),
        config.limits().maximum_artifact_bytes(),
        config.limits().artifact_quota_bytes(),
    )
    .and_then(|value| value.with_database_path(config.paths().database()))
    .map_err(store_error)?;
    let store = ArtifactStore::open(store_config).map_err(store_error)?;
    Ok((store, digest, request))
}

fn verify_empty_journal(config: &DaemonConfig) -> Result<bool, DaemonError> {
    let store_id = config.store_identity()?;
    let _instance = acquire_instance(config, store_id)?;
    let mut journal = open_journal(config, store_id)?;
    let report = journal.integrity_scan().map_err(journal_error)?;
    if report.event_count() != 0 || report.aggregate_count() != 0 || report.last_position() != 0 {
        return Err(blob_error("artifact qualification changed the authoritative journal"));
    }
    Ok(true)
}

fn owner() -> ReferenceOwner {
    ReferenceOwner::evidence(sha256(b"peritus/h1/blob-reference-owner/v1\0"))
}

fn event_id(digest: Sha256Digest) -> Result<EventId, DaemonError> {
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest.as_bytes()[..16]);
    EventId::new(bytes).map_err(|_| blob_error("artifact qualification event identity is invalid"))
}

fn count_files(path: &Path) -> Result<u64, DaemonError> {
    let entries = fs::read_dir(path).map_err(filesystem_error)?;
    let mut count = 0_u64;
    for entry in entries {
        let entry = entry.map_err(filesystem_error)?;
        if entry.file_type().map_err(filesystem_error)?.is_file() {
            count =
                count.checked_add(1).ok_or_else(|| blob_error("artifact file count overflow"))?;
        }
    }
    Ok(count)
}

fn count_files_recursive(path: &Path) -> Result<u64, DaemonError> {
    let mut pending = vec![PathBuf::from(path)];
    let mut count = 0_u64;
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).map_err(filesystem_error)? {
            let entry = entry.map_err(filesystem_error)?;
            let kind = entry.file_type().map_err(filesystem_error)?;
            if kind.is_dir() {
                pending.push(entry.path());
            } else if kind.is_file() {
                count = count
                    .checked_add(1)
                    .ok_or_else(|| blob_error("artifact file count overflow"))?;
            }
        }
    }
    Ok(count)
}

fn store_error(error: ArtifactStoreError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Storage,
        DaemonRecovery::Reconcile,
        "qualify artifact commit recovery",
        error.to_string(),
        error,
    )
}

fn filesystem_error(error: std::io::Error) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Storage,
        DaemonRecovery::Reconcile,
        "inspect artifact qualification layout",
        error.to_string(),
        error,
    )
}

fn blob_error(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::Operator,
        "qualify artifact commit recovery",
        detail,
    )
}
