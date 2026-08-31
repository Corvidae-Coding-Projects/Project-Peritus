//! Controlled artifact corruption followed by durable startup containment.

use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::PathBuf;

use peritus_artifact_store::{
    ArtifactDigest, ArtifactStore, ErrorCode, IntegrityState, StoreConfig,
};
use peritus_codec::sha256;
use peritus_types::Sha256Digest;

use crate::outbox::stage_blob_after_crash;
use crate::{DaemonConfig, DaemonError, DaemonErrorCode, DaemonRecovery};

use super::verify_empty_journal;

/// Exact facts retained after changing a referenced artifact's finalized bytes.
pub struct BlobCorruptionCheckpoint {
    digest: ArtifactDigest,
    original_sha256: Sha256Digest,
    corrupt_sha256: Sha256Digest,
    bytes: u64,
}

impl BlobCorruptionCheckpoint {
    pub const fn digest(&self) -> ArtifactDigest {
        self.digest
    }
    pub const fn original_sha256(&self) -> Sha256Digest {
        self.original_sha256
    }
    pub const fn corrupt_sha256(&self) -> Sha256Digest {
        self.corrupt_sha256
    }
    pub const fn bytes(&self) -> u64 {
        self.bytes
    }
}

/// Fresh-process facts after the store quarantined divergent referenced bytes.
pub struct BlobCorruptionObservation {
    digest: ArtifactDigest,
    quarantined_sha256: Sha256Digest,
    bytes: u64,
}

impl BlobCorruptionObservation {
    pub const fn digest(&self) -> ArtifactDigest {
        self.digest
    }
    pub const fn quarantined_sha256(&self) -> Sha256Digest {
        self.quarantined_sha256
    }
    pub const fn bytes(&self) -> u64 {
        self.bytes
    }
    pub const fn journal_verified(&self) -> bool {
        true
    }
    pub const fn reference_retained(&self) -> bool {
        true
    }
    pub const fn corruption_detected(&self) -> bool {
        true
    }
    pub const fn mutation_admitted(&self) -> bool {
        false
    }
}

/// Publishes one real referenced object, then changes its synchronized object bytes in place.
pub fn stage_corruption(config: &DaemonConfig) -> Result<BlobCorruptionCheckpoint, DaemonError> {
    let checkpoint = stage_blob_after_crash(config)?;
    let digest = checkpoint.artifact_digest();
    let object = object_path(config, digest);
    let mut bytes = fs::read(&object).map_err(filesystem_error)?;
    if bytes.is_empty() || bytes.len() as u64 != checkpoint.bytes() {
        return Err(qualification_error("staged artifact bytes differ from durable metadata"));
    }
    let original_sha256 = sha256(&bytes);
    if ArtifactDigest::from_sha256(original_sha256) != digest {
        return Err(qualification_error("staged artifact was corrupt before fault injection"));
    }
    bytes[0] ^= 0xff;
    let mut file =
        OpenOptions::new().write(true).truncate(true).open(&object).map_err(filesystem_error)?;
    file.write_all(&bytes).map_err(filesystem_error)?;
    file.sync_all().map_err(filesystem_error)?;
    let corrupt_sha256 = sha256(&bytes);
    if corrupt_sha256 == original_sha256 {
        return Err(qualification_error("artifact corruption did not change content identity"));
    }
    Ok(BlobCorruptionCheckpoint {
        digest,
        original_sha256,
        corrupt_sha256,
        bytes: checkpoint.bytes(),
    })
}

/// Reopens the real store and proves corruption is retained for audit but cannot be referenced.
pub fn recover_corruption(config: &DaemonConfig) -> Result<BlobCorruptionObservation, DaemonError> {
    let journal_verified = verify_empty_journal(config)?;
    let digest = qualification_digest();
    let store = ArtifactStore::open(store_config(config)?).map_err(store_error)?;
    let metadata = store
        .metadata(digest)
        .map_err(store_error)?
        .ok_or_else(|| qualification_error("contained artifact metadata is absent"))?;
    let reference_retained = store.reference_roots().map_err(store_error)?.contains(&digest);
    let unavailable =
        store.verify(digest).is_err_and(|error| error.code() == ErrorCode::MissingArtifact);
    let object = object_path(config, digest);
    let quarantine = quarantine_path(config, digest);
    let quarantined = fs::read(&quarantine).map_err(filesystem_error)?;
    let quarantined_sha256 = sha256(&quarantined);
    let corruption_detected = metadata.integrity() == IntegrityState::Corrupt
        && !metadata.is_referenceable()
        && unavailable
        && !object.exists()
        && quarantine.is_file();
    if !journal_verified
        || !reference_retained
        || !corruption_detected
        || metadata.size() != quarantined.len() as u64
        || ArtifactDigest::from_sha256(quarantined_sha256) == digest
    {
        return Err(qualification_error("artifact corruption was not durably contained"));
    }
    Ok(BlobCorruptionObservation { digest, quarantined_sha256, bytes: metadata.size() })
}

fn qualification_digest() -> ArtifactDigest {
    ArtifactDigest::from_sha256(sha256(b"peritus/h1/blob-commit-qualification/v1\n"))
}

fn object_path(config: &DaemonConfig, digest: ArtifactDigest) -> PathBuf {
    digest_path(config.paths().artifact_root().join("objects"), digest)
}

fn quarantine_path(config: &DaemonConfig, digest: ArtifactDigest) -> PathBuf {
    digest_path(config.paths().artifact_root().join("quarantine"), digest)
}

fn digest_path(root: PathBuf, digest: ArtifactDigest) -> PathBuf {
    let hex = digest.to_hex();
    root.join("sha256").join(&hex[..2]).join(hex)
}

fn store_config(config: &DaemonConfig) -> Result<StoreConfig, DaemonError> {
    StoreConfig::new(
        config.paths().artifact_root(),
        config.limits().maximum_artifact_bytes(),
        config.limits().artifact_quota_bytes(),
    )
    .and_then(|value| value.with_database_path(config.paths().database()))
    .map_err(store_error)
}

fn store_error(error: peritus_artifact_store::ArtifactStoreError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Storage,
        DaemonRecovery::Reconcile,
        "qualify artifact corruption",
        error.to_string(),
        error,
    )
}

fn filesystem_error(error: std::io::Error) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Storage,
        DaemonRecovery::Operator,
        "inject or inspect artifact corruption",
        error.to_string(),
        error,
    )
}

fn qualification_error(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::Operator,
        "qualify artifact corruption containment",
        detail,
    )
}
