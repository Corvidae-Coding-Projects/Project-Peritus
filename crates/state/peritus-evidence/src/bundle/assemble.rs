//! Streaming portable bundle assembly.

use super::format::{MAGIC, invalid};
use super::{BundleLimits, BundlePlan};
use crate::{EvidenceError, EvidenceErrorKind, RecoveryAction};
use peritus_artifact_store::{ArtifactDigest, ArtifactStore};
use peritus_types::Sha256Digest;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Write};

/// Successful streaming bundle observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BundleReceipt {
    root_digest: Sha256Digest,
    bundle_digest: Sha256Digest,
    byte_count: u64,
}

impl BundleReceipt {
    /// Returns the manifest-bound portable root digest.
    #[must_use]
    pub const fn root_digest(self) -> Sha256Digest {
        self.root_digest
    }
    /// Returns SHA-256 over every assembled bundle byte.
    #[must_use]
    pub const fn bundle_digest(self) -> Sha256Digest {
        self.bundle_digest
    }
    /// Returns the exact assembled byte count.
    #[must_use]
    pub const fn byte_count(self) -> u64 {
        self.byte_count
    }
}

/// Streams a deterministic bundle without buffering artifact objects.
///
/// # Errors
///
/// Returns stale/corrupt artifact, output I/O, size/digest, or configured-limit failures.
pub fn assemble_bundle<W: Write>(
    plan: &BundlePlan,
    artifacts: &ArtifactStore,
    output: W,
    limits: BundleLimits,
) -> Result<BundleReceipt, EvidenceError> {
    let mut writer = HashingWriter::new(output, limits.max_bundle_bytes());
    writer.write(MAGIC)?;
    let manifest = plan.manifest().canonical_bytes();
    write_sized(&mut writer, &manifest, limits.max_entry_bytes())?;
    write_count(&mut writer, plan.records().len(), limits.max_entries())?;
    for record in plan.records() {
        write_sized(&mut writer, &record.canonical_bytes(), limits.max_entry_bytes())?;
    }
    write_count(&mut writer, plan.frames().len(), limits.max_entries())?;
    for frame in plan.frames() {
        write_u64(&mut writer, frame.entry.global_position())?;
        write_sized(&mut writer, &frame.bytes, limits.max_entry_bytes())?;
    }
    write_count(&mut writer, plan.manifest().artifacts().len(), limits.max_entries())?;
    for entry in plan.manifest().artifacts() {
        writer.write(entry.digest().as_bytes())?;
        write_u64(&mut writer, entry.size())?;
        stream_artifact(&mut writer, artifacts, entry.digest(), entry.size(), limits)?;
    }
    writer.write(plan.manifest().root_digest().as_bytes())?;
    let (bundle_digest, byte_count) = writer.finish();
    Ok(BundleReceipt { root_digest: plan.manifest().root_digest(), bundle_digest, byte_count })
}

fn stream_artifact<W: Write>(
    writer: &mut HashingWriter<W>,
    store: &ArtifactStore,
    digest: ArtifactDigest,
    expected_size: u64,
    limits: BundleLimits,
) -> Result<(), EvidenceError> {
    if expected_size > limits.max_entry_bytes() {
        return Err(invalid("artifact exceeds entry limit"));
    }
    let metadata = store
        .verify(digest)
        .map_err(|error| EvidenceError::artifact("verify bundle artifact", error))?;
    if metadata.size() != expected_size {
        return Err(invalid("artifact size changed after planning"));
    }
    let path = artifact_path(store, digest);
    let mut file =
        File::open(path).map_err(|error| EvidenceError::io("open bundle artifact", error))?;
    let mut hasher = Sha256::new();
    let mut count = 0_u64;
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| EvidenceError::io("read bundle artifact", error))?;
        if read == 0 {
            break;
        }
        let read = u64::try_from(read).map_err(|_| invalid("artifact chunk length exceeds u64"))?;
        count = count.checked_add(read).ok_or_else(|| {
            EvidenceError::new(
                EvidenceErrorKind::ArithmeticOverflow,
                RecoveryAction::CorrectInput,
                "assemble evidence bundle",
                "artifact byte count overflowed",
            )
        })?;
        if count > expected_size {
            return Err(invalid("artifact grew during bundle assembly"));
        }
        let read =
            usize::try_from(read).map_err(|_| invalid("artifact chunk length exceeds usize"))?;
        hasher.update(&buffer[..read]);
        writer.write(&buffer[..read])?;
    }
    if count != expected_size || Sha256Digest::new(hasher.finalize().into()) != digest.sha256() {
        return Err(EvidenceError::new(
            EvidenceErrorKind::CorruptArtifact,
            RecoveryAction::RepairDependency,
            "assemble evidence bundle",
            "artifact size or digest changed during streaming",
        ));
    }
    Ok(())
}

fn artifact_path(store: &ArtifactStore, digest: ArtifactDigest) -> std::path::PathBuf {
    let hex = digest.to_hex();
    store.root().join("objects").join("sha256").join(&hex[..2]).join(hex)
}

pub(super) struct HashingWriter<W> {
    inner: W,
    hasher: Sha256,
    count: u64,
    limit: u64,
}

impl<W: Write> HashingWriter<W> {
    fn new(inner: W, limit: u64) -> Self {
        Self { inner, hasher: Sha256::new(), count: 0, limit }
    }
    pub(super) fn write(&mut self, bytes: &[u8]) -> Result<(), EvidenceError> {
        let length =
            u64::try_from(bytes.len()).map_err(|_| invalid("bundle byte count exceeds u64"))?;
        self.count = self
            .count
            .checked_add(length)
            .ok_or_else(|| invalid("bundle byte count overflowed"))?;
        if self.count > self.limit {
            return Err(invalid("bundle exceeds complete byte limit"));
        }
        self.inner
            .write_all(bytes)
            .map_err(|error| EvidenceError::io("write evidence bundle", error))?;
        self.hasher.update(bytes);
        Ok(())
    }
    fn finish(self) -> (Sha256Digest, u64) {
        (Sha256Digest::new(self.hasher.finalize().into()), self.count)
    }
}

fn write_count<W: Write>(
    writer: &mut HashingWriter<W>,
    count: usize,
    limit: u64,
) -> Result<(), EvidenceError> {
    let count = u64::try_from(count).map_err(|_| invalid("entry count exceeds u64"))?;
    if count > limit {
        return Err(invalid("entry count exceeds limit"));
    }
    write_u64(writer, count)
}
fn write_sized<W: Write>(
    writer: &mut HashingWriter<W>,
    bytes: &[u8],
    limit: u64,
) -> Result<(), EvidenceError> {
    let size = u64::try_from(bytes.len()).map_err(|_| invalid("entry size exceeds u64"))?;
    if size > limit {
        return Err(invalid("entry exceeds byte limit"));
    }
    write_u64(writer, size)?;
    writer.write(bytes)
}
fn write_u64<W: Write>(writer: &mut HashingWriter<W>, value: u64) -> Result<(), EvidenceError> {
    writer.write(&value.to_be_bytes())
}
