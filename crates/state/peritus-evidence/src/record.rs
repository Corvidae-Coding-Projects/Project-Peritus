//! Immutable evidence drafts, tags, and admitted records.

use crate::canonical::{Reader, put_digest, put_revision};
use crate::{EvidenceError, EvidenceErrorKind, JournalProvenance, RecoveryAction};
use peritus_artifact_store::ArtifactDigest;
use peritus_types::{EvidenceId, RevisionTuple, Sha256Digest};
use sha2::{Digest, Sha256};
use std::convert::Infallible;

const LEGACY_MAX_EVIDENCE_ARTIFACTS: usize = 4_096;
const LEGACY_MAX_EVIDENCE_CAUSES: usize = 4_096;
const LEGACY_MAX_TAG_BYTES: usize = 64;
const RECORD_PREFIX_V1: &[u8] = b"peritus-evidence-record-v1\0";
const RECORD_PREFIX_V2: &[u8] = b"peritus-evidence-record-v2\0";

/// Stable semantic kind of an evidence record.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EvidenceKind(String);

impl EvidenceKind {
    /// Validates and owns a stable lowercase ASCII kebab-case tag.
    ///
    /// # Errors
    ///
    /// Rejects empty or noncanonical tags.
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
    /// Rejects empty or noncanonical tags.
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
        && bytes.first() != Some(&b'-')
        && bytes.last() != Some(&b'-')
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
        && !bytes.windows(2).any(|pair| pair == b"--");
    if valid { Ok(value) } else { Err(invalid("evidence tag must be ASCII kebab-case")) }
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
    /// Creates a canonical evidence request.
    ///
    /// # Errors
    ///
    /// Rejects a zero journal position, duplicates, noncanonical order, or a direct self-cause.
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
        record.record_digest = record.body_digest();
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
        let mut bytes = Vec::new();
        let _ = self.write_canonical(&mut |chunk| {
            bytes.extend_from_slice(chunk);
            Ok(())
        });
        bytes
    }

    /// Decodes and re-verifies one inert portable record.
    ///
    /// # Errors
    ///
    /// Rejects malformed fields, bounds, ordering, or record digest mismatch.
    pub fn verify_portable(bytes: &[u8]) -> Result<Self, EvidenceError> {
        let mut outer = Reader::new(bytes);
        let body = outer.bytes_unbounded()?;
        let advertised = outer.digest()?;
        outer.finish()?;
        if sha256(body) != advertised {
            return Err(bundle_invalid("record digest mismatch"));
        }
        decode_body(body, advertised)
    }

    pub(crate) fn canonical_size(&self) -> Result<u64, EvidenceError> {
        self.canonical_body_size()?
            .checked_add(40)
            .ok_or_else(|| representation_overflow("portable record length overflowed"))
    }

    pub(crate) fn write_canonical(
        &self,
        write: &mut impl FnMut(&[u8]) -> Result<(), EvidenceError>,
    ) -> Result<(), EvidenceError> {
        write(&self.canonical_body_size()?.to_be_bytes())?;
        self.visit_body(write)?;
        write(self.record_digest.as_bytes())
    }

    fn uses_legacy_encoding(&self) -> bool {
        self.kind.as_str().len() <= LEGACY_MAX_TAG_BYTES
            && self.source.as_str().len() <= LEGACY_MAX_TAG_BYTES
            && self.artifacts.len() <= LEGACY_MAX_EVIDENCE_ARTIFACTS
            && self.causes.len() <= LEGACY_MAX_EVIDENCE_CAUSES
    }

    fn prefix(&self) -> &'static [u8] {
        if self.uses_legacy_encoding() { RECORD_PREFIX_V1 } else { RECORD_PREFIX_V2 }
    }

    fn canonical_body_size(&self) -> Result<u64, EvidenceError> {
        let mut fixed = Vec::new();
        put_revision(&mut fixed, &self.revision);
        self.provenance.encode_into(&mut fixed);
        put_digest(&mut fixed, self.payload_digest);
        let components = [
            u64::try_from(self.prefix().len())
                .map_err(|_| representation_overflow("record prefix exceeds u64"))?,
            16,
            8,
            u64::try_from(self.kind.as_str().len())
                .map_err(|_| representation_overflow("record kind exceeds u64"))?,
            8,
            u64::try_from(self.source.as_str().len())
                .map_err(|_| representation_overflow("record source exceeds u64"))?,
            u64::try_from(fixed.len())
                .map_err(|_| representation_overflow("record fixed fields exceed u64"))?,
            8,
            represented_bytes(self.artifacts.len(), 32)?,
            8,
            represented_bytes(self.causes.len(), 16)?,
        ];
        components.into_iter().try_fold(0_u64, |total, length| {
            total
                .checked_add(length)
                .ok_or_else(|| representation_overflow("portable record body overflowed"))
        })
    }

    fn visit_body<E>(
        &self,
        write: &mut impl FnMut(&[u8]) -> Result<(), E>,
    ) -> Result<(), E> {
        write(self.prefix())?;
        write(self.id.as_bytes())?;
        write(&(self.kind.as_str().len() as u64).to_be_bytes())?;
        write(self.kind.as_str().as_bytes())?;
        write(&(self.source.as_str().len() as u64).to_be_bytes())?;
        write(self.source.as_str().as_bytes())?;
        let mut fixed = Vec::new();
        put_revision(&mut fixed, &self.revision);
        self.provenance.encode_into(&mut fixed);
        put_digest(&mut fixed, self.payload_digest);
        write(&fixed)?;
        write(&(self.artifacts.len() as u64).to_be_bytes())?;
        for digest in &self.artifacts {
            write(digest.as_bytes())?;
        }
        write(&(self.causes.len() as u64).to_be_bytes())?;
        for cause in &self.causes {
            write(cause.as_bytes())?;
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

fn decode_body(body: &[u8], digest: Sha256Digest) -> Result<EvidenceRecord, EvidenceError> {
    let mut reader = Reader::new(body);
    let prefix = reader.take(RECORD_PREFIX_V1.len())?;
    let legacy = if prefix == RECORD_PREFIX_V1 {
        true
    } else if prefix == RECORD_PREFIX_V2 {
        false
    } else {
        return Err(bundle_invalid("record prefix mismatch"));
    };
    let id = reader.evidence_id()?;
    let kind_text = if legacy {
        reader.text(LEGACY_MAX_TAG_BYTES)?
    } else {
        reader.text_unbounded()?
    };
    let kind = EvidenceKind::new(kind_text)
        .map_err(|_| bundle_invalid("record kind is not canonical"))?;
    let source_text = if legacy {
        reader.text(LEGACY_MAX_TAG_BYTES)?
    } else {
        reader.text_unbounded()?
    };
    let source = EvidenceSource::new(source_text)
        .map_err(|_| bundle_invalid("record source is not canonical"))?;
    let revision = reader.revision()?;
    let provenance = JournalProvenance::decode(&mut reader)
        .map_err(|_| bundle_invalid("record journal provenance is invalid"))?;
    let payload_digest = reader.digest()?;
    let artifact_count_value = reader.u64()?;
    let artifact_count = represented_count(
        artifact_count_value,
        32,
        reader.remaining(),
        legacy.then_some(LEGACY_MAX_EVIDENCE_ARTIFACTS),
    )?;
    let mut artifacts = Vec::new();
    artifacts
        .try_reserve_exact(artifact_count)
        .map_err(|_| bundle_invalid("record artifact allocation failed"))?;
    for _ in 0..artifact_count {
        artifacts.push(ArtifactDigest::from_sha256(reader.digest()?));
    }
    let cause_count_value = reader.u64()?;
    let cause_count = represented_count(
        cause_count_value,
        16,
        reader.remaining(),
        legacy.then_some(LEGACY_MAX_EVIDENCE_CAUSES),
    )?;
    let mut causes = Vec::new();
    causes
        .try_reserve_exact(cause_count)
        .map_err(|_| bundle_invalid("record cause allocation failed"))?;
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
    let record = EvidenceRecord {
        id,
        kind,
        source,
        revision,
        provenance,
        payload_digest,
        artifacts,
        causes,
        record_digest: digest,
    };
    if record.uses_legacy_encoding() != legacy {
        return Err(bundle_invalid("record encoding version is not canonical"));
    }
    Ok(record)
}

fn represented_count(
    value: u64,
    entry_bytes: u64,
    remaining: usize,
    legacy_limit: Option<usize>,
) -> Result<usize, EvidenceError> {
    let count = usize::try_from(value).map_err(|_| bundle_invalid("record count overflows"))?;
    if legacy_limit.is_some_and(|limit| count > limit) {
        return Err(bundle_invalid("legacy record count exceeds bound"));
    }
    let required = value
        .checked_mul(entry_bytes)
        .ok_or_else(|| bundle_invalid("record collection size overflows"))?;
    let remaining = u64::try_from(remaining)
        .map_err(|_| bundle_invalid("record remaining bytes exceed u64"))?;
    if required > remaining {
        return Err(bundle_invalid("record count exceeds remaining bytes"));
    }
    Ok(count)
}

fn represented_bytes(count: usize, entry_bytes: u64) -> Result<u64, EvidenceError> {
    u64::try_from(count)
        .ok()
        .and_then(|count| count.checked_mul(entry_bytes))
        .ok_or_else(|| representation_overflow("portable record collection size overflowed"))
}

fn representation_overflow(detail: &'static str) -> EvidenceError {
    EvidenceError::new(
        EvidenceErrorKind::ArithmeticOverflow,
        RecoveryAction::CorrectInput,
        "encode evidence record",
        detail,
    )
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
