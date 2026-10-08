//! Effect-free streaming verification of a portable evidence bundle.

use super::BundleLimits;
use super::format::{MAGIC, invalid};
use crate::{EvidenceManifest, EvidenceRecord};
use peritus_artifact_store::ArtifactDigest;
use peritus_codec::{CodecLimits, decode_frame};
use peritus_types::Sha256Digest;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::io::Read;

const STREAM_CHUNK_BYTES: usize = 64 * 1024;
const STREAM_CHUNK_BYTES_U64: u64 = 64 * 1024;

/// Successful offline verification result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedBundle {
    manifest: EvidenceManifest,
    bundle_digest: Sha256Digest,
    byte_count: u64,
}

impl VerifiedBundle {
    /// Borrows the fully reverified manifest.
    #[must_use]
    pub const fn manifest(&self) -> &EvidenceManifest {
        &self.manifest
    }

    /// Returns SHA-256 over every portable bundle byte.
    #[must_use]
    pub const fn bundle_digest(&self) -> Sha256Digest {
        self.bundle_digest
    }

    /// Returns the exact number of consumed bundle bytes.
    #[must_use]
    pub const fn byte_count(&self) -> u64 {
        self.byte_count
    }
}

/// Streams and re-verifies an inert portable bundle without consulting live state.
///
/// This API accepts only `Read`; replay cannot acquire journal, artifact-store, network, or
/// process-effect capabilities. Memory grows only after the corresponding input bytes have been
/// read, and optional caller budgets are checked before each allocation.
///
/// # Errors
///
/// Rejects truncation, trailing bytes, selected resource-limit violations, noncanonical ordering,
/// and any record, frame, schema, artifact, manifest, or root digest mismatch.
pub fn verify_bundle<R: Read>(
    input: R,
    limits: BundleLimits,
) -> Result<VerifiedBundle, crate::EvidenceError> {
    let mut reader = HashingReader::new(input, limits.max_bundle_bytes());
    if reader.fixed::<8>()? != *MAGIC {
        return Err(invalid("bundle magic mismatch"));
    }
    let manifest_bytes = reader.sized(limits.max_entry_bytes())?;
    let manifest = EvidenceManifest::verify_portable(&manifest_bytes)?;

    let records = read_records(&mut reader, &manifest, limits)?;
    read_frames(&mut reader, &manifest, &records, limits)?;
    read_artifacts(&mut reader, &manifest, &records, limits)?;

    if Sha256Digest::new(reader.fixed::<32>()?) != manifest.root_digest() {
        return Err(invalid("bundle root trailer mismatch"));
    }
    reader.require_eof()?;
    let (bundle_digest, byte_count) = reader.finish();
    Ok(VerifiedBundle { manifest, bundle_digest, byte_count })
}

fn read_records<R: Read>(
    reader: &mut HashingReader<R>,
    manifest: &EvidenceManifest,
    limits: BundleLimits,
) -> Result<Vec<EvidenceRecord>, crate::EvidenceError> {
    let count = reader.count(limits.max_entries())?;
    if count != manifest.records().len() {
        return Err(invalid("record count disagrees with manifest"));
    }
    let mut records = Vec::new();
    records
        .try_reserve_exact(count)
        .map_err(|_| invalid("bundle record allocation failed"))?;
    for expected in manifest.records() {
        let bytes = reader.sized(limits.max_entry_bytes())?;
        let record = EvidenceRecord::verify_portable(&bytes)?;
        if record.id() != expected.id()
            || record.record_digest() != expected.record_digest()
            || record.provenance().revision_digest()
                != crate::freshness::revision_digest(record.revision())
            || (manifest.is_authority(record.id())
                && !crate::verified::revisions_equal(
                    record.revision(),
                    manifest.authority_revision(),
                ))
        {
            return Err(invalid("record disagrees with its manifest binding"));
        }
        records.push(record);
    }
    super::format::validate_ancestry(&records, manifest.authority_records())?;
    Ok(records)
}

fn read_frames<R: Read>(
    reader: &mut HashingReader<R>,
    manifest: &EvidenceManifest,
    records: &[EvidenceRecord],
    limits: BundleLimits,
) -> Result<(), crate::EvidenceError> {
    let count = reader.count(limits.max_entries())?;
    if count != manifest.journal().len() {
        return Err(invalid("journal frame count disagrees with manifest"));
    }
    let record_positions: BTreeSet<_> =
        records.iter().map(|record| record.provenance().global_position()).collect();
    let manifest_positions: BTreeSet<_> =
        manifest.journal().iter().map(|entry| entry.global_position()).collect();
    if record_positions != manifest_positions {
        return Err(invalid("journal manifest does not exactly cover record provenance"));
    }
    for expected in manifest.journal() {
        if reader.u64()? != expected.global_position() {
            return Err(invalid("journal frame position disagrees with manifest"));
        }
        let production_limit = u64::try_from(CodecLimits::PRODUCTION.max_frame_bytes)
            .map_err(|_| invalid("production frame limit overflows u64"))?;
        let frame_limit = minimum_limit(limits.max_entry_bytes(), Some(production_limit));
        let bytes = reader.sized(frame_limit)?;
        let size = u64::try_from(bytes.len()).map_err(|_| invalid("frame size overflows u64"))?;
        if size != expected.frame_size() || peritus_codec::sha256(&bytes) != expected.frame_digest()
        {
            return Err(invalid("journal frame size or digest mismatch"));
        }
        let frame = decode_frame(&bytes, CodecLimits::PRODUCTION)
            .map_err(|_| invalid("journal frame is not canonical B3"))?;
        let header = frame.header();
        let schema = crate::provenance::schema_digest(header.family(), header.schema_version())
            .map_err(|_| invalid("journal frame family/schema is unsupported"))?;
        if schema != expected.schema_digest() {
            return Err(invalid("journal frame schema digest mismatch"));
        }
        let matching = records
            .iter()
            .filter(|record| record.provenance().global_position() == expected.global_position());
        for record in matching {
            let provenance = record.provenance();
            let bound = (
                provenance.event_id(),
                provenance.event_hash(),
                provenance.frame_digest(),
                provenance.schema_digest(),
                provenance.frame_family(),
                provenance.frame_schema_version(),
            );
            let observed = (
                expected.event_id(),
                expected.event_hash(),
                expected.frame_digest(),
                expected.schema_digest(),
                header.family(),
                header.schema_version(),
            );
            if bound != observed {
                return Err(invalid("journal frame disagrees with record provenance"));
            }
        }
    }
    Ok(())
}

fn read_artifacts<R: Read>(
    reader: &mut HashingReader<R>,
    manifest: &EvidenceManifest,
    records: &[EvidenceRecord],
    limits: BundleLimits,
) -> Result<(), crate::EvidenceError> {
    let count = reader.count(limits.max_entries())?;
    if count != manifest.artifacts().len() {
        return Err(invalid("artifact count disagrees with manifest"));
    }
    let expected: BTreeSet<_> =
        records.iter().flat_map(|record| record.artifacts().iter().copied()).collect();
    if expected != manifest.artifacts().iter().map(|entry| entry.digest()).collect() {
        return Err(invalid("manifest does not exactly cover record artifacts"));
    }
    for entry in manifest.artifacts() {
        if ArtifactDigest::from_sha256(Sha256Digest::new(reader.fixed::<32>()?)) != entry.digest() {
            return Err(invalid("artifact identity disagrees with manifest"));
        }
        let size = reader.u64()?;
        if size != entry.size() || limits.max_entry_bytes().is_some_and(|limit| size > limit) {
            return Err(invalid("artifact size disagrees with manifest or selected limit"));
        }
        reader.ensure_remaining(size)?;
        let mut remaining = size;
        let mut digest = Sha256::new();
        let mut buffer = vec![0_u8; STREAM_CHUNK_BYTES].into_boxed_slice();
        while remaining != 0 {
            let available = usize::try_from(remaining.min(STREAM_CHUNK_BYTES_U64))
                .map_err(|_| invalid("artifact chunk size overflows usize"))?;
            reader.read_exact(&mut buffer[..available])?;
            digest.update(&buffer[..available]);
            remaining -= u64::try_from(available)
                .map_err(|_| invalid("artifact chunk size overflows u64"))?;
        }
        if Sha256Digest::new(digest.finalize().into()) != entry.digest().sha256() {
            return Err(invalid("artifact content digest mismatch"));
        }
    }
    Ok(())
}

fn minimum_limit(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (Some(limit), None) | (None, Some(limit)) => Some(limit),
        (None, None) => None,
    }
}

struct HashingReader<R> {
    inner: R,
    hasher: Sha256,
    count: u64,
    limit: Option<u64>,
}

impl<R: Read> HashingReader<R> {
    fn new(inner: R, limit: Option<u64>) -> Self {
        Self { inner, hasher: Sha256::new(), count: 0, limit }
    }

    fn ensure_remaining(&self, length: u64) -> Result<(), crate::EvidenceError> {
        let next = self
            .count
            .checked_add(length)
            .ok_or_else(|| invalid("bundle byte count overflowed"))?;
        if self.limit.is_some_and(|limit| next > limit) {
            Err(invalid("bundle exceeds selected complete byte limit"))
        } else {
            Ok(())
        }
    }

    fn read_exact(&mut self, bytes: &mut [u8]) -> Result<(), crate::EvidenceError> {
        let length = u64::try_from(bytes.len()).map_err(|_| invalid("read size overflows u64"))?;
        self.ensure_remaining(length)?;
        self.inner.read_exact(bytes).map_err(|error| {
            if error.kind() == std::io::ErrorKind::UnexpectedEof {
                invalid("bundle is truncated")
            } else {
                crate::EvidenceError::io("read evidence bundle", error)
            }
        })?;
        self.hasher.update(bytes);
        self.count += length;
        Ok(())
    }

    fn fixed<const N: usize>(&mut self) -> Result<[u8; N], crate::EvidenceError> {
        let mut value = [0_u8; N];
        self.read_exact(&mut value)?;
        Ok(value)
    }

    fn u64(&mut self) -> Result<u64, crate::EvidenceError> {
        Ok(u64::from_be_bytes(self.fixed()?))
    }

    fn count(&mut self, limit: Option<u64>) -> Result<usize, crate::EvidenceError> {
        let value = self.u64()?;
        if limit.is_some_and(|limit| value > limit) {
            return Err(invalid("bundle collection count exceeds selected limit"));
        }
        usize::try_from(value).map_err(|_| invalid("bundle collection count overflows usize"))
    }

    fn sized(&mut self, limit: Option<u64>) -> Result<Vec<u8>, crate::EvidenceError> {
        let size = self.u64()?;
        if limit.is_some_and(|limit| size > limit) {
            return Err(invalid("bundle entry exceeds selected byte limit"));
        }
        self.ensure_remaining(size)?;
        let length = usize::try_from(size)
            .map_err(|_| invalid("bundle entry size overflows usize"))?;
        let mut bytes = Vec::new();
        let mut remaining = length;
        let mut buffer = vec![0_u8; STREAM_CHUNK_BYTES].into_boxed_slice();
        while remaining != 0 {
            let available = remaining.min(STREAM_CHUNK_BYTES);
            self.read_exact(&mut buffer[..available])?;
            bytes
                .try_reserve_exact(available)
                .map_err(|_| invalid("bundle entry allocation failed"))?;
            bytes.extend_from_slice(&buffer[..available]);
            remaining -= available;
        }
        Ok(bytes)
    }

    fn require_eof(&mut self) -> Result<(), crate::EvidenceError> {
        let mut trailing = [0_u8; 1];
        match self.inner.read(&mut trailing) {
            Ok(0) => Ok(()),
            Ok(_) => Err(invalid("bundle contains trailing bytes")),
            Err(error) => Err(crate::EvidenceError::io("finish evidence bundle", error)),
        }
    }

    fn finish(self) -> (Sha256Digest, u64) {
        (Sha256Digest::new(self.hasher.finalize().into()), self.count)
    }
}
