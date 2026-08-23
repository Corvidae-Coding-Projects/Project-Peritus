#![allow(dead_code)]

use peritus_artifact_store::{
    ArtifactDigest, ArtifactStore, EncryptionMetadata, MediaType, StoreConfig, WriteRequest,
};
use peritus_types::EventId;
use sha2::{Digest, Sha256};
use tempfile::TempDir;

pub fn digest(bytes: &[u8]) -> ArtifactDigest {
    ArtifactDigest::new(Sha256::digest(bytes).into())
}

pub fn event(byte: u8) -> EventId {
    EventId::new([byte; 16]).expect("fixture event identity is nonzero")
}

pub fn request(bytes: &[u8], declared_limit: u64, event_byte: u8) -> WriteRequest {
    WriteRequest::new(
        digest(bytes),
        u64::try_from(bytes.len()).expect("fixture length fits u64"),
        declared_limit,
        MediaType::new("application/octet-stream").expect("static media type is valid"),
        EncryptionMetadata::unencrypted(),
        event(event_byte),
    )
}

pub fn store(max_artifact_bytes: u64, quota_bytes: u64) -> (TempDir, ArtifactStore) {
    let directory = tempfile::tempdir().expect("temporary store root");
    let config = StoreConfig::new(directory.path(), max_artifact_bytes, quota_bytes)
        .expect("fixture config is valid");
    let store = ArtifactStore::open(config).expect("store opens");
    (directory, store)
}

pub fn object_path(root: &std::path::Path, digest: ArtifactDigest) -> std::path::PathBuf {
    let hex = digest.to_hex();
    root.join("objects").join("sha256").join(&hex[..2]).join(hex)
}

pub fn quarantine_path(root: &std::path::Path, digest: ArtifactDigest) -> std::path::PathBuf {
    let hex = digest.to_hex();
    root.join("quarantine").join("sha256").join(&hex[..2]).join(hex)
}
