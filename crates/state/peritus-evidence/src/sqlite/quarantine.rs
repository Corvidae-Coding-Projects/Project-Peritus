//! Bounded startup containment and immutable audit observations for corrupt durable evidence.

use peritus_artifact_store::ReferenceOwner;
use peritus_types::{EvidenceId, Sha256Digest};
use rusqlite::{Connection, OptionalExtension, Row, Transaction, TransactionBehavior, params};
use sha2::{Digest, Sha256};

use super::contention;
use super::row::{integer, load_record, load_record_uncontained};
use super::store::EvidenceStore;
use crate::{EvidenceError, EvidenceErrorKind, EvidenceRecord, RecoveryAction};

const CONTAINMENT_PAGE_IDENTITIES: i64 = 128;

/// Permanent identity of one immutable quarantine observation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EvidenceQuarantineId(Sha256Digest);

impl EvidenceQuarantineId {
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
    pub(super) fn contain_corrupt_records(&mut self) -> Result<u64, EvidenceError> {
        let cancellation = self.cancellation.clone();
        ensure_live(cancellation.as_ref(), "start evidence containment")?;
        contention::run(cancellation.as_ref(), || begin_containment_scan(&mut self.connection))?;
        let mut contained = 0_u64;
        loop {
            ensure_live(cancellation.as_ref(), "scan evidence containment page")?;
            let page = contention::run(cancellation.as_ref(), || {
                contain_page(&mut self.connection, cancellation.as_ref())
            })?;
            contained = contained
                .checked_add(page.contained)
                .ok_or_else(|| corrupt("evidence quarantine count overflowed"))?;
            if page.complete {
                return Ok(contained);
            }
        }
    }

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
        contention::run(self.cancellation.as_ref(), || {
            let transaction =
                Transaction::new_unchecked(&self.connection, TransactionBehavior::Deferred)
                    .map_err(|error| {
                        EvidenceError::sqlite("begin evidence quarantine read", error)
                    })?;
            let raw = quarantine_row_by_evidence(&transaction, id)?;
            transaction.commit().map_err(|error| {
                EvidenceError::sqlite("finish evidence quarantine read", error)
            })?;
            raw.map(|row| row.observation()).transpose()
        })
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
        contention::run(self.cancellation.as_ref(), || {
            let transaction =
                Transaction::new_unchecked(&self.connection, TransactionBehavior::Deferred)
                    .map_err(|error| {
                        EvidenceError::sqlite("begin evidence quarantine audit", error)
                    })?;
            let raw = quarantine_row_by_id(&transaction, id)?;
            transaction.commit().map_err(|error| {
                EvidenceError::sqlite("finish evidence quarantine audit", error)
            })?;
            raw.map(|row| row.audit()).transpose()
        })
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
        contention::run(self.cancellation.as_ref(), || {
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
                .query_map(params![after.as_deref(), i64::from(limit)], |row| {
                    row.get::<_, Vec<u8>>(0)
                })
                .map_err(|error| {
                    EvidenceError::sqlite("query evidence quarantine identities", error)
                })?
                .map(|row| {
                    let bytes = row.map_err(|error| {
                        EvidenceError::sqlite("read evidence quarantine identity", error)
                    })?;
                    Ok(EvidenceQuarantineId(fixed_digest(
                        &bytes,
                        "quarantine identity",
                    )?))
                })
                .collect()
        })
    }

    /// Counts all retained immutable quarantine observations, including reconciled history.
    ///
    /// # Errors
    ///
    /// Returns storage or arithmetic failure when the quarantine catalog cannot be counted.
    pub fn quarantine_count(&self) -> Result<u64, EvidenceError> {
        contention::run(self.cancellation.as_ref(), || {
            let count: i64 = self
                .connection
                .query_row("SELECT COUNT(*) FROM peritus_evidence_quarantine", [], |row| {
                    row.get(0)
                })
                .map_err(|error| EvidenceError::sqlite("count evidence quarantine", error))?;
            u64::try_from(count).map_err(|_| corrupt("evidence quarantine count is negative"))
        })
    }

    /// Revalidates an unchanged active row after its external dependency was repaired.
    ///
    /// The permanent quarantine identity is the authorization fence. Reconciliation is recorded
    /// separately while every copied corrupt byte remains immutable.
    ///
    /// # Errors
    ///
    /// Rejects a mismatched identity, changed active row, unresolved corruption, or tampered audit
    /// metadata.
    pub fn reconcile_quarantined(
        &mut self,
        id: EvidenceId,
        quarantine_id: EvidenceQuarantineId,
    ) -> Result<EvidenceRecord, EvidenceError> {
        let cancellation = self.cancellation.clone();
        contention::run(cancellation.as_ref(), || {
            ensure_live(cancellation.as_ref(), "reconcile quarantined evidence")?;
            let transaction = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(|error| {
                    EvidenceError::sqlite("begin evidence reconciliation", error)
                })?;
            let raw = quarantine_row_by_id(&transaction, quarantine_id)?
                .ok_or_else(|| reconciliation_error("quarantine identity does not exist"))?;
            let audit = raw.audit()?;
            if audit.evidence_id != Some(id) {
                return Err(reconciliation_error(
                    "quarantine identity does not own the selected evidence identity",
                ));
            }
            if let Some(reconciled) = audit.reconciled_record_digest {
                let record = load_record_uncontained(&transaction, id)?
                    .ok_or_else(|| corrupt("reconciled evidence row is missing"))?;
                if record.record_digest() != reconciled {
                    return Err(corrupt(
                        "reconciled evidence row disagrees with its resolution digest",
                    ));
                }
                transaction.commit().map_err(|error| {
                    EvidenceError::sqlite("finish idempotent evidence reconciliation", error)
                })?;
                return Ok(record);
            }
            let active = record_row_by_raw(&transaction, id.as_bytes(), String::new())?
                .ok_or_else(|| reconciliation_error("active evidence row does not exist"))?;
            if !raw.same_active_record(&active) {
                return Err(reconciliation_error(
                    "active evidence row changed after its quarantine copy was made",
                ));
            }
            let record = load_record_uncontained(&transaction, id)?
                .ok_or_else(|| reconciliation_error("active evidence row does not exist"))?;
            mark_reconciled(&transaction, quarantine_id, record.record_digest())?;
            ensure_live(cancellation.as_ref(), "reconcile quarantined evidence")?;
            transaction.commit().map_err(|error| {
                EvidenceError::sqlite("commit evidence reconciliation", error)
            })?;
            Ok(record)
        })
    }

    /// Rebuilds one quarantined active row from canonical bytes under its original identity.
    ///
    /// The replacement must match the originally indexed record digest and provenance. Causes,
    /// artifact links, and evidence-owned artifact roots are reconstructed from those canonical
    /// bytes in the same transaction, then fully revalidated before reconciliation is recorded.
    ///
    /// # Errors
    ///
    /// Rejects a mismatched quarantine identity, replacement identity, digest, provenance,
    /// dependency, artifact root, or canonical encoding.
    pub fn rebuild_quarantined(
        &mut self,
        id: EvidenceId,
        quarantine_id: EvidenceQuarantineId,
        repaired_record_bytes: &[u8],
    ) -> Result<EvidenceRecord, EvidenceError> {
        let replacement = EvidenceRecord::verify_portable(repaired_record_bytes).map_err(|_| {
            reconciliation_error("replacement evidence bytes are not a canonical record")
        })?;
        if replacement.canonical_bytes() != repaired_record_bytes || replacement.id() != id {
            return Err(reconciliation_error(
                "replacement bytes do not preserve the selected evidence identity",
            ));
        }
        let cancellation = self.cancellation.clone();
        contention::run(cancellation.as_ref(), || {
            ensure_live(cancellation.as_ref(), "rebuild quarantined evidence")?;
            let transaction = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(|error| EvidenceError::sqlite("begin evidence rebuild", error))?;
            let raw = quarantine_row_by_id(&transaction, quarantine_id)?
                .ok_or_else(|| reconciliation_error("quarantine identity does not exist"))?;
            let audit = raw.audit()?;
            if audit.evidence_id != Some(id) {
                return Err(reconciliation_error(
                    "quarantine identity does not own the selected evidence identity",
                ));
            }
            validate_replacement_binding(&raw, &replacement)?;
            if let Some(reconciled) = audit.reconciled_record_digest {
                let record = load_record_uncontained(&transaction, id)?
                    .ok_or_else(|| corrupt("reconciled evidence row is missing"))?;
                if record.record_digest() != reconciled
                    || record.canonical_bytes() != repaired_record_bytes
                {
                    return Err(reconciliation_error(
                        "completed rebuild does not match this exact replacement",
                    ));
                }
                transaction.commit().map_err(|error| {
                    EvidenceError::sqlite("finish idempotent evidence rebuild", error)
                })?;
                return Ok(record);
            }
            let active = record_row_by_raw(&transaction, id.as_bytes(), String::new())?
                .ok_or_else(|| reconciliation_error("active evidence row does not exist"))?;
            if !raw.same_active_record(&active) {
                return Err(reconciliation_error(
                    "active evidence row changed after its quarantine copy was made",
                ));
            }
            rebuild_record(
                &transaction,
                &replacement,
                repaired_record_bytes,
                cancellation.as_ref(),
            )?;
            let record = load_record_uncontained(&transaction, id)?
                .ok_or_else(|| corrupt("rebuilt evidence row disappeared"))?;
            mark_reconciled(&transaction, quarantine_id, record.record_digest())?;
            ensure_live(cancellation.as_ref(), "rebuild quarantined evidence")?;
            transaction
                .commit()
                .map_err(|error| EvidenceError::sqlite("commit evidence rebuild", error))?;
            Ok(record)
        })
    }
}

struct ContainmentPage {
    contained: u64,
    complete: bool,
}

fn begin_containment_scan(connection: &mut Connection) -> Result<(), EvidenceError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| EvidenceError::sqlite("begin evidence containment pass", error))?;
    let complete: Option<i64> = transaction
        .query_row(
            "SELECT complete FROM peritus_evidence_containment WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| EvidenceError::sqlite("read evidence containment progress", error))?;
    if complete != Some(0) {
        let upper_bound: Option<Vec<u8>> = transaction
            .query_row("SELECT MAX(evidence_id) FROM peritus_evidence_records", [], |row| {
                row.get(0)
            })
            .map_err(|error| EvidenceError::sqlite("bound evidence containment pass", error))?;
        transaction
            .execute(
                "INSERT INTO peritus_evidence_containment(singleton, cursor, upper_bound, complete)
                 VALUES (1, NULL, ?1, 0)
                 ON CONFLICT(singleton) DO UPDATE SET
                    cursor = NULL, upper_bound = excluded.upper_bound, complete = 0",
                params![upper_bound],
            )
            .map_err(|error| EvidenceError::sqlite("start evidence containment pass", error))?;
    }
    transaction
        .commit()
        .map_err(|error| EvidenceError::sqlite("commit evidence containment pass", error))
}

fn contain_page(
    connection: &mut Connection,
    cancellation: Option<&peritus_journal::JournalCancellation>,
) -> Result<ContainmentPage, EvidenceError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| EvidenceError::sqlite("begin evidence containment page", error))?;
    let state: Option<(Option<Vec<u8>>, Option<Vec<u8>>, i64)> = transaction
        .query_row(
            "SELECT cursor, upper_bound, complete
               FROM peritus_evidence_containment WHERE singleton = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|error| EvidenceError::sqlite("read evidence containment page", error))?;
    let (cursor, Some(upper_bound), complete) = state
        .ok_or_else(|| corrupt("evidence containment progress is missing"))?
    else {
        transaction
            .execute(
                "UPDATE peritus_evidence_containment
                    SET cursor = NULL, upper_bound = NULL, complete = 1
                  WHERE singleton = 1",
                [],
            )
            .map_err(|error| EvidenceError::sqlite("complete empty evidence containment", error))?;
        transaction.commit().map_err(|error| {
            EvidenceError::sqlite("commit empty evidence containment", error)
        })?;
        return Ok(ContainmentPage { contained: 0, complete: true });
    };
    if complete != 0 {
        transaction.commit().map_err(|error| {
            EvidenceError::sqlite("finish completed evidence containment", error)
        })?;
        return Ok(ContainmentPage { contained: 0, complete: true });
    }
    let identities = active_identities(&transaction, cursor.as_deref(), &upper_bound)?;
    let mut contained = 0_u64;
    for raw_identity in &identities {
        ensure_live(cancellation, "scan evidence containment page")?;
        let identity = decode_evidence_id(raw_identity);
        match identity {
            Ok(identity) => match load_record_uncontained(&transaction, identity) {
                Ok(Some(_)) => {}
                Ok(None) => {
                    return Err(corrupt("evidence identity disappeared during containment"));
                }
                Err(error) if error.kind() == EvidenceErrorKind::CorruptCatalog => {
                    contain(&transaction, raw_identity, &error)?;
                    contained = contained
                        .checked_add(1)
                        .ok_or_else(|| corrupt("evidence quarantine count overflowed"))?;
                }
                Err(error) => return Err(error),
            },
            Err(error) => {
                contain(&transaction, raw_identity, &error)?;
                contained = contained
                    .checked_add(1)
                    .ok_or_else(|| corrupt("evidence quarantine count overflowed"))?;
            }
        }
    }
    let page_complete = identities.len()
        < usize::try_from(CONTAINMENT_PAGE_IDENTITIES)
            .map_err(|_| corrupt("containment page size is not representable"))?;
    if page_complete {
        transaction
            .execute(
                "UPDATE peritus_evidence_containment
                    SET cursor = NULL, upper_bound = NULL, complete = 1
                  WHERE singleton = 1",
                [],
            )
            .map_err(|error| EvidenceError::sqlite("complete evidence containment", error))?;
    } else {
        let last = identities
            .last()
            .ok_or_else(|| corrupt("nonempty containment page lost its cursor"))?;
        transaction
            .execute(
                "UPDATE peritus_evidence_containment SET cursor = ?1 WHERE singleton = 1",
                [last.as_slice()],
            )
            .map_err(|error| EvidenceError::sqlite("advance evidence containment cursor", error))?;
    }
    ensure_live(cancellation, "commit evidence containment page")?;
    transaction
        .commit()
        .map_err(|error| EvidenceError::sqlite("commit evidence containment page", error))?;
    Ok(ContainmentPage { contained, complete: page_complete })
}

fn active_identities(
    transaction: &Transaction<'_>,
    cursor: Option<&[u8]>,
    upper_bound: &[u8],
) -> Result<Vec<Vec<u8>>, EvidenceError> {
    let mut statement = transaction
        .prepare(
            "SELECT record.evidence_id
               FROM peritus_evidence_records AS record
              WHERE (?1 IS NULL OR record.evidence_id > ?1)
                AND record.evidence_id <= ?2
                AND NOT EXISTS (
                    SELECT 1 FROM peritus_evidence_quarantine AS quarantine
                     WHERE quarantine.evidence_id = record.evidence_id
                       AND quarantine.reconciled_record_digest IS NULL
                )
              ORDER BY record.evidence_id LIMIT ?3",
        )
        .map_err(|error| EvidenceError::sqlite("prepare evidence containment scan", error))?;
    statement
        .query_map(
            params![cursor, upper_bound, CONTAINMENT_PAGE_IDENTITIES],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .map_err(|error| EvidenceError::sqlite("scan evidence identities", error))?
        .map(|row| row.map_err(|error| EvidenceError::sqlite("read evidence identity", error)))
        .collect()
}

fn contain(
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
        let reconciled_record_digest = match (
            self.reconciled_record_digest.as_deref(),
            self.reconciliation_digest.as_deref(),
        ) {
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

fn validate_replacement_binding(
    raw: &RawEvidence,
    replacement: &EvidenceRecord,
) -> Result<(), EvidenceError> {
    let indexed_digest = fixed_digest(&raw.record_digest, "indexed record digest")?;
    let provenance = replacement.provenance();
    if replacement.record_digest() != indexed_digest
        || u64::try_from(raw.global_position).ok() != Some(provenance.global_position())
        || raw.event_id.as_slice() != provenance.event_id().as_bytes()
        || raw.batch_hash.as_slice() != provenance.batch_hash().as_bytes()
        || raw.revision_digest.as_slice() != provenance.revision_digest().as_bytes()
    {
        return Err(reconciliation_error(
            "replacement does not preserve the original digest and indexed provenance",
        ));
    }
    Ok(())
}

fn rebuild_record(
    transaction: &Transaction<'_>,
    record: &EvidenceRecord,
    record_bytes: &[u8],
    cancellation: Option<&peritus_journal::JournalCancellation>,
) -> Result<(), EvidenceError> {
    transaction
        .execute(
            "DELETE FROM peritus_evidence_causes WHERE child_id = ?1",
            [record.id().as_bytes().as_slice()],
        )
        .map_err(|error| EvidenceError::sqlite("clear quarantined evidence causes", error))?;
    transaction
        .execute(
            "DELETE FROM peritus_evidence_artifacts WHERE evidence_id = ?1",
            [record.id().as_bytes().as_slice()],
        )
        .map_err(|error| EvidenceError::sqlite("clear quarantined evidence artifacts", error))?;
    transaction
        .execute(
            "DELETE FROM artifact_references WHERE owner_kind = 2 AND owner_identity = ?1",
            [record.record_digest().as_bytes().as_slice()],
        )
        .map_err(|error| EvidenceError::sqlite("clear quarantined evidence roots", error))?;
    let updated = transaction
        .execute(
            "UPDATE peritus_evidence_records SET record_bytes = ?1 WHERE evidence_id = ?2",
            params![record_bytes, record.id().as_bytes().as_slice()],
        )
        .map_err(|error| EvidenceError::sqlite("replace quarantined evidence bytes", error))?;
    if updated != 1 {
        return Err(reconciliation_error("active evidence row does not exist"));
    }
    for (ordinal, parent) in record.causes().iter().enumerate() {
        ensure_live(cancellation, "rebuild quarantined evidence causes")?;
        let parent_record = load_record(transaction, *parent)?
            .ok_or_else(|| dependency_error("causal parent does not exist"))?;
        if !crate::verified::causal_position(
            parent_record.provenance().global_position(),
            record.provenance().global_position(),
        ) {
            return Err(dependency_error("causal parent is not older than rebuilt evidence"));
        }
        let ordinal = u64::try_from(ordinal)
            .map_err(|_| reconciliation_error("cause ordinal exceeds u64"))?;
        transaction
            .execute(
                "INSERT INTO peritus_evidence_causes(child_id, parent_id, ordinal)
                 VALUES (?1, ?2, ?3)",
                params![
                    record.id().as_bytes().as_slice(),
                    parent.as_bytes().as_slice(),
                    integer(ordinal, "rebuild cause ordinal")?,
                ],
            )
            .map_err(|error| EvidenceError::sqlite("rebuild evidence cause", error))?;
    }
    for (ordinal, artifact) in record.artifacts().iter().enumerate() {
        ensure_live(cancellation, "rebuild quarantined evidence artifacts")?;
        if !peritus_artifact_store::sqlite_interop::is_referenceable(transaction, *artifact)
            .map_err(|error| EvidenceError::sqlite("check rebuilt evidence artifact", error))?
        {
            return Err(artifact_error("rebuilt evidence artifact is not referenceable"));
        }
        let ordinal = u64::try_from(ordinal)
            .map_err(|_| reconciliation_error("artifact ordinal exceeds u64"))?;
        transaction
            .execute(
                "INSERT INTO peritus_evidence_artifacts(evidence_id, artifact_digest, ordinal)
                 VALUES (?1, ?2, ?3)",
                params![
                    record.id().as_bytes().as_slice(),
                    artifact.as_bytes().as_slice(),
                    integer(ordinal, "rebuild artifact ordinal")?,
                ],
            )
            .map_err(|error| EvidenceError::sqlite("rebuild evidence artifact", error))?;
        let rooted = peritus_artifact_store::sqlite_interop::insert_reference(
            transaction,
            ReferenceOwner::evidence(record.record_digest()),
            *artifact,
        )
        .map_err(|error| EvidenceError::sqlite("rebuild evidence artifact roots", error))?;
        if !rooted {
            return Err(artifact_error(
                "rebuilt evidence artifact closure is not referenceable",
            ));
        }
    }
    Ok(())
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
        return Err(reconciliation_error(
            "quarantine resolution changed during reconciliation",
        ));
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

fn decode_evidence_id(bytes: &[u8]) -> Result<EvidenceId, EvidenceError> {
    EvidenceId::new(
        bytes
            .try_into()
            .map_err(|_| corrupt("evidence identity is malformed"))?,
    )
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

fn ensure_live(
    cancellation: Option<&peritus_journal::JournalCancellation>,
    operation: &'static str,
) -> Result<(), EvidenceError> {
    if cancellation.is_some_and(peritus_journal::JournalCancellation::is_cancelled) {
        Err(EvidenceError::cancelled(operation))
    } else {
        Ok(())
    }
}

fn reconciliation_error(detail: &'static str) -> EvidenceError {
    EvidenceError::new(
        EvidenceErrorKind::InvalidInput,
        RecoveryAction::CorrectInput,
        "reconcile quarantined evidence",
        detail,
    )
}

fn dependency_error(detail: &'static str) -> EvidenceError {
    EvidenceError::new(
        EvidenceErrorKind::InvalidCause,
        RecoveryAction::RepairDependency,
        "rebuild quarantined evidence",
        detail,
    )
}

fn artifact_error(detail: &'static str) -> EvidenceError {
    EvidenceError::new(
        EvidenceErrorKind::MissingArtifact,
        RecoveryAction::RepairDependency,
        "rebuild quarantined evidence",
        detail,
    )
}

fn corrupt(detail: &str) -> EvidenceError {
    EvidenceError::new(
        EvidenceErrorKind::CorruptCatalog,
        RecoveryAction::RebuildCatalog,
        "contain evidence corruption",
        detail,
    )
}
