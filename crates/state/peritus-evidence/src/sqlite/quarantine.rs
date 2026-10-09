//! Immutable audit observations and exact identity fences for quarantined evidence.

mod recovery;

use peritus_types::{EvidenceId, Sha256Digest};
use rusqlite::{OptionalExtension, Row, Transaction, TransactionBehavior, params};
use sha2::{Digest, Sha256};

use super::store::EvidenceStore;
use crate::{EvidenceError, EvidenceErrorKind, RecoveryAction};

/// Permanent identity of one immutable quarantine observation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EvidenceQuarantineId(Sha256Digest);

impl EvidenceQuarantineId {
    /// Restores an exact persisted audit identity; catalog lookup verifies its copied content.
    #[must_use]
    pub const fn new(digest: Sha256Digest) -> Self {
        Self(digest)
    }

    /// Returns the digest that permanently names the copied observation.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.0
    }
}

/// Tamper-evident audit identity for one valid evidence identity removed from active use.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvidenceQuarantine {
    quarantine_id: EvidenceQuarantineId,
    evidence_id: EvidenceId,
    indexed_record_digest_sha256: Sha256Digest,
    record_bytes_sha256: Sha256Digest,
    quarantine_digest: Sha256Digest,
    record_bytes: u64,
    reconciled_record_digest: Option<Sha256Digest>,
}

impl EvidenceQuarantine {
    /// Returns the permanent identity of this copied quarantine observation.
    #[must_use]
    pub const fn quarantine_id(self) -> EvidenceQuarantineId {
        self.quarantine_id
    }
    /// Returns the isolated evidence identity.
    #[must_use]
    pub const fn evidence_id(self) -> EvidenceId {
        self.evidence_id
    }
    /// Hashes the exact record-digest column, even when that column is malformed.
    #[must_use]
    pub const fn indexed_record_digest_sha256(self) -> Sha256Digest {
        self.indexed_record_digest_sha256
    }
    /// Hashes the copied corrupt portable record bytes.
    #[must_use]
    pub const fn record_bytes_sha256(self) -> Sha256Digest {
        self.record_bytes_sha256
    }
    /// Binds every copied indexed field, byte, and containment reason.
    #[must_use]
    pub const fn quarantine_digest(self) -> Sha256Digest {
        self.quarantine_digest
    }
    /// Returns the copied corrupt portable-record byte count.
    #[must_use]
    pub const fn record_bytes(self) -> u64 {
        self.record_bytes
    }
    /// Returns the exact active record digest accepted by explicit reconciliation.
    #[must_use]
    pub const fn reconciled_record_digest(self) -> Option<Sha256Digest> {
        self.reconciled_record_digest
    }
}

/// Audit projection that can truthfully represent a malformed raw evidence identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvidenceQuarantineAudit {
    quarantine_id: EvidenceQuarantineId,
    evidence_id: Option<EvidenceId>,
    raw_evidence_id_sha256: Sha256Digest,
    indexed_record_digest_sha256: Sha256Digest,
    record_bytes_sha256: Sha256Digest,
    record_bytes: u64,
    reconciled_record_digest: Option<Sha256Digest>,
}

impl EvidenceQuarantineAudit {
    /// Returns the permanent identity of this copied observation.
    #[must_use]
    pub const fn quarantine_id(self) -> EvidenceQuarantineId {
        self.quarantine_id
    }
    /// Returns the canonical evidence identity, or `None` when the copied raw key was malformed.
    #[must_use]
    pub const fn evidence_id(self) -> Option<EvidenceId> {
        self.evidence_id
    }
    /// Hashes the exact raw identity bytes, including a malformed key.
    #[must_use]
    pub const fn raw_evidence_id_sha256(self) -> Sha256Digest {
        self.raw_evidence_id_sha256
    }
    /// Hashes the exact copied record-digest column.
    #[must_use]
    pub const fn indexed_record_digest_sha256(self) -> Sha256Digest {
        self.indexed_record_digest_sha256
    }
    /// Hashes the copied portable record bytes.
    #[must_use]
    pub const fn record_bytes_sha256(self) -> Sha256Digest {
        self.record_bytes_sha256
    }
    /// Returns the copied portable-record byte count.
    #[must_use]
    pub const fn record_bytes(self) -> u64 {
        self.record_bytes
    }
    /// Returns the exact active record digest accepted by explicit reconciliation.
    #[must_use]
    pub const fn reconciled_record_digest(self) -> Option<Sha256Digest> {
        self.reconciled_record_digest
    }
}

impl EvidenceStore {
    /// Returns the verified quarantine audit identity for one canonical evidence identity.
    ///
    /// Unresolved observations take precedence; otherwise the deterministic latest permanent
    /// identity is returned. Historical copied evidence remains available after reconciliation.
    ///
    /// # Errors
    ///
    /// Returns storage or corrupt-catalog failure when the retained row cannot be decoded or its
    /// audit digest differs from the copied bytes.
    pub fn quarantined(&self, id: EvidenceId) -> Result<Option<EvidenceQuarantine>, EvidenceError> {
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Deferred)
                .map_err(|error| EvidenceError::sqlite("begin evidence quarantine read", error))?;
        let raw = quarantine_row_by_evidence(&transaction, id)?;
        transaction
            .commit()
            .map_err(|error| EvidenceError::sqlite("finish evidence quarantine read", error))?;
        raw.map(|row| row.observation()).transpose()
    }

    /// Returns a verified audit projection by permanent quarantine identity.
    ///
    /// # Errors
    ///
    /// Returns storage or corrupt-catalog failure when copied bytes or resolution metadata do not
    /// match their immutable digests.
    pub fn quarantine_audit(
        &self,
        id: EvidenceQuarantineId,
    ) -> Result<Option<EvidenceQuarantineAudit>, EvidenceError> {
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Deferred)
                .map_err(|error| EvidenceError::sqlite("begin evidence quarantine audit", error))?;
        let raw = quarantine_row_by_id(&transaction, id)?;
        transaction
            .commit()
            .map_err(|error| EvidenceError::sqlite("finish evidence quarantine audit", error))?;
        raw.map(|row| row.audit()).transpose()
    }

    /// Pages permanent quarantine identities without decoding raw evidence keys.
    ///
    /// # Errors
    ///
    /// Returns storage or corrupt-catalog failure when an identity is malformed.
    pub fn quarantine_identities(
        &self,
        after: Option<EvidenceQuarantineId>,
        limit: u16,
    ) -> Result<Vec<EvidenceQuarantineId>, EvidenceError> {
        let after = after.map(|value| value.0.as_bytes().to_vec());
        let mut statement = self
            .connection
            .prepare(
                "SELECT quarantine_id FROM peritus_evidence_quarantine
                      WHERE (?1 IS NULL OR quarantine_id > ?1)
                      ORDER BY quarantine_id LIMIT ?2",
            )
            .map_err(|error| {
                EvidenceError::sqlite("prepare evidence quarantine identities", error)
            })?;
        statement
            .query_map(params![after.as_deref(), i64::from(limit)], |row| row.get::<_, Vec<u8>>(0))
            .map_err(|error| EvidenceError::sqlite("query evidence quarantine identities", error))?
            .map(|row| {
                let bytes = row.map_err(|error| {
                    EvidenceError::sqlite("read evidence quarantine identity", error)
                })?;
                Ok(EvidenceQuarantineId(fixed_digest(&bytes, "quarantine identity")?))
            })
            .collect()
    }

    /// Counts all retained immutable quarantine observations, including reconciled history.
    ///
    /// # Errors
    ///
    /// Returns storage or arithmetic failure when the quarantine catalog cannot be counted.
    pub fn quarantine_count(&self) -> Result<u64, EvidenceError> {
        let count: i64 = self
            .connection
            .query_row("SELECT COUNT(*) FROM peritus_evidence_quarantine", [], |row| row.get(0))
            .map_err(|error| EvidenceError::sqlite("count evidence quarantine", error))?;
        u64::try_from(count).map_err(|_| corrupt("evidence quarantine count is negative"))
    }
}

pub(super) fn contain(
    transaction: &Transaction<'_>,
    raw_identity: &[u8],
    error: &EvidenceError,
) -> Result<(), EvidenceError> {
    let mut raw = record_row_by_raw(transaction, raw_identity, error.to_string())?
        .ok_or_else(|| corrupt("corrupt evidence row disappeared before containment"))?;
    let digest = raw.digest();
    raw.quarantine_id = digest.as_bytes().to_vec();
    raw.quarantine_digest = digest.as_bytes().to_vec();
    let inserted = transaction
        .execute(
            "INSERT OR IGNORE INTO peritus_evidence_quarantine(
                quarantine_id, evidence_id, quarantine_digest, record_digest, global_position,
                event_id, batch_hash, revision_digest, record_bytes, detected_error,
                reconciled_record_digest, reconciliation_digest
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, NULL, NULL)",
            params![
                raw.quarantine_id.as_slice(),
                raw.evidence_id.as_slice(),
                raw.quarantine_digest.as_slice(),
                raw.record_digest.as_slice(),
                raw.global_position,
                raw.event_id.as_slice(),
                raw.batch_hash.as_slice(),
                raw.revision_digest.as_slice(),
                raw.record_bytes.as_slice(),
                raw.detected_error,
            ],
        )
        .map_err(|error| EvidenceError::sqlite("quarantine corrupt evidence", error))?;
    if inserted == 0 {
        let existing = quarantine_row_by_id(transaction, EvidenceQuarantineId(digest))?
            .ok_or_else(|| corrupt("quarantine identity conflict has no retained row"))?;
        if !raw.same_quarantine_copy(&existing) {
            return Err(corrupt("quarantine identity collides with different copied evidence"));
        }
        transaction
            .execute(
                "UPDATE peritus_evidence_quarantine
                    SET reconciled_record_digest = NULL, reconciliation_digest = NULL
                  WHERE quarantine_id = ?1",
                [digest.as_bytes().as_slice()],
            )
            .map_err(|error| EvidenceError::sqlite("reopen evidence quarantine", error))?;
    }
    Ok(())
}

fn record_row_by_raw(
    transaction: &Transaction<'_>,
    raw_identity: &[u8],
    detected_error: String,
) -> Result<Option<RawEvidence>, EvidenceError> {
    transaction
        .query_row(
            "SELECT evidence_id, record_digest, global_position, event_id, batch_hash,
                    revision_digest, record_bytes
               FROM peritus_evidence_records WHERE evidence_id = ?1",
            [raw_identity],
            |row| {
                Ok(RawEvidence {
                    quarantine_id: Vec::new(),
                    evidence_id: row.get(0)?,
                    record_digest: row.get(1)?,
                    global_position: row.get(2)?,
                    event_id: row.get(3)?,
                    batch_hash: row.get(4)?,
                    revision_digest: row.get(5)?,
                    record_bytes: row.get(6)?,
                    detected_error,
                    quarantine_digest: Vec::new(),
                    reconciled_record_digest: None,
                    reconciliation_digest: None,
                })
            },
        )
        .optional()
        .map_err(|error| EvidenceError::sqlite("read corrupt evidence", error))
}

fn quarantine_row_by_evidence(
    transaction: &Transaction<'_>,
    id: EvidenceId,
) -> Result<Option<RawEvidence>, EvidenceError> {
    transaction
        .query_row(
            "SELECT quarantine_id, evidence_id, record_digest, global_position, event_id,
                    batch_hash, revision_digest, record_bytes, detected_error,
                    quarantine_digest, reconciled_record_digest, reconciliation_digest
               FROM peritus_evidence_quarantine WHERE evidence_id = ?1
              ORDER BY (reconciled_record_digest IS NULL) DESC, quarantine_id DESC LIMIT 1",
            [id.as_bytes().as_slice()],
            read_quarantine_row,
        )
        .optional()
        .map_err(|error| EvidenceError::sqlite("read evidence quarantine", error))
}

fn quarantine_row_by_id(
    transaction: &Transaction<'_>,
    id: EvidenceQuarantineId,
) -> Result<Option<RawEvidence>, EvidenceError> {
    transaction
        .query_row(
            "SELECT quarantine_id, evidence_id, record_digest, global_position, event_id,
                    batch_hash, revision_digest, record_bytes, detected_error,
                    quarantine_digest, reconciled_record_digest, reconciliation_digest
               FROM peritus_evidence_quarantine WHERE quarantine_id = ?1",
            [id.0.as_bytes().as_slice()],
            read_quarantine_row,
        )
        .optional()
        .map_err(|error| EvidenceError::sqlite("read evidence quarantine by identity", error))
}

fn read_quarantine_row(row: &Row<'_>) -> rusqlite::Result<RawEvidence> {
    Ok(RawEvidence {
        quarantine_id: row.get(0)?,
        evidence_id: row.get(1)?,
        record_digest: row.get(2)?,
        global_position: row.get(3)?,
        event_id: row.get(4)?,
        batch_hash: row.get(5)?,
        revision_digest: row.get(6)?,
        record_bytes: row.get(7)?,
        detected_error: row.get(8)?,
        quarantine_digest: row.get(9)?,
        reconciled_record_digest: row.get(10)?,
        reconciliation_digest: row.get(11)?,
    })
}

struct RawEvidence {
    quarantine_id: Vec<u8>,
    evidence_id: Vec<u8>,
    record_digest: Vec<u8>,
    global_position: i64,
    event_id: Vec<u8>,
    batch_hash: Vec<u8>,
    revision_digest: Vec<u8>,
    record_bytes: Vec<u8>,
    detected_error: String,
    quarantine_digest: Vec<u8>,
    reconciled_record_digest: Option<Vec<u8>>,
    reconciliation_digest: Option<Vec<u8>>,
}

impl RawEvidence {
    fn digest(&self) -> Sha256Digest {
        let mut hash = Sha256::new();
        hash.update(b"peritus-evidence-quarantine-v1\0");
        hash_field(&mut hash, &self.evidence_id);
        hash_field(&mut hash, &self.record_digest);
        hash.update(self.global_position.to_be_bytes());
        hash_field(&mut hash, &self.event_id);
        hash_field(&mut hash, &self.batch_hash);
        hash_field(&mut hash, &self.revision_digest);
        hash_field(&mut hash, &self.record_bytes);
        hash_field(&mut hash, self.detected_error.as_bytes());
        Sha256Digest::new(hash.finalize().into())
    }

    fn audit(&self) -> Result<EvidenceQuarantineAudit, EvidenceError> {
        let quarantine_id = fixed_digest(&self.quarantine_id, "quarantine identity")?;
        let quarantine_digest = fixed_digest(&self.quarantine_digest, "quarantine digest")?;
        if quarantine_id != quarantine_digest || quarantine_digest != self.digest() {
            return Err(corrupt(
                "quarantined evidence bytes disagree with their permanent audit identity",
            ));
        }
        let reconciled_record_digest =
            match (self.reconciled_record_digest.as_deref(), self.reconciliation_digest.as_deref())
            {
                (None, None) => None,
                (Some(record), Some(advertised)) => {
                    let record = fixed_digest(record, "reconciled record digest")?;
                    let advertised = fixed_digest(advertised, "reconciliation digest")?;
                    if reconciliation_digest(quarantine_id, record) != advertised {
                        return Err(corrupt("evidence reconciliation digest is invalid"));
                    }
                    Some(record)
                }
                _ => return Err(corrupt("evidence reconciliation metadata is incomplete")),
            };
        Ok(EvidenceQuarantineAudit {
            quarantine_id: EvidenceQuarantineId(quarantine_id),
            evidence_id: decode_evidence_id(&self.evidence_id).ok(),
            raw_evidence_id_sha256: peritus_codec::sha256(&self.evidence_id),
            indexed_record_digest_sha256: peritus_codec::sha256(&self.record_digest),
            record_bytes_sha256: peritus_codec::sha256(&self.record_bytes),
            record_bytes: u64::try_from(self.record_bytes.len())
                .map_err(|_| corrupt("quarantined evidence size overflowed"))?,
            reconciled_record_digest,
        })
    }

    fn observation(&self) -> Result<EvidenceQuarantine, EvidenceError> {
        let audit = self.audit()?;
        let evidence_id = audit
            .evidence_id
            .ok_or_else(|| corrupt("quarantined evidence identity is malformed"))?;
        Ok(EvidenceQuarantine {
            quarantine_id: audit.quarantine_id,
            evidence_id,
            indexed_record_digest_sha256: audit.indexed_record_digest_sha256,
            record_bytes_sha256: audit.record_bytes_sha256,
            quarantine_digest: audit.quarantine_id.0,
            record_bytes: audit.record_bytes,
            reconciled_record_digest: audit.reconciled_record_digest,
        })
    }

    fn same_active_record(&self, other: &Self) -> bool {
        self.evidence_id == other.evidence_id
            && self.record_digest == other.record_digest
            && self.global_position == other.global_position
            && self.event_id == other.event_id
            && self.batch_hash == other.batch_hash
            && self.revision_digest == other.revision_digest
            && self.record_bytes == other.record_bytes
    }

    fn same_quarantine_copy(&self, other: &Self) -> bool {
        self.same_active_record(other)
            && self.detected_error == other.detected_error
            && self.quarantine_id == other.quarantine_id
            && self.quarantine_digest == other.quarantine_digest
    }
}

fn mark_reconciled(
    transaction: &Transaction<'_>,
    quarantine_id: EvidenceQuarantineId,
    record_digest: Sha256Digest,
) -> Result<(), EvidenceError> {
    let resolution = reconciliation_digest(quarantine_id.0, record_digest);
    let updated = transaction
        .execute(
            "UPDATE peritus_evidence_quarantine
                SET reconciled_record_digest = ?1, reconciliation_digest = ?2
              WHERE quarantine_id = ?3 AND reconciled_record_digest IS NULL",
            params![
                record_digest.as_bytes().as_slice(),
                resolution.as_bytes().as_slice(),
                quarantine_id.0.as_bytes().as_slice(),
            ],
        )
        .map_err(|error| EvidenceError::sqlite("record evidence reconciliation", error))?;
    if updated != 1 {
        return Err(corrupt("quarantine resolution changed during reconciliation"));
    }
    Ok(())
}

fn reconciliation_digest(
    quarantine_digest: Sha256Digest,
    record_digest: Sha256Digest,
) -> Sha256Digest {
    let mut hash = Sha256::new();
    hash.update(b"peritus-evidence-reconciliation-v1\0");
    hash.update(quarantine_digest.as_bytes());
    hash.update(record_digest.as_bytes());
    Sha256Digest::new(hash.finalize().into())
}

pub(super) fn decode_evidence_id(bytes: &[u8]) -> Result<EvidenceId, EvidenceError> {
    EvidenceId::new(bytes.try_into().map_err(|_| corrupt("evidence identity is malformed"))?)
        .map_err(|_| corrupt("evidence identity is reserved"))
}

fn hash_field(hash: &mut Sha256, value: &[u8]) {
    hash.update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_be_bytes());
    hash.update(value);
}

fn fixed_digest(bytes: &[u8], name: &str) -> Result<Sha256Digest, EvidenceError> {
    bytes
        .try_into()
        .map(Sha256Digest::new)
        .map_err(|_| corrupt(&format!("quarantined evidence {name} is malformed")))
}

fn corrupt(detail: &str) -> EvidenceError {
    EvidenceError::new(
        EvidenceErrorKind::CorruptCatalog,
        RecoveryAction::RebuildCatalog,
        "contain evidence corruption",
        detail,
    )
}
