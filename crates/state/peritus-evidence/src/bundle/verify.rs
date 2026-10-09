//! Framed incremental verification with an owned input, hashes, and exact continuation cursor.

use super::{
    BundleLimits,
    format::{MAGIC, invalid},
};
use crate::{EvidenceCancellation, EvidenceError, EvidenceManifest, EvidenceRecord};
use peritus_codec::{CodecLimits, decode_frame};
use peritus_types::Sha256Digest;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, Read},
    num::NonZeroUsize,
};

const CHUNK: usize = 64 * 1024;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum State {
    Magic,
    ManifestSize,
    Manifest,
    RecordCount,
    RecordSize,
    Record,
    FrameCount,
    FramePosition,
    FrameSize,
    Frame,
    ArtifactCount,
    ArtifactDigest,
    ArtifactSize,
    Artifact,
    Root,
    Complete,
}

/// Observable progress of a framed verification operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BundleVerificationCursor {
    input_bytes: u64,
    records: usize,
    frames: usize,
    artifacts: usize,
}
impl BundleVerificationCursor {
    /// Returns exact bytes consumed, including an incomplete field retained by the owner.
    #[must_use]
    pub const fn input_bytes(self) -> u64 {
        self.input_bytes
    }
    /// Returns fully authenticated records.
    #[must_use]
    pub const fn records(self) -> usize {
        self.records
    }
    /// Returns fully authenticated journal frames.
    #[must_use]
    pub const fn frames(self) -> usize {
        self.frames
    }
    /// Returns fully authenticated artifact objects.
    #[must_use]
    pub const fn artifacts(self) -> usize {
        self.artifacts
    }
}

/// Owns a framed input and all partial fields and hashes until the root trailer is verified.
///
/// Completion uses the format's root trailer and never waits for transport EOF. The caller can
/// recover the input with [`Self::into_input`] to consume its next frame. [`verify_bundle`] adds
/// EOF validation for callers verifying a complete standalone file.
pub struct BundleVerificationOperation<R> {
    input: R,
    limits: BundleLimits,
    state: State,
    field: Vec<u8>,
    needed: u64,
    hasher: Sha256,
    artifact_hash: Sha256,
    cursor: BundleVerificationCursor,
    manifest: Option<EvidenceManifest>,
    records: Vec<EvidenceRecord>,
    positions: BTreeMap<u64, Vec<usize>>,
    verified: Option<VerifiedBundle>,
    failed: bool,
}

impl<R: Read> BundleVerificationOperation<R> {
    /// Takes ownership of input without reading it or waiting for EOF.
    #[must_use]
    pub fn new(input: R, limits: BundleLimits) -> Self {
        Self {
            input,
            limits,
            state: State::Magic,
            field: Vec::new(),
            needed: 8,
            hasher: Sha256::new(),
            artifact_hash: Sha256::new(),
            cursor: BundleVerificationCursor {
                input_bytes: 0,
                records: 0,
                frames: 0,
                artifacts: 0,
            },
            manifest: None,
            records: Vec::new(),
            positions: BTreeMap::new(),
            verified: None,
            failed: false,
        }
    }
    /// Returns exact retained progress without resetting any hash or input position.
    #[must_use]
    pub const fn cursor(&self) -> BundleVerificationCursor {
        self.cursor
    }
    /// Returns the completed result, if the root trailer has been authenticated.
    #[must_use]
    pub const fn result(&self) -> Option<&VerifiedBundle> {
        self.verified.as_ref()
    }
    /// Returns input at its exact consumed position, including after cancellation or failure.
    #[must_use]
    pub fn into_input(self) -> R {
        self.input
    }

    /// Advances at most `bytes` input bytes, retaining partial fields and artifact hashes.
    ///
    /// Returns `None` when more work remains and the receipt only at framed completion. A new
    /// cancellation signal can resume a cancelled operation. A corrupt frame permanently fails
    /// this owner; I/O errors retain the exact position for a caller-directed retry.
    ///
    /// # Errors
    /// Returns cancellation, I/O, selected resource limits, or canonical integrity failures.
    pub fn advance(
        &mut self,
        bytes: NonZeroUsize,
        cancellation: &EvidenceCancellation,
    ) -> Result<Option<&VerifiedBundle>, EvidenceError> {
        if self.failed {
            return Err(invalid("verification owner already rejected this frame"));
        }
        let outcome = self.advance_inner(bytes, cancellation);
        if let Err(error) = &outcome
            && !matches!(
                error.kind(),
                crate::EvidenceErrorKind::Io | crate::EvidenceErrorKind::Cancelled
            )
        {
            self.failed = true;
        }
        outcome?;
        Ok(self.verified.as_ref())
    }

    /// Continues bounded steps until framed completion, with cancellation between reads.
    ///
    /// # Errors
    /// Returns the same failures as [`Self::advance`].
    pub fn resume(
        &mut self,
        cancellation: &EvidenceCancellation,
    ) -> Result<VerifiedBundle, EvidenceError> {
        let budget = NonZeroUsize::new(CHUNK).ok_or_else(|| invalid("invalid chunk size"))?;
        loop {
            if let Some(value) = self.advance(budget, cancellation)? {
                return Ok(value.clone());
            }
        }
    }

    fn advance_inner(
        &mut self,
        bytes: NonZeroUsize,
        cancellation: &EvidenceCancellation,
    ) -> Result<(), EvidenceError> {
        let mut remaining = bytes.get();
        let mut buffer = vec![0_u8; remaining.min(CHUNK)].into_boxed_slice();
        loop {
            cancellation.check("verify framed evidence bundle")?;
            if self.state == State::Complete {
                return Ok(());
            }
            if self.needed == 0 {
                self.complete_field()?;
                continue;
            }
            if remaining == 0 {
                return Ok(());
            }
            let wanted = usize::try_from(self.needed.min(remaining.min(CHUNK) as u64))
                .map_err(|_| invalid("verification chunk exceeds usize"))?;
            let next = self
                .cursor
                .input_bytes
                .checked_add(wanted as u64)
                .ok_or_else(|| invalid("bundle input offset overflowed"))?;
            self.limits.check_bundle_bytes(next)?;
            // Reserve before consuming input so allocation failure cannot lose stream progress.
            if self.state != State::Artifact {
                self.field
                    .try_reserve(wanted)
                    .map_err(|_| invalid("bundle field allocation failed"))?;
            }
            let read = match self.input.read(&mut buffer[..wanted]) {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(EvidenceError::io("read framed evidence bundle", error)),
                Ok(0) => return Err(invalid("bundle is truncated")),
                Ok(read) => read,
            };
            self.hasher.update(&buffer[..read]);
            if self.state == State::Artifact {
                self.artifact_hash.update(&buffer[..read]);
            } else {
                self.field.extend_from_slice(&buffer[..read]);
            }
            self.needed -= read as u64;
            self.cursor.input_bytes += read as u64;
            remaining -= read;
        }
    }

    fn manifest(&self) -> Result<&EvidenceManifest, EvidenceError> {
        self.manifest.as_ref().ok_or_else(|| invalid("bundle manifest is absent"))
    }
    fn next(&mut self, state: State, needed: u64) {
        self.state = state;
        self.needed = needed;
        self.field.clear();
    }
    fn number(&self) -> Result<u64, EvidenceError> {
        Ok(u64::from_be_bytes(
            self.field.as_slice().try_into().map_err(|_| invalid("invalid integer field"))?,
        ))
    }
    fn count(&self, expected: usize) -> Result<(), EvidenceError> {
        let actual = self.number()?;
        self.limits.check_entries(actual)?;
        if actual != expected as u64 {
            return Err(invalid("section count disagrees with manifest"));
        }
        Ok(())
    }
    fn sized(&mut self, state: State) -> Result<(), EvidenceError> {
        let length = self.number()?;
        self.limits.check_entry_bytes(length)?;
        self.limits.check_bundle_bytes(
            self.cursor
                .input_bytes
                .checked_add(length)
                .ok_or_else(|| invalid("bundle size overflowed"))?,
        )?;
        usize::try_from(length).map_err(|_| invalid("metadata length exceeds usize"))?;
        self.next(state, length);
        Ok(())
    }
    fn complete_field(&mut self) -> Result<(), EvidenceError> {
        match self.state {
            State::Magic => {
                if self.field != MAGIC {
                    return Err(invalid("bundle magic mismatch"));
                }
                self.next(State::ManifestSize, 8);
            }
            State::ManifestSize => self.sized(State::Manifest)?,
            State::Manifest => {
                self.manifest = Some(EvidenceManifest::verify_portable(&self.field)?);
                let manifest = self.manifest()?;
                for count in
                    [manifest.records().len(), manifest.journal().len(), manifest.artifacts().len()]
                {
                    self.limits.check_entries(count as u64)?;
                }
                self.next(State::RecordCount, 8);
            }
            State::RecordCount => {
                self.count(self.manifest()?.records().len())?;
                self.next(State::RecordSize, 8);
            }
            State::RecordSize => self.sized(State::Record)?,
            State::Record => self.record()?,
            State::FrameCount => {
                self.count(self.manifest()?.journal().len())?;
                self.next(State::FramePosition, 8);
            }
            State::FramePosition => {
                if self.number()?
                    != self.manifest()?.journal()[self.cursor.frames].global_position()
                {
                    return Err(invalid("journal frame position disagrees with manifest"));
                }
                self.next(State::FrameSize, 8);
            }
            State::FrameSize => {
                if self.number()? != self.manifest()?.journal()[self.cursor.frames].frame_size()
                    || self.number()? > CodecLimits::PRODUCTION.max_frame_bytes as u64
                {
                    return Err(invalid(
                        "frame length disagrees with manifest or codec representation",
                    ));
                }
                self.sized(State::Frame)?;
            }
            State::Frame => self.frame()?,
            State::ArtifactCount => {
                let count = self.manifest()?.artifacts().len();
                self.count(count)?;
                self.next(if count == 0 { State::Root } else { State::ArtifactDigest }, 32);
            }
            State::ArtifactDigest => {
                if self.field
                    != self.manifest()?.artifacts()[self.cursor.artifacts].digest().as_bytes()
                {
                    return Err(invalid("artifact identity disagrees with manifest"));
                }
                self.next(State::ArtifactSize, 8);
            }
            State::ArtifactSize => {
                let size = self.number()?;
                if size != self.manifest()?.artifacts()[self.cursor.artifacts].size() {
                    return Err(invalid("artifact size disagrees with manifest"));
                }
                self.limits.check_entry_bytes(size)?;
                self.artifact_hash = Sha256::new();
                self.next(State::Artifact, size);
            }
            State::Artifact => {
                let digest = Sha256Digest::new(self.artifact_hash.clone().finalize().into());
                if digest != self.manifest()?.artifacts()[self.cursor.artifacts].digest().sha256() {
                    return Err(invalid("artifact content digest mismatch"));
                }
                self.cursor.artifacts += 1;
                let done = self.cursor.artifacts == self.manifest()?.artifacts().len();
                self.next(if done { State::Root } else { State::ArtifactDigest }, 32);
            }
            State::Root => {
                if self.field != self.manifest()?.root_digest().as_bytes() {
                    return Err(invalid("bundle root trailer mismatch"));
                }
                self.verified = Some(VerifiedBundle {
                    manifest: self.manifest()?.clone(),
                    bundle_digest: Sha256Digest::new(self.hasher.clone().finalize().into()),
                    byte_count: self.cursor.input_bytes,
                });
                self.next(State::Complete, 0);
            }
            State::Complete => {}
        }
        Ok(())
    }

    fn record(&mut self) -> Result<(), EvidenceError> {
        let record = EvidenceRecord::verify_portable(&self.field)?;
        let manifest = self.manifest()?;
        let expected = manifest.records()[self.cursor.records];
        if record.id() != expected.id()
            || record.record_digest() != expected.record_digest()
            || record.provenance().revision_digest() != crate::revision_digest(record.revision())
            || (manifest.is_authority(record.id())
                && !crate::verified::revisions_equal(record.revision(), manifest.revision()))
        {
            return Err(invalid("record disagrees with manifest or original revision binding"));
        }
        self.positions
            .entry(record.provenance().global_position())
            .or_default()
            .push(self.cursor.records);
        self.records.push(record);
        self.cursor.records += 1;
        if self.cursor.records == self.manifest()?.records().len() {
            let manifest = self.manifest()?;
            super::format::validate_ancestry(&self.records, manifest.authority_records())?;
            if self
                .positions
                .keys()
                .copied()
                .ne(manifest.journal().iter().map(|entry| entry.global_position()))
            {
                return Err(invalid("journal manifest does not exactly cover record provenance"));
            }
            let artifacts: BTreeSet<_> =
                self.records.iter().flat_map(|record| record.artifacts().iter().copied()).collect();
            if artifacts.iter().copied().ne(manifest.artifacts().iter().map(|entry| entry.digest()))
            {
                return Err(invalid("artifact manifest does not exactly cover record references"));
            }
            self.next(State::FrameCount, 8);
        } else {
            self.next(State::RecordSize, 8);
        }
        Ok(())
    }

    fn frame(&mut self) -> Result<(), EvidenceError> {
        let expected = self.manifest()?.journal()[self.cursor.frames];
        if peritus_codec::sha256(&self.field) != expected.frame_digest() {
            return Err(invalid("journal frame digest mismatch"));
        }
        let frame = decode_frame(&self.field, CodecLimits::PRODUCTION)
            .map_err(|_| invalid("journal frame is not canonical B3"))?;
        let header = frame.header();
        let schema = crate::provenance::schema_digest(header.family(), header.schema_version())?;
        if schema != expected.schema_digest() {
            return Err(invalid("journal frame schema mismatch"));
        }
        for index in self
            .positions
            .get(&expected.global_position())
            .ok_or_else(|| invalid("frame lacks evidence provenance"))?
        {
            let provenance = self.records[*index].provenance();
            if (
                provenance.event_id(),
                provenance.event_hash(),
                provenance.frame_digest(),
                provenance.schema_digest(),
                provenance.frame_family(),
                provenance.frame_schema_version(),
            ) != (
                expected.event_id(),
                expected.event_hash(),
                expected.frame_digest(),
                expected.schema_digest(),
                header.family(),
                header.schema_version(),
            ) {
                return Err(invalid("journal frame disagrees with record provenance"));
            }
        }
        self.cursor.frames += 1;
        self.next(
            if self.cursor.frames == self.manifest()?.journal().len() {
                State::ArtifactCount
            } else {
                State::FramePosition
            },
            8,
        );
        Ok(())
    }
}

/// Verifies a complete standalone bundle, including the absence of trailing bytes.
///
/// Use [`BundleVerificationOperation`] for framed transports that remain open after a bundle.
///
/// # Errors
/// Rejects truncation, trailing bytes, selected limits, noncanonical order, and digest mismatch.
pub fn verify_bundle<R: Read>(
    input: R,
    limits: BundleLimits,
) -> Result<VerifiedBundle, EvidenceError> {
    let mut operation = BundleVerificationOperation::new(input, limits);
    let verified = operation.resume(&EvidenceCancellation::new())?;
    let mut input = operation.into_input();
    let mut trailing = [0_u8; 1];
    loop {
        match input.read(&mut trailing) {
            Ok(0) => return Ok(verified),
            Ok(_) => return Err(invalid("bundle contains trailing bytes")),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(EvidenceError::io("finish evidence bundle", error)),
        }
    }
}
