//! Startup containment and audit observations for corrupt durable evidence.

use peritus_types::{EvidenceId, Sha256Digest};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use sha2::{Digest, Sha256};

use super::row::load_record;
use super::store::EvidenceStore;
use crate::{EvidenceError, EvidenceErrorKind, RecoveryAction};

/// Tamper-evident audit identity for one evidence record removed from active use.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvidenceQuarantine {
    evidence_id: EvidenceId,
    indexed_record_digest_sha256: Sha256Digest,
    record_bytes_sha256: Sha256Digest,
    quarantine_digest: Sha256Digest,
    record_bytes: u64,
}

impl EvidenceQuarantine {
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
}

impl EvidenceStore {
    pub(super) fn contain_corrupt_records(&mut self) -> Result<u64, EvidenceError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| EvidenceError::sqlite("begin evidence containment", error))?;
        let identities = active_identities(&transaction)?;
        let mut contained = 0_u64;
        for identity in identities {
            match load_record(&transaction, identity) {
                Ok(Some(_)) => {}
                Ok(None) => {
                    return Err(corrupt("evidence identity disappeared during containment"));
                }
                Err(error) if error.kind() == EvidenceErrorKind::CorruptCatalog => {
                    contain(&transaction, identity, &error)?;
                    contained = contained
                        .checked_add(1)
                        .ok_or_else(|| corrupt("evidence quarantine count overflowed"))?;
                }
                Err(error) => return Err(error),
            }
        }
        transaction
            .commit()
            .map_err(|error| EvidenceError::sqlite("commit evidence containment", error))?;
        Ok(contained)
    }

    /// Returns the verified quarantine audit identity for one evidence record.
    ///
    /// # Errors
    ///
    /// Returns storage or corrupt-catalog failure when the retained quarantine row cannot be
    /// decoded or its digest differs from the copied bytes.
    pub fn quarantined(&self, id: EvidenceId) -> Result<Option<EvidenceQuarantine>, EvidenceError> {
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Deferred)
                .map_err(|error| EvidenceError::sqlite("begin evidence quarantine read", error))?;
        let raw = quarantine_row(&transaction, id)?;
        transaction
            .commit()
            .map_err(|error| EvidenceError::sqlite("finish evidence quarantine read", error))?;
        raw.map(|row| row.observation()).transpose()
    }

    /// Counts evidence identities durably removed from active use.
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

struct RawEvidence {
    evidence_id: Vec<u8>,
    record_digest: Vec<u8>,
    global_position: i64,
    event_id: Vec<u8>,
    batch_hash: Vec<u8>,
    revision_digest: Vec<u8>,
    record_bytes: Vec<u8>,
    detected_error: String,
    quarantine_digest: Vec<u8>,
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

    fn observation(&self) -> Result<EvidenceQuarantine, EvidenceError> {
        let evidence_id = EvidenceId::new(
            self.evidence_id
                .as_slice()
                .try_into()
                .map_err(|_| corrupt("quarantined evidence identity is malformed"))?,
        )
        .map_err(|_| corrupt("quarantined evidence identity is reserved"))?;
        let quarantine_digest = fixed_digest(&self.quarantine_digest, "quarantine digest")?;
        if quarantine_digest != self.digest() {
            return Err(corrupt("quarantined evidence bytes disagree with their audit digest"));
        }
        Ok(EvidenceQuarantine {
            evidence_id,
            indexed_record_digest_sha256: peritus_codec::sha256(&self.record_digest),
            record_bytes_sha256: peritus_codec::sha256(&self.record_bytes),
            quarantine_digest,
            record_bytes: u64::try_from(self.record_bytes.len())
                .map_err(|_| corrupt("quarantined evidence size overflowed"))?,
        })
    }
}

fn active_identities(transaction: &Transaction<'_>) -> Result<Vec<EvidenceId>, EvidenceError> {
    let mut statement = transaction
        .prepare(
            "SELECT evidence_id FROM peritus_evidence_records WHERE evidence_id NOT IN (SELECT evidence_id FROM peritus_evidence_quarantine) ORDER BY evidence_id",
        )
        .map_err(|error| EvidenceError::sqlite("prepare evidence containment scan", error))?;
    statement
        .query_map([], |row| row.get::<_, Vec<u8>>(0))
        .map_err(|error| EvidenceError::sqlite("scan evidence identities", error))?
        .map(|row| {
            let bytes =
                row.map_err(|error| EvidenceError::sqlite("read evidence identity", error))?;
            EvidenceId::new(
                bytes
                    .as_slice()
                    .try_into()
                    .map_err(|_| corrupt("evidence identity is malformed"))?,
            )
            .map_err(|_| corrupt("evidence identity is reserved"))
        })
        .collect()
}

fn contain(
    transaction: &Transaction<'_>,
    id: EvidenceId,
    error: &EvidenceError,
) -> Result<(), EvidenceError> {
    let mut raw = record_row(transaction, id, error.to_string())?
        .ok_or_else(|| corrupt("corrupt evidence row disappeared before containment"))?;
    raw.quarantine_digest = raw.digest().as_bytes().to_vec();
    transaction
        .execute(
            "INSERT INTO peritus_evidence_quarantine(evidence_id, quarantine_digest, record_digest, global_position, event_id, batch_hash, revision_digest, record_bytes, detected_error) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![raw.evidence_id, raw.quarantine_digest, raw.record_digest, raw.global_position,
                raw.event_id, raw.batch_hash, raw.revision_digest, raw.record_bytes,
                raw.detected_error],
        )
        .map_err(|error| EvidenceError::sqlite("quarantine corrupt evidence", error))?;
    Ok(())
}

fn record_row(
    transaction: &Transaction<'_>,
    id: EvidenceId,
    detected_error: String,
) -> Result<Option<RawEvidence>, EvidenceError> {
    transaction
        .query_row(
            "SELECT evidence_id, record_digest, global_position, event_id, batch_hash, revision_digest, record_bytes FROM peritus_evidence_records WHERE evidence_id = ?1",
            [id.as_bytes().as_slice()],
            |row| {
                Ok(RawEvidence {
                    evidence_id: row.get(0)?, record_digest: row.get(1)?,
                    global_position: row.get(2)?, event_id: row.get(3)?, batch_hash: row.get(4)?,
                    revision_digest: row.get(5)?, record_bytes: row.get(6)?, detected_error,
                    quarantine_digest: Vec::new(),
                })
            },
        )
        .optional()
        .map_err(|error| EvidenceError::sqlite("read corrupt evidence", error))
}

fn quarantine_row(
    transaction: &Transaction<'_>,
    id: EvidenceId,
) -> Result<Option<RawEvidence>, EvidenceError> {
    transaction
        .query_row(
            "SELECT evidence_id, record_digest, global_position, event_id, batch_hash, revision_digest, record_bytes, detected_error, quarantine_digest FROM peritus_evidence_quarantine WHERE evidence_id = ?1",
            [id.as_bytes().as_slice()],
            |row| {
                Ok(RawEvidence {
                    evidence_id: row.get(0)?, record_digest: row.get(1)?,
                    global_position: row.get(2)?, event_id: row.get(3)?, batch_hash: row.get(4)?,
                    revision_digest: row.get(5)?, record_bytes: row.get(6)?,
                    detected_error: row.get(7)?, quarantine_digest: row.get(8)?,
                })
            },
        )
        .optional()
        .map_err(|error| EvidenceError::sqlite("read evidence quarantine", error))
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
