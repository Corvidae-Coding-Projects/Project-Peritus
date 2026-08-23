//! Immutable evidence drafts, tags, and admitted records.

use crate::canonical::{Reader, put_bytes, put_digest, put_revision, put_text, put_u64};
use crate::{EvidenceError, EvidenceErrorKind, JournalProvenance, RecoveryAction};
use peritus_artifact_store::ArtifactDigest;
use peritus_codec::sha256;
use peritus_types::{EvidenceId, RevisionTuple, Sha256Digest};

/// Maximum canonical artifact references on one evidence record.
pub const MAX_EVIDENCE_ARTIFACTS: usize = 4_096;
/// Maximum direct causal parents on one evidence record.
pub const MAX_EVIDENCE_CAUSES: usize = 4_096;
const MAX_TAG_BYTES: usize = 64;
const RECORD_PREFIX: &[u8] = b"peritus-evidence-record-v1\0";

/// Stable semantic kind of an evidence record.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EvidenceKind(String);

impl EvidenceKind {
    /// Validates and owns a stable lowercase ASCII kebab-case tag.
    ///
    /// # Errors
    ///
    /// Rejects empty, oversized, or noncanonical tags.
    pub fn new(value: impl Into<String>) -> Result<Self, EvidenceError> {
        validate_tag(value).map(Self)
    }

    /// Borrows the canonical tag.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Stable origin class of an evidence record.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EvidenceSource(String);

impl EvidenceSource {
    /// Validates and owns a stable lowercase ASCII kebab-case tag.
    ///
    /// # Errors
    ///
    /// Rejects empty, oversized, or noncanonical tags.
    pub fn new(value: impl Into<String>) -> Result<Self, EvidenceError> {
        validate_tag(value).map(Self)
    }

    /// Borrows the canonical tag.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn validate_tag(value: impl Into<String>) -> Result<String, EvidenceError> {
    let value = value.into();
    let bytes = value.as_bytes();
    let valid = !bytes.is_empty()
        && bytes.len() <= MAX_TAG_BYTES
        && bytes.first() != Some(&b'-')
        && bytes.last() != Some(&b'-')
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
        && !bytes.windows(2).any(|pair| pair == b"--");
    if valid { Ok(value) } else { Err(invalid("evidence tag must be bounded ASCII kebab-case")) }
}

/// Checked but not yet durable evidence admission request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceDraft {
    id: EvidenceId,
    kind: EvidenceKind,
    source: EvidenceSource,
    revision: RevisionTuple,
    journal_position: u64,
    payload_digest: Sha256Digest,
    artifacts: Vec<ArtifactDigest>,
    causes: Vec<EvidenceId>,
}

impl EvidenceDraft {
    /// Creates a bounded canonical evidence request.
    ///
    /// # Errors
    ///
    /// Rejects a zero journal position, oversized sets, duplicates, noncanonical order, or a
    /// direct self-cause.
    #[allow(clippy::too_many_arguments, reason = "all durable evidence bindings remain explicit")]
    pub fn new(
        id: EvidenceId,
        kind: EvidenceKind,
        source: EvidenceSource,
        revision: RevisionTuple,
        journal_position: u64,
        payload_digest: Sha256Digest,
        artifacts: Vec<ArtifactDigest>,
        causes: Vec<EvidenceId>,
    ) -> Result<Self, EvidenceError> {
        if journal_position == 0
            || artifacts.len() > MAX_EVIDENCE_ARTIFACTS
            || causes.len() > MAX_EVIDENCE_CAUSES
            || artifacts.windows(2).any(|pair| pair[0] >= pair[1])
            || causes.windows(2).any(|pair| pair[0] >= pair[1])
            || causes.contains(&id)
        {
            return Err(invalid("invalid bound, order, duplicate, or direct self-cause"));
        }
        Ok(Self { id, kind, source, revision, journal_position, payload_digest, artifacts, causes })
    }

    /// Returns the proposed evidence identity.
    #[must_use]
    pub const fn id(&self) -> EvidenceId {
        self.id
    }
    /// Returns the proposed semantic kind.
    #[must_use]
    pub const fn kind(&self) -> &EvidenceKind {
        &self.kind
    }
    /// Returns the proposed origin class.
    #[must_use]
    pub const fn source(&self) -> &EvidenceSource {
        &self.source
    }
    /// Returns the exact revision binding.
    #[must_use]
    pub const fn revision(&self) -> &RevisionTuple {
        &self.revision
    }
    /// Returns the producing journal position.
    #[must_use]
    pub const fn journal_position(&self) -> u64 {
        self.journal_position
    }
    /// Returns the proposed evidence payload digest.
    #[must_use]
    pub const fn payload_digest(&self) -> Sha256Digest {
        self.payload_digest
    }
    /// Borrows expected actual journal artifact references.
    #[must_use]
    pub fn artifacts(&self) -> &[ArtifactDigest] {
        &self.artifacts
    }
    /// Borrows canonical direct causal parents.
    #[must_use]
    pub fn causes(&self) -> &[EvidenceId] {
        &self.causes
    }
}

/// Immutable admitted or offline-reverified evidence record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceRecord {
    id: EvidenceId,
    kind: EvidenceKind,
    source: EvidenceSource,
    revision: RevisionTuple,
    provenance: JournalProvenance,
    payload_digest: Sha256Digest,
    artifacts: Vec<ArtifactDigest>,
    causes: Vec<EvidenceId>,
    record_digest: Sha256Digest,
}

impl EvidenceRecord {
    pub(crate) fn from_draft(draft: EvidenceDraft, provenance: JournalProvenance) -> Self {
        let mut record = Self {
            id: draft.id,
            kind: draft.kind,
            source: draft.source,
            revision: draft.revision,
            provenance,
            payload_digest: draft.payload_digest,
            artifacts: draft.artifacts,
            causes: draft.causes,
            record_digest: Sha256Digest::new([0; 32]),
        };
        record.record_digest = sha256(&record.canonical_body());
        record
    }

    /// Returns the stable evidence identity.
    #[must_use]
    pub const fn id(&self) -> EvidenceId {
        self.id
    }
    /// Returns the semantic kind.
    #[must_use]
    pub const fn kind(&self) -> &EvidenceKind {
        &self.kind
    }
    /// Returns the origin class.
    #[must_use]
    pub const fn source(&self) -> &EvidenceSource {
        &self.source
    }
    /// Returns the exact revision tuple.
    #[must_use]
    pub const fn revision(&self) -> &RevisionTuple {
        &self.revision
    }
    /// Returns exact committed journal provenance.
    #[must_use]
    pub const fn provenance(&self) -> JournalProvenance {
        self.provenance
    }
    /// Returns the evidence payload digest.
    #[must_use]
    pub const fn payload_digest(&self) -> Sha256Digest {
        self.payload_digest
    }
    /// Borrows canonical actual artifact digests.
    #[must_use]
    pub fn artifacts(&self) -> &[ArtifactDigest] {
        &self.artifacts
    }
    /// Borrows canonical direct parents.
    #[must_use]
    pub fn causes(&self) -> &[EvidenceId] {
        &self.causes
    }
    /// Returns the digest over every canonical record field.
    #[must_use]
    pub const fn record_digest(&self) -> Sha256Digest {
        self.record_digest
    }
    /// Encodes the complete portable record including its advertised digest.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let body = self.canonical_body();
        let mut bytes = Vec::with_capacity(body.len() + 40);
        put_bytes(&mut bytes, &body);
        put_digest(&mut bytes, self.record_digest);
        bytes
    }

    /// Decodes and re-verifies one inert portable record.
    ///
    /// # Errors
    ///
    /// Rejects malformed fields, bounds, ordering, or record digest mismatch.
    pub fn verify_portable(bytes: &[u8]) -> Result<Self, EvidenceError> {
        let mut outer = Reader::new(bytes);
        let body = outer.bytes(32 * 1024 * 1024)?;
        let advertised = outer.digest()?;
        outer.finish()?;
        if sha256(body) != advertised {
            return Err(bundle_invalid("record digest mismatch"));
        }
        decode_body(body, advertised)
    }

    fn canonical_body(&self) -> Vec<u8> {
        let mut bytes = RECORD_PREFIX.to_vec();
        bytes.extend_from_slice(self.id.as_bytes());
        put_text(&mut bytes, self.kind.as_str());
        put_text(&mut bytes, self.source.as_str());
        put_revision(&mut bytes, &self.revision);
        self.provenance.encode_into(&mut bytes);
        put_digest(&mut bytes, self.payload_digest);
        put_u64(&mut bytes, self.artifacts.len() as u64);
        for digest in &self.artifacts {
            put_digest(&mut bytes, digest.sha256());
        }
        put_u64(&mut bytes, self.causes.len() as u64);
        for cause in &self.causes {
            bytes.extend_from_slice(cause.as_bytes());
        }
        bytes
    }
}

fn decode_body(body: &[u8], digest: Sha256Digest) -> Result<EvidenceRecord, EvidenceError> {
    let mut reader = Reader::new(body);
    if reader.take(RECORD_PREFIX.len())? != RECORD_PREFIX {
        return Err(bundle_invalid("record prefix mismatch"));
    }
    let id = reader.evidence_id()?;
    let kind = EvidenceKind::new(reader.text(MAX_TAG_BYTES)?)
        .map_err(|_| bundle_invalid("record kind is not canonical"))?;
    let source = EvidenceSource::new(reader.text(MAX_TAG_BYTES)?)
        .map_err(|_| bundle_invalid("record source is not canonical"))?;
    let revision = reader.revision()?;
    let provenance = JournalProvenance::decode(&mut reader)
        .map_err(|_| bundle_invalid("record journal provenance is invalid"))?;
    let payload_digest = reader.digest()?;
    let artifact_count = bounded_count(reader.u64()?, MAX_EVIDENCE_ARTIFACTS)?;
    let mut artifacts = Vec::with_capacity(artifact_count);
    for _ in 0..artifact_count {
        artifacts.push(ArtifactDigest::from_sha256(reader.digest()?));
    }
    let cause_count = bounded_count(reader.u64()?, MAX_EVIDENCE_CAUSES)?;
    let mut causes = Vec::with_capacity(cause_count);
    for _ in 0..cause_count {
        causes.push(reader.evidence_id()?);
    }
    reader.finish()?;
    if artifacts.windows(2).any(|pair| pair[0] >= pair[1])
        || causes.windows(2).any(|pair| pair[0] >= pair[1])
        || causes.contains(&id)
    {
        return Err(bundle_invalid("record collections are not canonical"));
    }
    Ok(EvidenceRecord {
        id,
        kind,
        source,
        revision,
        provenance,
        payload_digest,
        artifacts,
        causes,
        record_digest: digest,
    })
}

fn bounded_count(value: u64, limit: usize) -> Result<usize, EvidenceError> {
    let value = usize::try_from(value).map_err(|_| bundle_invalid("record count overflows"))?;
    if value > limit { Err(bundle_invalid("record count exceeds limit")) } else { Ok(value) }
}

fn invalid(detail: &'static str) -> EvidenceError {
    EvidenceError::new(
        EvidenceErrorKind::InvalidInput,
        RecoveryAction::CorrectInput,
        "validate evidence draft",
        detail,
    )
}
fn bundle_invalid(detail: &'static str) -> EvidenceError {
    EvidenceError::new(
        EvidenceErrorKind::InvalidBundle,
        RecoveryAction::CorrectInput,
        "verify evidence record",
        detail,
    )
}
