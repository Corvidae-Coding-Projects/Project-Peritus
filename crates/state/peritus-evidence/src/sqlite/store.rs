//! Evidence connection policy, atomic admission, reads, and invalidation.

use crate::admission::AdmissionPlan;
use crate::sqlite::row::{digest, integer, journal_observation, load_record};
use crate::{
    EvidenceDraft, EvidenceError, EvidenceErrorKind, EvidenceId, EvidenceInvalidation,
    EvidenceRecord, Freshness, RecoveryAction, evaluate_freshness,
};
use peritus_artifact_store::{ArtifactStore, ReferenceOwner};
use peritus_journal::IntegrityExport;
use peritus_types::{EventId, RevisionTuple};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use std::{collections::BTreeMap, path::Path, time::Duration};

/// Shared `SQLite` connection policy for the evidence adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvidenceStoreOptions {
    busy_timeout: Duration,
}

impl EvidenceStoreOptions {
    /// Creates options with a caller-selected busy timeout.
    #[must_use]
    pub const fn new(busy_timeout: Duration) -> Self {
        Self { busy_timeout }
    }
    /// Returns the configured timeout.
    #[must_use]
    pub const fn busy_timeout(self) -> Duration {
        self.busy_timeout
    }
}

impl Default for EvidenceStoreOptions {
    fn default() -> Self {
        Self { busy_timeout: Duration::from_secs(5) }
    }
}

/// Single-owner durable evidence catalog in a caller-selected shared database.
pub struct EvidenceStore {
    pub(crate) connection: Connection,
}

impl EvidenceStore {
    /// Opens a shared database, verifies journal/artifact dependencies, and installs only
    /// evidence-prefixed tables.
    ///
    /// # Errors
    ///
    /// Returns a storage or missing-dependency error when configuration or schema installation
    /// fails.
    pub fn open(
        path: impl AsRef<Path>,
        options: EvidenceStoreOptions,
    ) -> Result<Self, EvidenceError> {
        let connection = super::connection::open(path.as_ref(), options.busy_timeout())?;
        validate_dependencies(&connection)?;
        connection
            .execute_batch(super::schema::INSTALL)
            .map_err(|error| EvidenceError::sqlite("install evidence schema", error))?;
        let mut store = Self { connection };
        store.contain_corrupt_records()?;
        Ok(store)
    }

    /// Atomically admits immutable evidence and its actual artifact roots.
    ///
    /// Exact retries are idempotent. Reusing an evidence identity or record digest for different
    /// content is a stable conflict.
    ///
    /// # Errors
    ///
    /// Returns typed journal, revision, artifact, causality, identity, or storage failures.
    pub fn admit(
        &mut self,
        draft: EvidenceDraft,
        export: &IntegrityExport,
        artifacts: &ArtifactStore,
    ) -> Result<EvidenceRecord, EvidenceError> {
        for digest in draft.artifacts() {
            artifacts
                .verify(*digest)
                .map_err(|error| EvidenceError::artifact("verify evidence artifact", error))?;
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| EvidenceError::sqlite("begin evidence admission", error))?;
        let existing = load_record(&transaction, draft.id())?;
        let durable = journal_observation(&transaction, draft.journal_position())?;
        let parents = load_parents(&transaction, draft.causes())?;
        let provenance_head_digest = existing.as_ref().map_or_else(
            || export.report().journal_head_digest(),
            |record| record.provenance().journal_head_digest(),
        );
        let plan = AdmissionPlan::build(draft, &durable, export, provenance_head_digest, &parents)?;
        if let Some(existing) = existing {
            if existing == *plan.record() {
                transaction
                    .commit()
                    .map_err(|error| EvidenceError::sqlite("finish evidence retry", error))?;
                return Ok(existing);
            }
            return Err(conflict("evidence identity already names different content"));
        }
        let digest_owner: Option<Vec<u8>> = transaction
            .query_row(
                "SELECT evidence_id FROM peritus_evidence_records WHERE record_digest = ?1",
                [plan.record().record_digest().as_bytes().as_slice()],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| EvidenceError::sqlite("check evidence digest identity", error))?;
        if digest_owner.is_some() {
            return Err(conflict("record digest already belongs to another identity"));
        }
        insert_record(&transaction, &plan)?;
        transaction
            .commit()
            .map_err(|error| EvidenceError::sqlite("commit evidence admission", error))?;
        Ok(plan.record().clone())
    }

    /// Loads and re-verifies one immutable durable record.
    ///
    /// # Errors
    ///
    /// Returns a storage or corrupt-catalog error.
    pub fn load(&self, id: EvidenceId) -> Result<Option<EvidenceRecord>, EvidenceError> {
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Deferred)
                .map_err(|error| EvidenceError::sqlite("begin evidence read", error))?;
        let record = load_record(&transaction, id)?;
        transaction
            .commit()
            .map_err(|error| EvidenceError::sqlite("finish evidence read", error))?;
        Ok(record)
    }

    /// Durably records an explicit later journal invalidation without deleting history.
    ///
    /// # Errors
    ///
    /// Rejects a missing target, non-later or mismatched journal event, and storage failures.
    pub fn invalidate(
        &mut self,
        invalidation: EvidenceInvalidation,
        export: &IntegrityExport,
    ) -> Result<(), EvidenceError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| EvidenceError::sqlite("begin evidence invalidation", error))?;
        let target = load_record(&transaction, invalidation.target())?
            .ok_or_else(|| invalidation_error("target evidence does not exist"))?;
        if !crate::verified::causal_position(
            target.provenance().global_position(),
            invalidation.global_position(),
        ) {
            return Err(invalidation_error("invalidation is not later than target evidence"));
        }
        let durable = journal_observation(&transaction, invalidation.global_position())?;
        let exported = export
            .records()
            .get(
                usize::try_from(invalidation.global_position() - 1)
                    .map_err(|_| invalidation_error("position overflows"))?,
            )
            .ok_or_else(|| invalidation_error("invalidating event is absent from export"))?;
        if durable.event_id != invalidation.event_id()
            || durable.event_hash != invalidation.event_hash()
            || exported.event_id() != invalidation.event_id()
            || exported.event_hash() != invalidation.event_hash()
        {
            return Err(invalidation_error("invalidating event provenance does not match"));
        }
        transaction.execute(
            "INSERT OR IGNORE INTO peritus_evidence_invalidations(target_id, invalidation_digest, global_position, event_id, event_hash, reason_digest) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                invalidation.target().as_bytes().as_slice(), invalidation.invalidation_digest().as_bytes().as_slice(),
                integer(invalidation.global_position(), "invalidation position")?, invalidation.event_id().as_bytes().as_slice(),
                invalidation.event_hash().as_bytes().as_slice(), invalidation.reason_digest().as_bytes().as_slice(),
            ],
        ).map_err(|error| EvidenceError::sqlite("insert evidence invalidation", error))?;
        transaction
            .commit()
            .map_err(|error| EvidenceError::sqlite("commit evidence invalidation", error))
    }

    /// Evaluates durable explicit invalidation and exact revision freshness.
    ///
    /// # Errors
    ///
    /// Returns missing-record, storage, or corrupt-invalidation errors.
    pub fn freshness(
        &self,
        id: EvidenceId,
        current: &RevisionTuple,
    ) -> Result<Freshness, EvidenceError> {
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Deferred)
                .map_err(|error| EvidenceError::sqlite("begin evidence freshness read", error))?;
        let record = load_record(&transaction, id)?.ok_or_else(|| {
            EvidenceError::new(
                EvidenceErrorKind::InvalidInput,
                RecoveryAction::CorrectInput,
                "evaluate durable evidence freshness",
                "evidence record does not exist",
            )
        })?;
        let invalidation = load_invalidation(&transaction, id)?;
        transaction
            .commit()
            .map_err(|error| EvidenceError::sqlite("finish evidence freshness read", error))?;
        Ok(evaluate_freshness(&record, current, invalidation))
    }
}

fn insert_record(transaction: &Transaction<'_>, plan: &AdmissionPlan) -> Result<(), EvidenceError> {
    let record = plan.record();
    let provenance = record.provenance();
    transaction.execute(
        "INSERT INTO peritus_evidence_records(evidence_id, record_digest, global_position, event_id, batch_hash, revision_digest, record_bytes) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![record.id().as_bytes().as_slice(), record.record_digest().as_bytes().as_slice(), integer(provenance.global_position(), "global position")?, provenance.event_id().as_bytes().as_slice(), provenance.batch_hash().as_bytes().as_slice(), provenance.revision_digest().as_bytes().as_slice(), record.canonical_bytes()],
    ).map_err(|error| EvidenceError::sqlite("insert evidence record", error))?;
    for (ordinal, link) in plan.causal_links().iter().enumerate() {
        let ordinal = u64::try_from(ordinal).map_err(|_| {
            EvidenceError::new(
                EvidenceErrorKind::ArithmeticOverflow,
                RecoveryAction::CorrectInput,
                "insert evidence cause",
                "cause ordinal exceeds u64",
            )
        })?;
        transaction.execute(
            "INSERT INTO peritus_evidence_causes(child_id, parent_id, ordinal) VALUES (?1, ?2, ?3)",
            params![link.child().as_bytes().as_slice(), link.parent().as_bytes().as_slice(), integer(ordinal, "cause ordinal")?],
        ).map_err(|error| EvidenceError::sqlite("insert evidence cause", error))?;
    }
    for (ordinal, artifact) in record.artifacts().iter().enumerate() {
        let ordinal = u64::try_from(ordinal).map_err(|_| {
            EvidenceError::new(
                EvidenceErrorKind::ArithmeticOverflow,
                RecoveryAction::CorrectInput,
                "insert evidence artifact",
                "artifact ordinal exceeds u64",
            )
        })?;
        if !peritus_artifact_store::sqlite_interop::is_referenceable(transaction, *artifact)
            .map_err(|error| EvidenceError::sqlite("check evidence artifact", error))?
        {
            return Err(EvidenceError::new(
                EvidenceErrorKind::MissingArtifact,
                RecoveryAction::RepairDependency,
                "insert evidence artifact",
                "artifact is not finalized and active",
            ));
        }
        transaction.execute(
            "INSERT INTO peritus_evidence_artifacts(evidence_id, artifact_digest, ordinal) VALUES (?1, ?2, ?3)",
            params![record.id().as_bytes().as_slice(), artifact.as_bytes().as_slice(), integer(ordinal, "artifact ordinal")?],
        ).map_err(|error| EvidenceError::sqlite("insert evidence artifact", error))?;
        peritus_artifact_store::sqlite_interop::insert_reference(
            transaction,
            ReferenceOwner::evidence(record.record_digest()),
            *artifact,
        )
        .map_err(|error| EvidenceError::sqlite("insert durable evidence artifact root", error))?;
    }
    Ok(())
}

fn load_parents(
    transaction: &Transaction<'_>,
    ids: &[EvidenceId],
) -> Result<BTreeMap<EvidenceId, EvidenceRecord>, EvidenceError> {
    ids.iter()
        .map(|id| {
            load_record(transaction, *id)?.map(|record| (*id, record)).ok_or_else(|| {
                EvidenceError::new(
                    EvidenceErrorKind::InvalidCause,
                    RecoveryAction::CorrectInput,
                    "load evidence causes",
                    "causal parent does not exist",
                )
            })
        })
        .collect()
}

fn load_invalidation(
    transaction: &Transaction<'_>,
    id: EvidenceId,
) -> Result<Option<EvidenceInvalidation>, EvidenceError> {
    type InvalidationRow = (Vec<u8>, i64, Vec<u8>, Vec<u8>, Vec<u8>);
    let raw: Option<InvalidationRow> = transaction.query_row(
        "SELECT invalidation_digest, global_position, event_id, event_hash, reason_digest FROM peritus_evidence_invalidations WHERE target_id = ?1 ORDER BY global_position, invalidation_digest LIMIT 1",
        [id.as_bytes().as_slice()],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
    ).optional().map_err(|error| EvidenceError::sqlite("load evidence invalidation", error))?;
    raw.map(|(advertised, position, event, event_hash, reason)| {
        let position = u64::try_from(position)
            .map_err(|_| invalidation_error("negative invalidation position"))?;
        let event = EventId::new(
            event
                .as_slice()
                .try_into()
                .map_err(|_| invalidation_error("invalid invalidation event id"))?,
        )
        .map_err(|_| invalidation_error("reserved invalidation event id"))?;
        let value = EvidenceInvalidation::new(
            id,
            position,
            event,
            digest(&event_hash, "invalidation event hash")?,
            digest(&reason, "invalidation reason")?,
        )?;
        if value.invalidation_digest() != digest(&advertised, "invalidation digest")? {
            return Err(invalidation_error("invalidation digest mismatch"));
        }
        Ok(value)
    })
    .transpose()
}

fn validate_dependencies(connection: &Connection) -> Result<(), EvidenceError> {
    for table in ["events", "commands", "artifact_records", "artifact_references"] {
        let exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = ?1)",
                [table],
                |row| row.get(0),
            )
            .map_err(|error| EvidenceError::sqlite("inspect evidence dependencies", error))?;
        if !exists {
            return Err(EvidenceError::new(
                EvidenceErrorKind::MissingJournalRecord,
                RecoveryAction::RepairDependency,
                "open evidence database",
                format!("required shared table {table} is absent"),
            ));
        }
    }
    Ok(())
}

fn conflict(detail: &'static str) -> EvidenceError {
    EvidenceError::new(
        EvidenceErrorKind::IdentityConflict,
        RecoveryAction::CorrectInput,
        "admit evidence",
        detail,
    )
}
fn invalidation_error(detail: &'static str) -> EvidenceError {
    EvidenceError::new(
        EvidenceErrorKind::InvalidInput,
        RecoveryAction::CorrectInput,
        "invalidate evidence",
        detail,
    )
}
