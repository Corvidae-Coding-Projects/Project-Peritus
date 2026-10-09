//! Canonical portable evidence bundle manifest.

use crate::canonical::{Reader, put_digest, put_revision, put_u64};
use crate::record::{MAX_EVIDENCE_ARTIFACTS, MAX_EVIDENCE_CAUSES};
use crate::{EvidenceError, EvidenceErrorKind, EvidenceId, RecoveryAction};
use peritus_artifact_store::ArtifactDigest;
use peritus_codec::sha256;
use peritus_types::{EventId, RevisionTuple, Sha256Digest};

const PREFIX: &[u8] = b"peritus-evidence-manifest-v1\0";
/// Maximum records, frames, or artifacts in one portable bundle.
pub const MAX_MANIFEST_ENTRIES: usize = 4_096;

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
        manifest.manifest_digest = sha256(&manifest.body());
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
        let mut bytes = self.body();
        put_digest(&mut bytes, self.manifest_digest);
        put_digest(&mut bytes, self.root_digest);
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
        if reader.take(PREFIX.len())? != PREFIX {
            return Err(invalid("manifest prefix mismatch"));
        }
        let revision = reader.revision()?;
        let journal_head_digest = reader.digest()?;
        let records = decode_records(&mut reader)?;
        let journal = decode_journal(&mut reader)?;
        let artifacts = decode_artifacts(&mut reader)?;
        reader.finish()?;
        validate_entries(&records, &journal, &artifacts)?;
        Ok(Self {
            revision,
            journal_head_digest,
            records,
            journal,
            artifacts,
            manifest_digest,
            root_digest: advertised_root,
        })
    }

    fn body(&self) -> Vec<u8> {
        let mut bytes = PREFIX.to_vec();
        put_revision(&mut bytes, &self.revision);
        put_digest(&mut bytes, self.journal_head_digest);
        put_u64(&mut bytes, self.records.len() as u64);
        for entry in &self.records {
            bytes.extend_from_slice(entry.id.as_bytes());
            put_digest(&mut bytes, entry.record_digest);
        }
        put_u64(&mut bytes, self.journal.len() as u64);
        for entry in &self.journal {
            put_u64(&mut bytes, entry.global_position);
            bytes.extend_from_slice(entry.event_id.as_bytes());
            put_digest(&mut bytes, entry.event_hash);
            put_digest(&mut bytes, entry.frame_digest);
            put_digest(&mut bytes, entry.schema_digest);
            put_u64(&mut bytes, entry.frame_size);
        }
        put_u64(&mut bytes, self.artifacts.len() as u64);
        for entry in &self.artifacts {
            put_digest(&mut bytes, entry.digest.sha256());
            put_u64(&mut bytes, entry.size);
        }
        bytes
    }
}

fn validate_entries(
    records: &[RecordManifestEntry],
    journal: &[JournalManifestEntry],
    artifacts: &[ArtifactManifestEntry],
) -> Result<(), EvidenceError> {
    if records.is_empty()
        || records.len() > MAX_MANIFEST_ENTRIES
        || journal.len() > MAX_MANIFEST_ENTRIES
        || artifacts.len() > MAX_MANIFEST_ENTRIES
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

fn decode_records(reader: &mut Reader<'_>) -> Result<Vec<RecordManifestEntry>, EvidenceError> {
    let count = count(reader.u64()?)?;
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        values.push(RecordManifestEntry::new(reader.evidence_id()?, reader.digest()?));
    }
    Ok(values)
}
fn decode_journal(reader: &mut Reader<'_>) -> Result<Vec<JournalManifestEntry>, EvidenceError> {
    let count = count(reader.u64()?)?;
    let mut values = Vec::with_capacity(count);
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
fn decode_artifacts(reader: &mut Reader<'_>) -> Result<Vec<ArtifactManifestEntry>, EvidenceError> {
    let count = count(reader.u64()?)?;
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        values.push(ArtifactManifestEntry::new(
            ArtifactDigest::from_sha256(reader.digest()?),
            reader.u64()?,
        ));
    }
    Ok(values)
}
fn count(value: u64) -> Result<usize, EvidenceError> {
    let value = usize::try_from(value).map_err(|_| invalid("manifest count overflows"))?;
    if value > MAX_MANIFEST_ENTRIES || value > MAX_EVIDENCE_ARTIFACTS || value > MAX_EVIDENCE_CAUSES
    {
        Err(invalid("manifest count exceeds bound"))
    } else {
        Ok(value)
    }
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
