//! Read-only quarantine projections and deterministic audit-identity paging.

use super::{
    EvidenceQuarantine, EvidenceQuarantineAudit, EvidenceQuarantineId, EvidenceStore, corrupt,
    fixed_digest, quarantine_row_by_evidence, quarantine_row_by_id,
};
use crate::EvidenceError;
use peritus_types::EvidenceId;
use rusqlite::{Transaction, TransactionBehavior, params};

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
