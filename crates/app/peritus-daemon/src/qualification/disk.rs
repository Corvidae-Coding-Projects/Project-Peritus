//! Real artifact-finalization quota exhaustion and restart verification.

use std::fs;
use std::path::{Path, PathBuf};

use peritus_artifact_store::{
    ArtifactDigest, ArtifactStore, ArtifactStoreError, EncryptionMetadata, ErrorCode, MediaType,
    ReferenceOwner, StoreConfig, WriteRequest,
};
use peritus_codec::sha256;
use peritus_types::{EventId, Sha256Digest};

use crate::{DaemonConfig, DaemonError, DaemonErrorCode, DaemonRecovery};

use super::{acquire_instance, journal_error, open_journal, verify_empty_journal};

const FILLER: &[u8] = b"peritus/h1/blob-quota/filler/v1\n";
const REJECTED: &[u8] = b"peritus/h1/blob-quota/target/v1\n";
const QUOTA_BYTES: u64 = FILLER.len() as u64;

/// Direct facts captured after the real finalize-time quota rejection.
pub struct BlobQuotaCheckpoint {
    filler_digest: String,
    rejected_digest: String,
    quota_bytes: u64,
    temporary_files: u64,
    object_files: u64,
}

impl BlobQuotaCheckpoint {
    pub(crate) fn filler_digest(&self) -> &str {
        &self.filler_digest
    }

    pub(crate) fn rejected_digest(&self) -> &str {
        &self.rejected_digest
    }

    pub(crate) const fn quota_bytes(&self) -> u64 {
        self.quota_bytes
    }

    pub(crate) const fn temporary_files(&self) -> u64 {
        self.temporary_files
    }

    pub(crate) const fn object_files(&self) -> u64 {
        self.object_files
    }
}

/// Fresh-process facts proving that only the admitted object remains.
pub struct BlobQuotaQualification {
    filler_digest: String,
    rejected_digest: String,
    quota_bytes: u64,
    used_bytes: u64,
    journal_verified: bool,
    temporary_files: u64,
    object_files: u64,
}

impl BlobQuotaQualification {
    pub(crate) fn filler_digest(&self) -> &str {
        &self.filler_digest
    }

    pub(crate) fn rejected_digest(&self) -> &str {
        &self.rejected_digest
    }

    pub(crate) const fn quota_bytes(&self) -> u64 {
        self.quota_bytes
    }

    pub(crate) const fn used_bytes(&self) -> u64 {
        self.used_bytes
    }

    pub(crate) const fn journal_verified(&self) -> bool {
        self.journal_verified
    }

    pub(crate) const fn temporary_files(&self) -> u64 {
        self.temporary_files
    }

    pub(crate) const fn object_files(&self) -> u64 {
        self.object_files
    }
}

/// Opens two writers against the same available quota, admits one, and requires the second
/// finalize to lose the real durable quota race and roll its published bytes back.
pub fn stage_blob_finalize_exhaustion(
    config: &DaemonConfig,
) -> Result<BlobQuotaCheckpoint, DaemonError> {
    let store = open_quota_store(config)?;
    let filler_digest = digest(FILLER);
    let rejected_digest = digest(REJECTED);
    let mut filler =
        store.begin_owned_write(request(FILLER, filler_digest)?).map_err(store_error)?;
    let mut rejected =
        store.begin_owned_write(request(REJECTED, rejected_digest)?).map_err(store_error)?;
    filler.write_chunk(FILLER).map_err(store_error)?;
    rejected.write_chunk(REJECTED).map_err(store_error)?;
    store.complete_write(filler).map_err(store_error)?;
    store.add_reference(owner(), filler_digest).map_err(store_error)?;
    let error = match store.complete_write(rejected) {
        Ok(_) => return Err(disk_error("the second finalize unexpectedly passed its quota")),
        Err(error) => error,
    };
    if error.code() != ErrorCode::QuotaExceeded
        || store.metadata(rejected_digest).map_err(store_error)?.is_some()
        || store.verify(filler_digest).map_err(store_error)?.size() != QUOTA_BYTES
    {
        return Err(disk_error("artifact quota rejection did not roll back the target object"));
    }
    let temporary_files = count_files(&config.paths().artifact_root().join("temporary"))?;
    let object_files = count_files_recursive(&config.paths().artifact_root().join("objects"))?;
    if temporary_files != 0 || object_files != 1 {
        return Err(disk_error("artifact quota rejection left unexpected filesystem objects"));
    }
    Ok(BlobQuotaCheckpoint {
        filler_digest: filler_digest.to_hex(),
        rejected_digest: rejected_digest.to_hex(),
        quota_bytes: QUOTA_BYTES,
        temporary_files,
        object_files,
    })
}

/// Reopens the production store and verifies durable quota accounting and exact rollback.
pub fn recover_blob_finalize_exhaustion(
    config: &DaemonConfig,
) -> Result<BlobQuotaQualification, DaemonError> {
    let store = open_quota_store(config)?;
    let filler_digest = digest(FILLER);
    let rejected_digest = digest(REJECTED);
    let journal_verified = verify_empty_journal(config)?;
    let filler = store.verify(filler_digest).map_err(store_error)?;
    let filler_bytes = store.read(filler_digest, QUOTA_BYTES).map_err(store_error)?;
    let roots = store.reference_roots().map_err(store_error)?;
    let used_bytes = store.quota_snapshot(0).map_err(store_error)?.used_bytes();
    let temporary_files = count_files(&config.paths().artifact_root().join("temporary"))?;
    let object_files = count_files_recursive(&config.paths().artifact_root().join("objects"))?;
    if filler.size() != QUOTA_BYTES
        || filler_bytes != FILLER
        || !roots.contains(&filler_digest)
        || store.metadata(rejected_digest).map_err(store_error)?.is_some()
        || used_bytes != QUOTA_BYTES
        || temporary_files != 0
        || object_files != 1
    {
        return Err(disk_error("reopened artifact quota state differs from the exact rollback"));
    }
    Ok(BlobQuotaQualification {
        filler_digest: filler_digest.to_hex(),
        rejected_digest: rejected_digest.to_hex(),
        quota_bytes: QUOTA_BYTES,
        used_bytes,
        journal_verified,
        temporary_files,
        object_files,
    })
}

fn open_quota_store(config: &DaemonConfig) -> Result<ArtifactStore, DaemonError> {
    let store_id = config.store_identity()?;
    let _instance = acquire_instance(config, store_id)?;
    let mut journal = open_journal(config, store_id)?;
    journal.integrity_scan().map_err(journal_error)?;
    let store_config = StoreConfig::new(config.paths().artifact_root(), QUOTA_BYTES, QUOTA_BYTES)
        .and_then(|value| value.with_database_path(config.paths().database()))
        .map_err(store_error)?;
    ArtifactStore::open(store_config).map_err(store_error)
}

fn request(payload: &[u8], digest: ArtifactDigest) -> Result<WriteRequest, DaemonError> {
    let bytes = u64::try_from(payload.len()).map_err(|_| disk_error("payload size overflow"))?;
    Ok(WriteRequest::new(
        digest,
        bytes,
        bytes,
        MediaType::new("application/octet-stream").map_err(store_error)?,
        EncryptionMetadata::unencrypted(),
        event_id(digest.sha256())?,
    ))
}

fn digest(payload: &[u8]) -> ArtifactDigest {
    ArtifactDigest::from_sha256(sha256(payload))
}

fn owner() -> ReferenceOwner {
    ReferenceOwner::evidence(sha256(b"peritus/h1/blob-quota-owner/v1\0"))
}

fn event_id(digest: Sha256Digest) -> Result<EventId, DaemonError> {
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest.as_bytes()[..16]);
    EventId::new(bytes).map_err(|_| disk_error("artifact quota event identity is invalid"))
}

fn count_files(path: &Path) -> Result<u64, DaemonError> {
    let mut count = 0_u64;
    for entry in fs::read_dir(path).map_err(filesystem_error)? {
        if entry.map_err(filesystem_error)?.file_type().map_err(filesystem_error)?.is_file() {
            count = count.checked_add(1).ok_or_else(|| disk_error("file count overflow"))?;
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
                count = count.checked_add(1).ok_or_else(|| disk_error("file count overflow"))?;
            }
        }
    }
    Ok(count)
}

fn store_error(error: ArtifactStoreError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Storage,
        DaemonRecovery::Reconcile,
        "qualify artifact quota recovery",
        error.to_string(),
        error,
    )
}

fn filesystem_error(error: std::io::Error) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Storage,
        DaemonRecovery::Reconcile,
        "inspect artifact quota layout",
        error.to_string(),
        error,
    )
}

fn disk_error(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::Operator,
        "qualify artifact quota recovery",
        detail,
    )
}
