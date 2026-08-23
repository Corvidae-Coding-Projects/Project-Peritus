//! Integrity scan and exact export observations.

mod artifacts;
mod scan;
mod validation;

use crate::{AggregateHead, CommittedRecord, StoreId};
use peritus_types::Sha256Digest;

use crate::{JournalError, JournalErrorKind, SqliteJournal};
use rusqlite::TransactionBehavior;

use scan::scan_transaction;

/// Successful complete integrity-scan summary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntegrityReport {
    pub(crate) store_id: StoreId,
    pub(crate) event_count: u64,
    pub(crate) aggregate_count: u64,
    pub(crate) last_position: u64,
    pub(crate) journal_head_digest: Sha256Digest,
}

impl IntegrityReport {
    /// Returns the store identity.
    #[must_use]
    pub const fn store_id(&self) -> StoreId {
        self.store_id
    }

    /// Returns the number of checked immutable events.
    #[must_use]
    pub const fn event_count(&self) -> u64 {
        self.event_count
    }

    /// Returns the number of checked aggregate heads.
    #[must_use]
    pub const fn aggregate_count(&self) -> u64 {
        self.aggregate_count
    }

    /// Returns the final global position, or zero for an empty store.
    #[must_use]
    pub const fn last_position(&self) -> u64 {
        self.last_position
    }

    /// Returns the digest binding the checked canonical head catalog.
    #[must_use]
    pub const fn journal_head_digest(&self) -> Sha256Digest {
        self.journal_head_digest
    }
}

/// Exact checked journal export suitable for deterministic replay or backup tooling.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntegrityExport {
    pub(crate) report: IntegrityReport,
    pub(crate) records: Vec<CommittedRecord>,
    pub(crate) heads: Vec<AggregateHead>,
    pub(crate) artifact_references: Vec<CommittedArtifactReference>,
}

impl IntegrityExport {
    /// Borrows the successful scan summary.
    #[must_use]
    pub const fn report(&self) -> &IntegrityReport {
        &self.report
    }

    /// Borrows exact records in global-position order.
    #[must_use]
    pub fn records(&self) -> &[CommittedRecord] {
        &self.records
    }

    /// Borrows aggregate heads in canonical key order.
    #[must_use]
    pub fn heads(&self) -> &[AggregateHead] {
        &self.heads
    }

    /// Borrows actual journal-owned artifact references in canonical commit/digest order.
    #[must_use]
    pub fn artifact_references(&self) -> &[CommittedArtifactReference] {
        &self.artifact_references
    }
}

/// One digest-verified artifact dependency owned by an exact committed command batch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommittedArtifactReference {
    batch_hash: Sha256Digest,
    first_position: u64,
    last_position: u64,
    artifact_digest: Sha256Digest,
}

impl CommittedArtifactReference {
    /// Returns the committed batch identity that owns this reference.
    #[must_use]
    pub const fn batch_hash(self) -> Sha256Digest {
        self.batch_hash
    }

    /// Returns the first event position in the owning atomic batch.
    #[must_use]
    pub const fn first_position(self) -> u64 {
        self.first_position
    }

    /// Returns the last event position in the owning atomic batch.
    #[must_use]
    pub const fn last_position(self) -> u64 {
        self.last_position
    }

    /// Returns the exact finalized artifact content digest.
    #[must_use]
    pub const fn artifact_digest(self) -> Sha256Digest {
        self.artifact_digest
    }
}

impl SqliteJournal {
    /// Recomputes every exact-frame digest and event hash, validates global and aggregate ordering,
    /// checks command ranges, and compares the stored aggregate-head catalog.
    ///
    /// # Errors
    ///
    /// Returns a terminal corrupt-journal error at the first invalid authoritative value.
    pub fn integrity_scan(&mut self) -> Result<IntegrityReport, JournalError> {
        let store_id = self.store_id;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| JournalError::sqlite("begin integrity scan", error))?;
        let checked = scan_transaction(&transaction, store_id)?;
        transaction
            .commit()
            .map_err(|error| JournalError::sqlite("finish integrity scan", error))?;
        Ok(checked.report)
    }

    /// Produces an exact immutable export only after a complete integrity scan succeeds in the
    /// same read transaction.
    ///
    /// # Errors
    ///
    /// Returns the same storage and terminal integrity failures as [`Self::integrity_scan`].
    pub fn integrity_export(&mut self) -> Result<IntegrityExport, JournalError> {
        let store_id = self.store_id;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| JournalError::sqlite("begin integrity export", error))?;
        let export = scan_transaction(&transaction, store_id)?;
        transaction
            .commit()
            .map_err(|error| JournalError::sqlite("finish integrity export", error))?;
        Ok(export)
    }
}

const fn corrupt(detail: &'static str) -> JournalError {
    JournalError::new(JournalErrorKind::CorruptJournal, "scan journal integrity", detail)
}
