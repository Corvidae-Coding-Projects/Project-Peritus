//! Canonical portable evidence bundle manifest.

use crate::canonical::{Reader, put_digest, put_revision};
use crate::{EvidenceError, EvidenceErrorKind, EvidenceId, RecoveryAction};
use peritus_artifact_store::ArtifactDigest;
use peritus_codec::sha256;
use peritus_types::{EventId, RevisionTuple, Sha256Digest};
use sha2::{Digest, Sha256};
use std::convert::Infallible;

const PREFIX_V1: &[u8] = b"peritus-evidence-manifest-v1\0";
const PREFIX_V2: &[u8] = b"peritus-evidence-manifest-v2\0";
const LEGACY_MAX_MANIFEST_ENTRIES: usize = 4_096;

/// Manifest binding for one canonical evidence record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecordManifestEntry {
    id: EvidenceId,
    record_digest: Sha256Digest,
}

impl RecordManifestEntry {
    pub(crate) const fn new(id: EvidenceId, record_digest: Sha256Digest) -> Self {
        Self { id, record_digest }
    }
    /// Returns the evidence identity.
    #[must_use]
    pub const fn id(self) -> EvidenceId {
        self.id
    }
    /// Returns the complete record digest.
    #[must_use]
    pub const fn record_digest(self) -> Sha256Digest {
        self.record_digest
    }
}

/// Manifest binding for one exact committed journal frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JournalManifestEntry {
    global_position: u64,
    event_id: EventId,
    event_hash: Sha256Digest,
    frame_digest: Sha256Digest,
    schema_digest: Sha256Digest,
    frame_size: u64,
}

impl JournalManifestEntry {
    pub(crate) const fn new(
        global_position: u64,
        event_id: EventId,
        event_hash: Sha256Digest,
        frame_digest: Sha256Digest,
        schema_digest: Sha256Digest,
        frame_size: u64,
    ) -> Self {
        Self { global_position, event_id, event_hash, frame_digest, schema_digest, frame_size }
    }
    /// Returns the exact global journal position.
    #[must_use]
    pub const fn global_position(self) -> u64 {
        self.global_position
    }
    /// Returns the producing event identity.
    #[must_use]
    pub const fn event_id(self) -> EventId {
        self.event_id
    }
    /// Returns the journal event-chain hash.
    #[must_use]
    pub const fn event_hash(self) -> Sha256Digest {
        self.event_hash
    }
    /// Returns the exact complete-frame digest.
    #[must_use]
    pub const fn frame_digest(self) -> Sha256Digest {
        self.frame_digest
    }
    /// Returns the family-schema digest.
    #[must_use]
    pub const fn schema_digest(self) -> Sha256Digest {
        self.schema_digest
    }
    /// Returns the exact complete-frame byte length.
    #[must_use]
    pub const fn frame_size(self) -> u64 {
        self.frame_size
    }
}

/// Manifest binding for one exact artifact object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArtifactManifestEntry {
    digest: ArtifactDigest,
    size: u64,
}

impl ArtifactManifestEntry {
    pub(crate) const fn new(digest: ArtifactDigest, size: u64) -> Self {
        Self { digest, size }
    }
    /// Returns the finalized artifact digest.
    #[must_use]
    pub const fn digest(self) -> ArtifactDigest {
        self.digest
    }
    /// Returns exact artifact bytes.
    #[must_use]
    pub const fn size(self) -> u64 {
        self.size
    }
}

/// Immutable canonical bundle manifest and root binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceManifest {
    revision: RevisionTuple,
    journal_head_digest: Sha256Digest,
    records: Vec<RecordManifestEntry>,
    journal: Vec<JournalManifestEntry>,
    artifacts: Vec<ArtifactManifestEntry>,
    manifest_digest: Sha256Digest,
    root_digest: Sha256Digest,
}

impl EvidenceManifest {
    pub(crate) fn build(
        revision: RevisionTuple,
        journal_head_digest: Sha256Digest,
        records: Vec<RecordManifestEntry>,
        journal: Vec<JournalManifestEntry>,
        artifacts: Vec<ArtifactManifestEntry>,
    ) -> Result<Self, EvidenceError> {
        validate_entries(&records, &journal, &artifacts)?;
        let mut manifest = Self {
            revision,
            journal_head_digest,
            records,
            journal,
            artifacts,
            manifest_digest: Sha256Digest::new([0; 32]),
            root_digest: Sha256Digest::new([0; 32]),
        };
        manifest.manifest_digest = manifest.body_digest();
        manifest.root_digest = root_digest(manifest.manifest_digest);
        Ok(manifest)
    }

    /// Returns the exact common revision tuple.
    #[must_use]
    pub const fn revision(&self) -> &RevisionTuple {
        &self.revision
    }
    /// Returns the integrity-checked journal-head digest.
    #[must_use]
    pub const fn journal_head_digest(&self) -> Sha256Digest {
        self.journal_head_digest
    }
    /// Borrows canonical record bindings.
    #[must_use]
    pub fn records(&self) -> &[RecordManifestEntry] {
        &self.records
    }
    /// Borrows canonical journal bindings.
    #[must_use]
    pub fn journal(&self) -> &[JournalManifestEntry] {
        &self.journal
    }
    /// Borrows canonical artifact bindings.
    #[must_use]
    pub fn artifacts(&self) -> &[ArtifactManifestEntry] {
        &self.artifacts
    }
    /// Returns the digest over the canonical manifest body.
    #[must_use]
    pub const fn manifest_digest(&self) -> Sha256Digest {
        self.manifest_digest
    }
    /// Returns the portable bundle root digest.
    #[must_use]
    pub const fn root_digest(&self) -> Sha256Digest {
        self.root_digest
    }
    /// Encodes the complete portable manifest.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        let _ = self.write_canonical(&mut |chunk| {
            bytes.extend_from_slice(chunk);
            Ok(())
        });
        bytes
    }

    /// Decodes and verifies a complete portable manifest.
    ///
    /// # Errors
    ///
    /// Rejects malformed, oversized, noncanonical, or digest-invalid bytes.
    pub fn verify_portable(bytes: &[u8]) -> Result<Self, EvidenceError> {
        let body_length =
            bytes.len().checked_sub(64).ok_or_else(|| invalid("manifest is truncated"))?;
        let (body, advertised) = bytes.split_at(body_length);
        let mut trailer = Reader::new(advertised);
        let manifest_digest = trailer.digest()?;
        let advertised_root = trailer.digest()?;
        trailer.finish()?;
        if sha256(body) != manifest_digest || root_digest(manifest_digest) != advertised_root {
            return Err(invalid("manifest or root digest mismatch"));
        }
        let mut reader = Reader::new(body);
        let prefix = reader.take(PREFIX_V1.len())?;
        let legacy = if prefix == PREFIX_V1 {
            true
        } else if prefix == PREFIX_V2 {
            false
        } else {
            return Err(invalid("manifest prefix mismatch"));
        };
        let revision = reader.revision()?;
        let journal_head_digest = reader.digest()?;
        let records = decode_records(&mut reader, legacy)?;
        let journal = decode_journal(&mut reader, legacy)?;
        let artifacts = decode_artifacts(&mut reader, legacy)?;
        reader.finish()?;
        validate_entries(&records, &journal, &artifacts)?;
        let manifest = Self {
            revision,
            journal_head_digest,
            records,
            journal,
            artifacts,
            manifest_digest,
            root_digest: advertised_root,
        };
        if manifest.uses_legacy_encoding() != legacy {
            return Err(invalid("manifest encoding version is not canonical"));
        }
        Ok(manifest)
    }

    pub(crate) fn canonical_size(&self) -> Result<u64, EvidenceError> {
        self.canonical_body_size()?
            .checked_add(64)
            .ok_or_else(|| overflow("portable manifest length overflowed"))
    }

    pub(crate) fn write_canonical(
        &self,
        write: &mut impl FnMut(&[u8]) -> Result<(), EvidenceError>,
    ) -> Result<(), EvidenceError> {
        self.visit_body(write)?;
        write(self.manifest_digest.as_bytes())?;
        write(self.root_digest.as_bytes())
    }

    fn uses_legacy_encoding(&self) -> bool {
        self.records.len() <= LEGACY_MAX_MANIFEST_ENTRIES
            && self.journal.len() <= LEGACY_MAX_MANIFEST_ENTRIES
            && self.artifacts.len() <= LEGACY_MAX_MANIFEST_ENTRIES
    }

    fn prefix(&self) -> &'static [u8] {
        if self.uses_legacy_encoding() { PREFIX_V1 } else { PREFIX_V2 }
    }

    fn canonical_body_size(&self) -> Result<u64, EvidenceError> {
        let records = represented_bytes(self.records.len(), 48)?;
        let journal = represented_bytes(self.journal.len(), 128)?;
        let artifacts = represented_bytes(self.artifacts.len(), 40)?;
        [
            u64::try_from(self.prefix().len()).map_err(|_| overflow("manifest prefix"))?,
            96,
            32,
            8,
            records,
            8,
            journal,
            8,
            artifacts,
        ]
        .into_iter()
        .try_fold(0_u64, |total, length| {
            total.checked_add(length).ok_or_else(|| overflow("manifest body length overflowed"))
        })
    }

    fn visit_body<E>(
        &self,
        write: &mut impl FnMut(&[u8]) -> Result<(), E>,
    ) -> Result<(), E> {
        write(self.prefix())?;
        let mut revision = Vec::new();
        put_revision(&mut revision, &self.revision);
        write(&revision)?;
        write(self.journal_head_digest.as_bytes())?;
        write(&(self.records.len() as u64).to_be_bytes())?;
        for entry in &self.records {
            write(entry.id.as_bytes())?;
            write(entry.record_digest.as_bytes())?;
        }
        write(&(self.journal.len() as u64).to_be_bytes())?;
        for entry in &self.journal {
            write(&entry.global_position.to_be_bytes())?;
            write(entry.event_id.as_bytes())?;
            write(entry.event_hash.as_bytes())?;
            write(entry.frame_digest.as_bytes())?;
            write(entry.schema_digest.as_bytes())?;
            write(&entry.frame_size.to_be_bytes())?;
        }
        write(&(self.artifacts.len() as u64).to_be_bytes())?;
        for entry in &self.artifacts {
            write(entry.digest.as_bytes())?;
            write(&entry.size.to_be_bytes())?;
        }
        Ok(())
    }

    fn body_digest(&self) -> Sha256Digest {
        let mut hasher = Sha256::new();
        let result: Result<(), Infallible> = self.visit_body(&mut |chunk| {
            hasher.update(chunk);
            Ok(())
        });
        if let Err(never) = result {
            match never {}
        }
        Sha256Digest::new(hasher.finalize().into())
    }
}

fn validate_entries(
    records: &[RecordManifestEntry],
    journal: &[JournalManifestEntry],
    artifacts: &[ArtifactManifestEntry],
) -> Result<(), EvidenceError> {
    if records.is_empty()
        || records.windows(2).any(|pair| pair[0].id >= pair[1].id)
        || journal.windows(2).any(|pair| pair[0].global_position >= pair[1].global_position)
        || artifacts.windows(2).any(|pair| pair[0].digest >= pair[1].digest)
        || journal.iter().any(|entry| entry.global_position == 0 || entry.frame_size == 0)
    {
        Err(invalid("manifest entries violate bounds or canonical order"))
    } else {
        Ok(())
    }
}

fn decode_records(
    reader: &mut Reader<'_>,
    legacy: bool,
) -> Result<Vec<RecordManifestEntry>, EvidenceError> {
    let value = reader.u64()?;
    let count = count(reader, value, 48, legacy)?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| invalid("manifest record allocation failed"))?;
    for _ in 0..count {
        values.push(RecordManifestEntry::new(reader.evidence_id()?, reader.digest()?));
    }
    Ok(values)
}
fn decode_journal(
    reader: &mut Reader<'_>,
    legacy: bool,
) -> Result<Vec<JournalManifestEntry>, EvidenceError> {
    let value = reader.u64()?;
    let count = count(reader, value, 128, legacy)?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| invalid("manifest journal allocation failed"))?;
    for _ in 0..count {
        values.push(JournalManifestEntry::new(
            reader.u64()?,
            reader.event_id()?,
            reader.digest()?,
            reader.digest()?,
            reader.digest()?,
            reader.u64()?,
        ));
    }
    Ok(values)
}
fn decode_artifacts(
    reader: &mut Reader<'_>,
    legacy: bool,
) -> Result<Vec<ArtifactManifestEntry>, EvidenceError> {
    let value = reader.u64()?;
    let count = count(reader, value, 40, legacy)?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| invalid("manifest artifact allocation failed"))?;
    for _ in 0..count {
        values.push(ArtifactManifestEntry::new(
            ArtifactDigest::from_sha256(reader.digest()?),
            reader.u64()?,
        ));
    }
    Ok(values)
}
fn count(
    reader: &Reader<'_>,
    value: u64,
    entry_bytes: u64,
    legacy: bool,
) -> Result<usize, EvidenceError> {
    let count = usize::try_from(value).map_err(|_| invalid("manifest count overflows"))?;
    if legacy && count > LEGACY_MAX_MANIFEST_ENTRIES {
        return Err(invalid("legacy manifest count exceeds bound"));
    }
    let required = value
        .checked_mul(entry_bytes)
        .ok_or_else(|| invalid("manifest collection size overflows"))?;
    let remaining = u64::try_from(reader.remaining())
        .map_err(|_| invalid("manifest remaining bytes exceed u64"))?;
    if required > remaining {
        return Err(invalid("manifest count exceeds remaining bytes"));
    }
    Ok(count)
}
fn represented_bytes(count: usize, entry_bytes: u64) -> Result<u64, EvidenceError> {
    u64::try_from(count)
        .ok()
        .and_then(|count| count.checked_mul(entry_bytes))
        .ok_or_else(|| overflow("manifest collection length overflowed"))
}
fn overflow(detail: &'static str) -> EvidenceError {
    EvidenceError::new(
        EvidenceErrorKind::ArithmeticOverflow,
        RecoveryAction::CorrectInput,
        "encode evidence manifest",
        detail,
    )
}
fn root_digest(manifest: Sha256Digest) -> Sha256Digest {
    let mut bytes = b"peritus-evidence-bundle-root-v1\0".to_vec();
    put_digest(&mut bytes, manifest);
    sha256(&bytes)
}
fn invalid(detail: &'static str) -> EvidenceError {
    EvidenceError::new(
        EvidenceErrorKind::InvalidBundle,
        RecoveryAction::CorrectInput,
        "verify evidence manifest",
        detail,
    )
}
