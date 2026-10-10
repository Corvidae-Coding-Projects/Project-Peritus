//! Durable keyset startup progress, read snapshots, and per-record containment transactions.

use super::{
    quarantine::{contain, decode_evidence_id},
    row::load_record_uncontained,
};
use crate::{
    EvidenceCancellation, EvidenceError, EvidenceErrorKind, EvidenceStore, RecoveryAction,
};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use std::num::NonZeroUsize;

/// Durable progress of one exact catalog startup scan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvidenceContainmentProgress {
    scan_id: u64,
    scanned: u64,
    contained: u64,
    complete: bool,
}
impl EvidenceContainmentProgress {
    /// Returns the durable scan generation, retained across interrupted startups.
    #[must_use]
    pub const fn scan_id(self) -> u64 {
        self.scan_id
    }
    /// Returns the number of identities inspected in this scan.
    #[must_use]
    pub const fn scanned(self) -> u64 {
        self.scanned
    }
    /// Returns the number of corruption observations contained in this scan.
    #[must_use]
    pub const fn contained(self) -> u64 {
        self.contained
    }
    /// Reports whether the captured catalog interval was fully inspected.
    #[must_use]
    pub const fn is_complete(self) -> bool {
        self.complete
    }
}

struct Scan {
    progress: EvidenceContainmentProgress,
    cursor: Option<Vec<u8>>,
    upper: Option<Vec<u8>>,
}

impl EvidenceStore {
    pub(super) fn begin_containment(&mut self) -> Result<(), EvidenceError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| EvidenceError::sqlite("begin evidence scan", e))?;
        let existing: Option<(i64, bool)> = transaction
            .query_row(
                "SELECT scan_id, complete FROM peritus_evidence_containment WHERE singleton = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|e| EvidenceError::sqlite("inspect evidence scan", e))?;
        if existing.is_none_or(|(_, complete)| complete) {
            let generation = existing
                .map_or(Some(1), |(value, _)| value.checked_add(1))
                .ok_or_else(|| corrupt("evidence scan generation exhausted"))?;
            let upper: Option<Vec<u8>> = transaction
                .query_row("SELECT MAX(evidence_id) FROM peritus_evidence_records", [], |row| {
                    row.get(0)
                })
                .map_err(|e| EvidenceError::sqlite("capture evidence scan upper bound", e))?;
            transaction.execute("INSERT INTO peritus_evidence_containment(singleton, scan_id, scanned, contained, cursor, upper_bound, complete)
                VALUES(1, ?1, 0, 0, NULL, ?2, 0) ON CONFLICT(singleton) DO UPDATE SET
                scan_id=excluded.scan_id, scanned=0, contained=0, cursor=NULL, upper_bound=excluded.upper_bound, complete=0", params![generation, upper])
                .map_err(|e| EvidenceError::sqlite("start evidence scan", e))?;
        }
        transaction.commit().map_err(|e| EvidenceError::sqlite("commit evidence scan start", e))
    }

    /// Inspects up to `records` identities without holding a catalog-wide writer transaction.
    ///
    /// Healthy records are read and verified in a WAL read snapshot. Each corrupt candidate is
    /// rechecked under its own short write transaction, then the exact cursor advances atomically
    /// with that containment. Cancellation preserves completed identities for the next owner.
    /// Concurrent scan owners compare the durable generation and cursor before advancing it.
    ///
    /// # Errors
    /// Returns cancellation, I/O, corrupt scan state, or retryable `SQLite` contention failures.
    pub fn containment_step(
        &mut self,
        records: NonZeroUsize,
        cancellation: &EvidenceCancellation,
    ) -> Result<EvidenceContainmentProgress, EvidenceError> {
        cancellation.check("advance evidence containment")?;
        let scan = read_scan(&self.connection)?;
        if scan.progress.complete {
            return Ok(scan.progress);
        }
        let limit = i64::try_from(records.get())
            .map_err(|_| corrupt("scan page exceeds SQLite representation"))?;
        let identities = {
            let mut query = self.connection.prepare(
                "SELECT evidence_id FROM peritus_evidence_records WHERE (?1 IS NULL OR evidence_id > ?1)
                 AND evidence_id <= ?2 ORDER BY evidence_id LIMIT ?3")
                .map_err(|e| EvidenceError::sqlite("prepare evidence scan page", e))?;
            query
                .query_map(params![scan.cursor, scan.upper, limit], |row| row.get::<_, Vec<u8>>(0))
                .map_err(|e| EvidenceError::sqlite("read evidence scan page", e))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| EvidenceError::sqlite("decode evidence scan page", e))?
        };
        let mut current = scan;
        for raw_id in &identities {
            cancellation.check("inspect evidence identity")?;
            let suspected = {
                let transaction =
                    Transaction::new_unchecked(&self.connection, TransactionBehavior::Deferred)
                        .map_err(|e| EvidenceError::sqlite("begin evidence scan read", e))?;
                let result = candidate(&transaction, raw_id)?;
                transaction
                    .commit()
                    .map_err(|e| EvidenceError::sqlite("finish evidence scan read", e))?;
                result
            };
            cancellation.check("contain evidence identity")?;
            let transaction = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(|e| EvidenceError::sqlite("begin narrow evidence containment", e))?;
            let contained = if suspected {
                if let Some(error) = corruption(&transaction, raw_id)? {
                    contain(&transaction, raw_id, &error)?;
                    true
                } else {
                    false
                }
            } else {
                false
            };
            let updated = transaction.execute(
                "UPDATE peritus_evidence_containment SET cursor=?1, scanned=scanned+1, contained=contained+?2
                 WHERE singleton=1 AND scan_id=?3 AND cursor IS ?4 AND complete=0",
                params![raw_id, i64::from(contained), super::row::integer(current.progress.scan_id, "scan generation")?, current.cursor])
                .map_err(|e| EvidenceError::sqlite("advance durable evidence scan cursor", e))?;
            if updated != 1 {
                return Err(contended());
            }
            transaction
                .commit()
                .map_err(|e| EvidenceError::sqlite("commit narrow evidence containment", e))?;
            current.cursor = Some(raw_id.clone());
            current.progress.scanned += 1;
            current.progress.contained += u64::from(contained);
        }
        if identities.len() < records.get() {
            let updated = self.connection.execute(
                "UPDATE peritus_evidence_containment SET complete=1 WHERE singleton=1 AND scan_id=?1 AND cursor IS ?2 AND complete=0",
                params![super::row::integer(current.progress.scan_id, "scan generation")?, current.cursor])
                .map_err(|e| EvidenceError::sqlite("complete evidence scan", e))?;
            if updated != 1 {
                return Err(contended());
            }
            current.progress.complete = true;
        }
        Ok(current.progress)
    }

    /// Reads the durable startup scan identity and progress without advancing it.
    ///
    /// # Errors
    /// Returns storage or malformed scan-state failures.
    pub fn containment_progress(&self) -> Result<EvidenceContainmentProgress, EvidenceError> {
        Ok(read_scan(&self.connection)?.progress)
    }
}

fn candidate(transaction: &Transaction<'_>, raw_id: &[u8]) -> Result<bool, EvidenceError> {
    Ok(corruption(transaction, raw_id)?.is_some())
}
fn corruption(
    transaction: &Transaction<'_>,
    raw_id: &[u8],
) -> Result<Option<EvidenceError>, EvidenceError> {
    let quarantined: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM peritus_evidence_quarantine WHERE evidence_id=?1 AND reconciled_record_digest IS NULL)",
        [raw_id], |row| row.get(0)).map_err(|e| EvidenceError::sqlite("inspect containment fence", e))?;
    if quarantined {
        return Ok(None);
    }
    let id = match decode_evidence_id(raw_id) {
        Ok(id) => id,
        Err(error) => return Ok(Some(error)),
    };
    match load_record_uncontained(transaction, id) {
        Ok(_) => Ok(None),
        Err(error) if error.kind() == EvidenceErrorKind::CorruptCatalog => Ok(Some(error)),
        Err(error) => Err(error),
    }
}
fn read_scan(connection: &Connection) -> Result<Scan, EvidenceError> {
    let (id, scanned, contained, complete, cursor, upper) = connection.query_row(
        "SELECT scan_id, scanned, contained, complete, cursor, upper_bound FROM peritus_evidence_containment WHERE singleton=1", [], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?, row.get(3)?, row.get(4)?, row.get(5)?))
        }).map_err(|e| EvidenceError::sqlite("read evidence scan state", e))?;
    Ok(Scan {
        progress: EvidenceContainmentProgress {
            scan_id: u64::try_from(id).map_err(|_| corrupt("invalid scan generation"))?,
            scanned: u64::try_from(scanned).map_err(|_| corrupt("invalid scan count"))?,
            contained: u64::try_from(contained)
                .map_err(|_| corrupt("invalid containment count"))?,
            complete,
        },
        cursor,
        upper,
    })
}
fn contended() -> EvidenceError {
    EvidenceError::new(
        EvidenceErrorKind::Storage,
        RecoveryAction::Retry,
        "advance evidence containment",
        "another owner advanced this exact scan; reload durable progress",
    )
}
fn corrupt(detail: &'static str) -> EvidenceError {
    EvidenceError::new(
        EvidenceErrorKind::CorruptCatalog,
        RecoveryAction::RebuildCatalog,
        "scan evidence containment",
        detail,
    )
}
