//! Exact committed journal provenance bound into evidence records.

use crate::canonical::{Reader, put_digest, put_u16, put_u64};
use crate::{EvidenceError, EvidenceErrorKind, RecoveryAction};
use peritus_codec::sha256;
use peritus_protocol::schema::FAMILIES;
use peritus_types::{EventId, Sha256Digest};

/// Immutable provenance for one digest-verified committed journal record.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct JournalProvenance {
    global_position: u64,
    event_id: EventId,
    event_hash: Sha256Digest,
    batch_hash: Sha256Digest,
    journal_head_digest: Sha256Digest,
    frame_family: u16,
    frame_schema_version: u16,
    frame_digest: Sha256Digest,
    schema_digest: Sha256Digest,
    revision_digest: Sha256Digest,
}

impl JournalProvenance {
    #[allow(
        clippy::too_many_arguments,
        reason = "every immutable journal binding remains explicit"
    )]
    pub(crate) const fn new(
        global_position: u64,
        event_id: EventId,
        event_hash: Sha256Digest,
        batch_hash: Sha256Digest,
        journal_head_digest: Sha256Digest,
        frame_family: u16,
        frame_schema_version: u16,
        frame_digest: Sha256Digest,
        schema_digest: Sha256Digest,
        revision_digest: Sha256Digest,
    ) -> Self {
        Self {
            global_position,
            event_id,
            event_hash,
            batch_hash,
            journal_head_digest,
            frame_family,
            frame_schema_version,
            frame_digest,
            schema_digest,
            revision_digest,
        }
    }

    /// Returns the one-based global journal position.
    #[must_use]
    pub const fn global_position(self) -> u64 {
        self.global_position
    }
    /// Returns the producing event identity.
    #[must_use]
    pub const fn event_id(self) -> EventId {
        self.event_id
    }
    /// Returns the immutable event-chain hash.
    #[must_use]
    pub const fn event_hash(self) -> Sha256Digest {
        self.event_hash
    }
    /// Returns the owning atomic journal batch hash.
    #[must_use]
    pub const fn batch_hash(self) -> Sha256Digest {
        self.batch_hash
    }
    /// Returns the integrity-export journal-head digest.
    #[must_use]
    pub const fn journal_head_digest(self) -> Sha256Digest {
        self.journal_head_digest
    }
    /// Returns the stable B3 family tag.
    #[must_use]
    pub const fn frame_family(self) -> u16 {
        self.frame_family
    }
    /// Returns the family schema version.
    #[must_use]
    pub const fn frame_schema_version(self) -> u16 {
        self.frame_schema_version
    }
    /// Returns the exact complete-frame digest.
    #[must_use]
    pub const fn frame_digest(self) -> Sha256Digest {
        self.frame_digest
    }
    /// Returns the derived immutable family-schema digest.
    #[must_use]
    pub const fn schema_digest(self) -> Sha256Digest {
        self.schema_digest
    }
    /// Returns the journal revision digest.
    #[must_use]
    pub const fn revision_digest(self) -> Sha256Digest {
        self.revision_digest
    }

    pub(crate) fn encode_into(self, bytes: &mut Vec<u8>) {
        put_u64(bytes, self.global_position);
        bytes.extend_from_slice(self.event_id.as_bytes());
        put_digest(bytes, self.event_hash);
        put_digest(bytes, self.batch_hash);
        put_digest(bytes, self.journal_head_digest);
        put_u16(bytes, self.frame_family);
        put_u16(bytes, self.frame_schema_version);
        put_digest(bytes, self.frame_digest);
        put_digest(bytes, self.schema_digest);
        put_digest(bytes, self.revision_digest);
    }

    pub(crate) fn decode(reader: &mut Reader<'_>) -> Result<Self, EvidenceError> {
        let global_position = reader.u64()?;
        if global_position == 0 {
            return Err(invalid("journal position must be positive"));
        }
        Ok(Self::new(
            global_position,
            reader.event_id()?,
            reader.digest()?,
            reader.digest()?,
            reader.digest()?,
            reader.u16()?,
            reader.u16()?,
            reader.digest()?,
            reader.digest()?,
            reader.digest()?,
        ))
    }
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "the private provenance module shares schema derivation with bundle verification"
)]
pub(crate) fn schema_digest(family: u16, version: u16) -> Result<Sha256Digest, EvidenceError> {
    let registered = FAMILIES
        .iter()
        .find(|candidate| candidate.tag == family && candidate.schema_version == version)
        .ok_or_else(|| invalid("journal frame family/schema is unsupported"))?;
    let mut bytes = b"peritus-evidence-frame-schema-v1\0".to_vec();
    put_u16(&mut bytes, family);
    put_u16(&mut bytes, version);
    bytes.extend_from_slice(registered.name.as_bytes());
    Ok(sha256(&bytes))
}

fn invalid(detail: &'static str) -> EvidenceError {
    EvidenceError::new(
        EvidenceErrorKind::JournalMismatch,
        RecoveryAction::RepairDependency,
        "validate journal provenance",
        detail,
    )
}
