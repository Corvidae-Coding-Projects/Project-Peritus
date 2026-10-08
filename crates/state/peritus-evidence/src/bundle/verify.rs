//! Effect-free streaming verification of a portable evidence bundle.

use super::BundleLimits;
use super::format::{MAGIC, invalid};
use crate::{EvidenceCancellation, EvidenceManifest, EvidenceRecord};
use peritus_artifact_store::ArtifactDigest;
use peritus_codec::{CodecLimits, decode_frame};
use peritus_types::Sha256Digest;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::io::{Read, Seek, SeekFrom, Write};
use tempfile::NamedTempFile;

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

/// Truthful phase of an owned cancellable bundle verification operation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BundleVerificationPhase {
    /// External input is being copied into the operation-owned immutable stage.
    Receiving,
    /// Input reached EOF and the retained stage is being canonically verified.
    Verifying,
    /// Canonical verification completed successfully.
    Complete,
}

/// Retained byte progress for an owned verification operation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BundleVerificationCursor {
    input_bytes: u64,
    staged_bytes: u64,
}

impl BundleVerificationCursor {
    /// Returns bytes accepted from the owned input, including bytes pending a staging write.
    #[must_use]
    pub const fn input_bytes(self) -> u64 {
        self.input_bytes
    }
    /// Returns bytes retained in the private verification stage.
    #[must_use]
    pub const fn staged_bytes(self) -> u64 {
        self.staged_bytes
    }
}

/// Owned input, private stage, phase, and cursor for cancellable bundle verification.
pub struct BundleVerificationOperation<R> {
    input: R,
    staged: NamedTempFile,
    pending: Vec<u8>,
    pending_offset: usize,
    cursor: BundleVerificationCursor,
    phase: BundleVerificationPhase,
    limits: BundleLimits,
    verified: Option<VerifiedBundle>,
}

impl<R: Read> BundleVerificationOperation<R> {
    /// Creates an operation that owns its input until verification completes.
    ///
    /// # Errors
    ///
    /// Returns an I/O failure when private staging cannot be created.
    pub fn new(input: R, limits: BundleLimits) -> Result<Self, crate::EvidenceError> {
        let staged = NamedTempFile::new()
            .map_err(|error| crate::EvidenceError::io("create staged bundle input", error))?;
        Ok(Self {
            input,
            staged,
            pending: Vec::new(),
            pending_offset: 0,
            cursor: BundleVerificationCursor { input_bytes: 0, staged_bytes: 0 },
            phase: BundleVerificationPhase::Receiving,
            limits,
            verified: None,
        })
    }

    /// Returns exact retained input and staging progress.
    #[must_use]
    pub const fn cursor(&self) -> BundleVerificationCursor {
        self.cursor
    }

    /// Returns the truthful current verification phase.
    #[must_use]
    pub const fn phase(&self) -> BundleVerificationPhase {
        self.phase
    }

    /// Resumes external input, EOF observation, and canonical verification.
    ///
    /// Cancellation retains the owned input, pending bytes, private stage, phase, and cursor.
    ///
    /// # Errors
    ///
    /// Returns cancellation, I/O, resource-limit, or canonical verification failures.
    pub fn resume(
        &mut self,
        cancellation: &EvidenceCancellation,
    ) -> Result<VerifiedBundle, crate::EvidenceError> {
        if let Some(verified) = &self.verified {
            return Ok(verified.clone());
        }
        if matches!(self.phase, BundleVerificationPhase::Receiving) {
            self.receive(cancellation)?;
        }
        ensure_live(Some(cancellation), "verify staged evidence bundle")?;
        self.staged
            .as_file_mut()
            .seek(SeekFrom::Start(0))
            .map_err(|error| crate::EvidenceError::io("seek staged bundle input", error))?;
        let verified = verify_bundle_inner(
            self.staged.as_file_mut(),
            self.limits,
            Some(cancellation.clone()),
        )?;
        if verified.byte_count() != self.cursor.staged_bytes
            || self.cursor.input_bytes != self.cursor.staged_bytes
        {
            return Err(invalid("verified byte count disagrees with retained input cursor"));
        }
        self.phase = BundleVerificationPhase::Complete;
        self.verified = Some(verified.clone());
        Ok(verified)
    }

    fn receive(
        &mut self,
        cancellation: &EvidenceCancellation,
    ) -> Result<(), crate::EvidenceError> {
        let mut buffer = vec![0_u8; STREAM_CHUNK_BYTES].into_boxed_slice();
        loop {
            while self.pending_offset < self.pending.len() {
                ensure_live(Some(cancellation), "stage bundle input")?;
                let written = self
                    .staged
                    .as_file_mut()
                    .write(&self.pending[self.pending_offset..])
                    .map_err(|error| crate::EvidenceError::io("stage bundle input", error))?;
                if written == 0 {
                    return Err(crate::EvidenceError::io(
                        "stage bundle input",
                        std::io::Error::from(std::io::ErrorKind::WriteZero),
                    ));
                }
                self.pending_offset += written;
                self.cursor.staged_bytes = self
                    .cursor
                    .staged_bytes
                    .checked_add(
                        u64::try_from(written)
                            .map_err(|_| invalid("staged input write exceeds u64"))?,
                    )
                    .ok_or_else(|| invalid("staged input cursor overflowed"))?;
                ensure_live(Some(cancellation), "stage bundle input")?;
            }
            self.pending.clear();
            self.pending_offset = 0;

            let wanted_u64 = self.limits.max_bundle_bytes().map_or(
                STREAM_CHUNK_BYTES_U64,
                |limit| {
                    limit
                        .saturating_sub(self.cursor.input_bytes)
                        .min(STREAM_CHUNK_BYTES_U64)
                        .max(1)
                },
            );
            let wanted = usize::try_from(wanted_u64)
                .map_err(|_| invalid("bundle input chunk exceeds usize"))?;
            ensure_live(Some(cancellation), "read bundle input")?;
            let read = self
                .input
                .read(&mut buffer[..wanted])
                .map_err(|error| crate::EvidenceError::io("read bundle input", error))?;
            if read == 0 {
                ensure_live(Some(cancellation), "finish bundle input")?;
                if self.cursor.input_bytes != self.cursor.staged_bytes {
                    return Err(invalid("bundle input reached EOF with unstaged bytes"));
                }
                self.staged
                    .as_file_mut()
                    .flush()
                    .map_err(|error| crate::EvidenceError::io("flush staged bundle input", error))?;
                ensure_live(Some(cancellation), "finish bundle input")?;
                self.phase = BundleVerificationPhase::Verifying;
                return Ok(());
            }
            self.pending.extend_from_slice(&buffer[..read]);
            self.cursor.input_bytes = self
                .cursor
                .input_bytes
                .checked_add(
                    u64::try_from(read).map_err(|_| invalid("bundle input read exceeds u64"))?,
                )
                .ok_or_else(|| invalid("bundle input cursor overflowed"))?;
            if self
                .limits
                .max_bundle_bytes()
                .is_some_and(|limit| self.cursor.input_bytes > limit)
            {
                return Err(invalid("bundle exceeds selected complete byte limit"));
            }
            ensure_live(Some(cancellation), "read bundle input")?;
        }
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
    verify_bundle_inner(input, limits, None)
}

fn verify_bundle_inner<R: Read>(
    input: R,
    limits: BundleLimits,
    cancellation: Option<EvidenceCancellation>,
) -> Result<VerifiedBundle, crate::EvidenceError> {
    let mut reader = HashingReader::new(input, limits.max_bundle_bytes(), cancellation);
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
    cancellation: Option<EvidenceCancellation>,
}

impl<R: Read> HashingReader<R> {
    fn new(
        inner: R,
        limit: Option<u64>,
        cancellation: Option<EvidenceCancellation>,
    ) -> Self {
        Self { inner, hasher: Sha256::new(), count: 0, limit, cancellation }
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
        ensure_live(self.cancellation.as_ref(), "read evidence bundle")?;
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
        ensure_live(self.cancellation.as_ref(), "read evidence bundle")
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
        ensure_live(self.cancellation.as_ref(), "finish evidence bundle")?;
        let mut trailing = [0_u8; 1];
        let result = match self.inner.read(&mut trailing) {
            Ok(0) => Ok(()),
            Ok(_) => Err(invalid("bundle contains trailing bytes")),
            Err(error) => Err(crate::EvidenceError::io("finish evidence bundle", error)),
        };
        ensure_live(self.cancellation.as_ref(), "finish evidence bundle")?;
        result
    }

    fn finish(self) -> (Sha256Digest, u64) {
        (Sha256Digest::new(self.hasher.finalize().into()), self.count)
    }
}

fn ensure_live(
    cancellation: Option<&EvidenceCancellation>,
    operation: &'static str,
) -> Result<(), crate::EvidenceError> {
    if cancellation.is_some_and(EvidenceCancellation::is_cancelled) {
        Err(crate::EvidenceError::cancelled(operation))
    } else {
        Ok(())
    }
}
