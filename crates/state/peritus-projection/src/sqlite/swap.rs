//! Transactional shadow-generation install and active-pointer swap.

use super::ProjectionStore;
use super::contention;
use super::store::{stored_positive_u64, u64_to_i64};
use crate::{
    CatalogGeneration, ProjectionError, ProjectionErrorKind, ProjectionSchema, RebuildCandidate,
    RecoveryClass,
};
use peritus_codec::sha256;
use peritus_journal::StoreId;
use peritus_types::Sha256Digest;
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};

const JOURNAL_HEAD_DOMAIN: &[u8] = b"peritus.journal.head.v1\0";

impl ProjectionStore {
    pub(crate) fn confirm_current(
        &mut self,
        schema: &ProjectionSchema,
        expected_active: CatalogGeneration,
        owner_store_id: StoreId,
        last_position: u64,
        journal_head_digest: Sha256Digest,
    ) -> Result<CatalogGeneration, ProjectionError> {
        let cancellation = self.cancellation.clone();
        contention::run(cancellation.as_ref(), || {
            let transaction = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(|error| ProjectionError::sqlite("begin current projection check", error))?;
            verify_authoritative_journal(
                &transaction,
                owner_store_id,
                last_position,
                journal_head_digest,
            )?;
            let identity = schema.identity();
            let version = u64_to_i64(identity.version().get(), "projection version")?;
            let current = active_generation(&transaction, identity.name().as_str(), version)?;
            if current != Some(expected_active.get()) {
                return Err(ProjectionError::new(
                    ProjectionErrorKind::Conflict,
                    RecoveryClass::Retry,
                    "confirm active projection",
                    "active generation changed before the authoritative journal check",
                ));
            }
            if !generation_is_current(
                &transaction,
                identity.name().as_str(),
                version,
                expected_active.get(),
                schema,
                last_position,
                journal_head_digest,
            )? {
                return Err(ProjectionError::new(
                    ProjectionErrorKind::StaleCheckpoint,
                    RecoveryClass::Rebuild,
                    "confirm active projection",
                    "active generation changed after its checked startup observation",
                ));
            }
            backfill_receipts(&transaction, owner_store_id)?;
            ensure_receipt_owner(
                &transaction,
                identity.name().as_str(),
                version,
                expected_active.get(),
                owner_store_id,
            )?;
            initialize_recovery_root(
                &transaction,
                identity.name().as_str(),
                version,
                expected_active.get(),
                owner_store_id,
            )?;
            delete_progress(&transaction, identity.name().as_str(), version)?;
            reclaim_unreferenced(
                &transaction,
                identity.name().as_str(),
                version,
                owner_store_id,
            )?;
            transaction
                .commit()
                .map_err(|error| ProjectionError::sqlite("finish current projection check", error))?;
            Ok(expected_active)
        })
    }

    /// Installs a checked shadow generation and atomically advances the active pointer.
    ///
    /// `expected_active` is an explicit compare-and-swap expectation. Passing `None` requires no
    /// active generation to exist. An identical rebuild at the current journal/schema binding is
    /// reused, while a differing checksum at that same binding is rejected as nondeterminism.
    ///
    /// # Errors
    ///
    /// Returns conflict, deterministic checksum, bound, or `SQLite` failures. A failure leaves both
    /// the generation catalog and active pointer unchanged.
    pub fn install_shadow<S>(
        &mut self,
        candidate: &RebuildCandidate<S>,
        expected_active: Option<CatalogGeneration>,
    ) -> Result<CatalogGeneration, ProjectionError> {
        let encoded = validate_candidate(candidate)?;
        let cancellation = self.cancellation.clone();
        contention::run(cancellation.as_ref(), || {
            let transaction = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(|error| ProjectionError::sqlite("begin projection swap", error))?;
            let identity = candidate.checkpoint().schema().identity();
            let version = encoded.projection_version;
            verify_authoritative_journal(
                &transaction,
                candidate.owner_store_id(),
                candidate.checkpoint().last_position(),
                candidate.checkpoint().journal_head_digest(),
            )?;
            backfill_receipts(&transaction, candidate.owner_store_id())?;
            let current = active_generation(&transaction, identity.name().as_str(), version)?;
            if let Some(current) = current
                && existing_is_same_binding(
                    &transaction,
                    identity.name().as_str(),
                    version,
                    current,
                    candidate,
                    encoded,
                )?
            {
                ensure_frontier(
                    &transaction,
                    identity.name().as_str(),
                    version,
                    current,
                    candidate,
                )?;
                ensure_receipt_owner(
                    &transaction,
                    identity.name().as_str(),
                    version,
                    current,
                    candidate.owner_store_id(),
                )?;
                initialize_recovery_root(
                    &transaction,
                    identity.name().as_str(),
                    version,
                    current,
                    candidate.owner_store_id(),
                )?;
                delete_progress(&transaction, identity.name().as_str(), version)?;
                reclaim_unreferenced(
                    &transaction,
                    identity.name().as_str(),
                    version,
                    candidate.owner_store_id(),
                )?;
                transaction.commit().map_err(|error| {
                    ProjectionError::sqlite("finish identical projection rebuild", error)
                })?;
                return CatalogGeneration::from_u64(current);
            }
            if current != expected_active.map(CatalogGeneration::get) {
                return Err(ProjectionError::new(
                    ProjectionErrorKind::Conflict,
                    RecoveryClass::Retry,
                    "swap active projection",
                    "active generation changed before compare-and-swap",
                ));
            }
            let highest = highest_generation(&transaction, identity.name().as_str(), version)?;
            let next = crate::verified::next_generation(highest).ok_or_else(|| {
                ProjectionError::new(
                    ProjectionErrorKind::InvalidInput,
                    RecoveryClass::CorrectInput,
                    "allocate projection generation",
                    "generation number exhausted",
                )
            })?;
            let next_sql = u64_to_i64(next, "generation")?;
            insert_generation(&transaction, next_sql, candidate, encoded)?;
            insert_receipt(&transaction, next_sql, candidate, encoded)?;
            transaction
                .execute(
                    "INSERT INTO peritus_projection_catalog(projection_name, projection_version, active_generation) VALUES (?1, ?2, ?3) ON CONFLICT(projection_name, projection_version) DO UPDATE SET active_generation = excluded.active_generation",
                    params![identity.name().as_str(), version, next_sql],
                )
                .map_err(|error| ProjectionError::sqlite("activate projection generation", error))?;
            if let Some(previous) = current {
                set_recovery_root(
                    &transaction,
                    identity.name().as_str(),
                    version,
                    previous,
                    candidate.owner_store_id(),
                )?;
            }
            delete_progress(&transaction, identity.name().as_str(), version)?;
            reclaim_unreferenced(
                &transaction,
                identity.name().as_str(),
                version,
                candidate.owner_store_id(),
            )?;
            transaction
                .commit()
                .map_err(|error| ProjectionError::sqlite("commit projection swap", error))?;
            CatalogGeneration::from_u64(next)
        })
    }
}

#[derive(Clone, Copy)]
struct EncodedCandidate {
    projection_version: i64,
    last_position: i64,
    record_count: i64,
}

fn validate_candidate<S>(
    candidate: &RebuildCandidate<S>,
) -> Result<EncodedCandidate, ProjectionError> {
    let identity = candidate.checkpoint().schema().identity();
    let encoded = EncodedCandidate {
        projection_version: u64_to_i64(identity.version().get(), "projection version")?,
        last_position: u64_to_i64(candidate.checkpoint().last_position(), "last position")?,
        record_count: u64_to_i64(candidate.record_count(), "record count")?,
    };
    if sha256(candidate.payload()) != candidate.checkpoint().payload_digest()
        || sha256(candidate.frontier_payload()) != candidate.frontier_digest()
        || candidate.record_count() != candidate.checkpoint().last_position()
    {
        return Err(ProjectionError::new(
            ProjectionErrorKind::FoldInvariant,
            RecoveryClass::Rebuild,
            "validate shadow projection",
            "candidate payload or record count does not match checkpoint",
        ));
    }
    Ok(encoded)
}

fn active_generation(
    transaction: &Transaction<'_>,
    name: &str,
    version: i64,
) -> Result<Option<u64>, ProjectionError> {
    let value: Option<i64> = transaction
        .query_row(
            "SELECT active_generation FROM peritus_projection_catalog WHERE projection_name = ?1 AND projection_version = ?2",
            params![name, version],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| ProjectionError::sqlite("read active projection generation", error))?;
    value
        .map(|value| stored_positive_u64(value, "active generation"))
        .transpose()
}

fn highest_generation(
    transaction: &Transaction<'_>,
    name: &str,
    version: i64,
) -> Result<Option<u64>, ProjectionError> {
    let value: Option<i64> = transaction
        .query_row(
            "SELECT MAX(generation) FROM (SELECT generation FROM peritus_projection_generations WHERE projection_name = ?1 AND projection_version = ?2 UNION ALL SELECT generation FROM peritus_projection_receipts WHERE projection_name = ?1 AND projection_version = ?2)",
            params![name, version],
            |row| row.get(0),
        )
        .map_err(|error| ProjectionError::sqlite("read highest projection generation", error))?;
    value
        .map(|value| stored_positive_u64(value, "highest generation"))
        .transpose()
}

fn generation_is_current(
    transaction: &Transaction<'_>,
    name: &str,
    version: i64,
    generation: u64,
    schema: &ProjectionSchema,
    last_position: u64,
    journal_head_digest: Sha256Digest,
) -> Result<bool, ProjectionError> {
    let stored: (i64, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>) = transaction
        .query_row(
            "SELECT last_position, journal_head_digest, schema_digest, payload_digest, payload FROM peritus_projection_generations WHERE projection_name = ?1 AND projection_version = ?2 AND generation = ?3",
            params![name, version, u64_to_i64(generation, "generation")?],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .map_err(|error| ProjectionError::sqlite("confirm active projection payload", error))?;
    Ok(stored.0 == u64_to_i64(last_position, "last position")?
        && stored.1.as_slice() == journal_head_digest.as_bytes()
        && stored.2.as_slice() == schema.digest().as_bytes()
        && stored.3.as_slice() == sha256(&stored.4).as_bytes())
}

fn existing_is_same_binding<S>(
    transaction: &Transaction<'_>,
    name: &str,
    version: i64,
    generation: u64,
    candidate: &RebuildCandidate<S>,
    encoded: EncodedCandidate,
) -> Result<bool, ProjectionError> {
    let stored = transaction
        .query_row(
            "SELECT last_position, journal_head_digest, schema_digest, payload_digest, invariant_digest, record_count, payload FROM peritus_projection_generations WHERE projection_name = ?1 AND projection_version = ?2 AND generation = ?3",
            params![name, version, u64_to_i64(generation, "generation")?],
            |row| {
                Ok(StoredGeneration {
                    last_position: row.get(0)?,
                    journal_head: row.get(1)?,
                    schema_digest: row.get(2)?,
                    payload_digest: row.get(3)?,
                    invariant_digest: row.get(4)?,
                    record_count: row.get(5)?,
                    payload: row.get(6)?,
                })
            },
        )
        .map_err(|error| ProjectionError::sqlite("compare active projection", error))?;
    let checkpoint = candidate.checkpoint();
    let same_binding = stored.last_position == encoded.last_position
        && stored.record_count == encoded.record_count
        && stored.journal_head.as_slice() == checkpoint.journal_head_digest().as_bytes()
        && stored.schema_digest.as_slice() == checkpoint.schema().digest().as_bytes();
    if !same_binding {
        return Ok(false);
    }
    if sha256(&stored.payload).as_bytes().as_slice() != stored.payload_digest.as_slice() {
        return Ok(false);
    }
    if stored.payload_digest.as_slice() != checkpoint.payload_digest().as_bytes()
        || stored.invariant_digest.as_slice() != candidate.invariant_digest().as_bytes()
    {
        return Err(ProjectionError::new(
            ProjectionErrorKind::FoldInvariant,
            RecoveryClass::Rebuild,
            "compare projection checksums",
            "same journal and schema binding produced different deterministic checksums",
        ));
    }
    Ok(true)
}

struct StoredGeneration {
    last_position: i64,
    journal_head: Vec<u8>,
    schema_digest: Vec<u8>,
    payload_digest: Vec<u8>,
    invariant_digest: Vec<u8>,
    record_count: i64,
    payload: Vec<u8>,
}

fn insert_generation<S>(
    transaction: &Transaction<'_>,
    generation: i64,
    candidate: &RebuildCandidate<S>,
    encoded: EncodedCandidate,
) -> Result<(), ProjectionError> {
    let checkpoint = candidate.checkpoint();
    let identity = checkpoint.schema().identity();
    transaction
        .execute(
            "INSERT INTO peritus_projection_generations(projection_name, projection_version, generation, last_position, journal_head_digest, payload_digest, schema_digest, invariant_digest, record_count, payload) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                identity.name().as_str(),
                encoded.projection_version,
                generation,
                encoded.last_position,
                checkpoint.journal_head_digest().as_bytes().as_slice(),
                checkpoint.payload_digest().as_bytes().as_slice(),
                checkpoint.schema().digest().as_bytes().as_slice(),
                candidate.invariant_digest().as_bytes().as_slice(),
                encoded.record_count,
                candidate.payload(),
            ],
        )
        .map_err(|error| ProjectionError::sqlite("insert shadow projection", error))?;
    transaction
        .execute(
            "INSERT INTO peritus_projection_frontiers(projection_name, projection_version, generation, frontier_digest, frontier) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                identity.name().as_str(),
                encoded.projection_version,
                generation,
                candidate.frontier_digest().as_bytes().as_slice(),
                candidate.frontier_payload(),
            ],
        )
        .map_err(|error| ProjectionError::sqlite("insert projection frontier", error))?;
    Ok(())
}

fn insert_receipt<S>(
    transaction: &Transaction<'_>,
    generation: i64,
    candidate: &RebuildCandidate<S>,
    encoded: EncodedCandidate,
) -> Result<(), ProjectionError> {
    let checkpoint = candidate.checkpoint();
    let identity = checkpoint.schema().identity();
    transaction
        .execute(
            "INSERT INTO peritus_projection_receipts(projection_name, projection_version, generation, owner_store_id, last_position, journal_head_digest, payload_digest, schema_digest, invariant_digest, frontier_digest, record_count) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                identity.name().as_str(),
                encoded.projection_version,
                generation,
                candidate.owner_store_id().as_bytes().as_slice(),
                encoded.last_position,
                checkpoint.journal_head_digest().as_bytes().as_slice(),
                checkpoint.payload_digest().as_bytes().as_slice(),
                checkpoint.schema().digest().as_bytes().as_slice(),
                candidate.invariant_digest().as_bytes().as_slice(),
                candidate.frontier_digest().as_bytes().as_slice(),
                encoded.record_count,
            ],
        )
        .map(|_| ())
        .map_err(|error| ProjectionError::sqlite("record projection generation receipt", error))
}

fn ensure_frontier<S>(
    transaction: &Transaction<'_>,
    name: &str,
    version: i64,
    generation: u64,
    candidate: &RebuildCandidate<S>,
) -> Result<(), ProjectionError> {
    let stored: Option<(Vec<u8>, Vec<u8>)> = transaction
        .query_row(
            "SELECT frontier_digest, frontier FROM peritus_projection_frontiers WHERE projection_name = ?1 AND projection_version = ?2 AND generation = ?3",
            params![name, version, u64_to_i64(generation, "generation")?],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| ProjectionError::sqlite("load projection frontier", error))?;
    if let Some((digest, payload)) = stored {
        if sha256(&payload).as_bytes().as_slice() != digest.as_slice()
            || digest.as_slice() != candidate.frontier_digest().as_bytes()
            || payload != candidate.frontier_payload()
        {
            return Err(ProjectionError::new(
                ProjectionErrorKind::FoldInvariant,
                RecoveryClass::Rebuild,
                "compare projection frontier",
                "same journal binding produced a different aggregate frontier",
            ));
        }
        return Ok(());
    }
    transaction
        .execute(
            "INSERT INTO peritus_projection_frontiers(projection_name, projection_version, generation, frontier_digest, frontier) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                name,
                version,
                u64_to_i64(generation, "generation")?,
                candidate.frontier_digest().as_bytes().as_slice(),
                candidate.frontier_payload(),
            ],
        )
        .map(|_| ())
        .map_err(|error| ProjectionError::sqlite("migrate projection frontier", error))
}

fn delete_progress(
    transaction: &Transaction<'_>,
    name: &str,
    version: i64,
) -> Result<(), ProjectionError> {
    transaction
        .execute(
            "DELETE FROM peritus_projection_work WHERE projection_name = ?1 AND projection_version = ?2",
            params![name, version],
        )
        .map(|_| ())
        .map_err(|error| ProjectionError::sqlite("clear installed projection progress", error))
}

fn verify_authoritative_journal(
    transaction: &Transaction<'_>,
    expected_owner: StoreId,
    expected_position: u64,
    expected_head: Sha256Digest,
) -> Result<(), ProjectionError> {
    let owner: Vec<u8> = transaction
        .query_row("SELECT store_id FROM store_meta WHERE singleton = 1", [], |row| row.get(0))
        .map_err(|error| ProjectionError::sqlite("read authoritative journal owner", error))?;
    if owner.as_slice() != expected_owner.as_bytes() {
        return Err(journal_binding_error(
            RecoveryClass::CorrectInput,
            "candidate belongs to another durable journal owner",
        ));
    }
    let (event_count, last_position): (i64, Option<i64>) = transaction
        .query_row("SELECT COUNT(*), MAX(global_position) FROM events", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .map_err(|error| ProjectionError::sqlite("read authoritative journal range", error))?;
    let event_count = u64::try_from(event_count).map_err(|_| {
        journal_binding_error(RecoveryClass::RepairJournal, "journal event count is negative")
    })?;
    let last_position = match last_position {
        Some(value) => stored_positive_u64(value, "journal last position")?,
        None => 0,
    };
    if event_count != last_position {
        return Err(journal_binding_error(
            RecoveryClass::RepairJournal,
            "journal global positions are not contiguous from origin",
        ));
    }
    if last_position > expected_position {
        return Err(ProjectionError::new(
            ProjectionErrorKind::JournalAdvanced,
            RecoveryClass::Retry,
            "fence projection generation",
            "authoritative journal advanced beyond the candidate frontier",
        ));
    }
    if last_position < expected_position {
        return Err(journal_binding_error(
            RecoveryClass::RepairJournal,
            "authoritative journal moved behind the candidate frontier",
        ));
    }
    let observed_head = authoritative_head(transaction, expected_owner, last_position)?;
    if observed_head != expected_head {
        return Err(journal_binding_error(
            RecoveryClass::RepairJournal,
            "authoritative journal head differs at the candidate position",
        ));
    }
    Ok(())
}

fn authoritative_head(
    transaction: &Transaction<'_>,
    owner: StoreId,
    last_position: u64,
) -> Result<Sha256Digest, ProjectionError> {
    let aggregate_count: i64 = transaction
        .query_row("SELECT COUNT(*) FROM aggregate_heads", [], |row| row.get(0))
        .map_err(|error| ProjectionError::sqlite("count authoritative aggregate heads", error))?;
    let aggregate_count = u32::try_from(aggregate_count).map_err(|_| {
        journal_binding_error(
            RecoveryClass::RepairJournal,
            "aggregate-head count cannot be represented canonically",
        )
    })?;
    let mut bytes = Vec::with_capacity(
        JOURNAL_HEAD_DOMAIN.len() + 16 + 8 + 4 + (aggregate_count as usize) * 58,
    );
    bytes.extend_from_slice(JOURNAL_HEAD_DOMAIN);
    bytes.extend_from_slice(owner.as_bytes());
    bytes.extend_from_slice(&last_position.to_be_bytes());
    bytes.extend_from_slice(&aggregate_count.to_be_bytes());
    let observed = {
        let mut statement = transaction
            .prepare(
                "SELECT aggregate_kind, aggregate_id, sequence, event_hash FROM aggregate_heads ORDER BY aggregate_kind, aggregate_id",
            )
            .map_err(|error| ProjectionError::sqlite("prepare authoritative aggregate heads", error))?;
        let mut rows = statement
            .query([])
            .map_err(|error| ProjectionError::sqlite("query authoritative aggregate heads", error))?;
        let mut observed = 0_u32;
        while let Some(row) = rows
            .next()
            .map_err(|error| ProjectionError::sqlite("read authoritative aggregate head", error))?
        {
            let kind: i64 = row
                .get(0)
                .map_err(|error| ProjectionError::sqlite("decode authoritative aggregate kind", error))?;
            let aggregate_id: Vec<u8> = row
                .get(1)
                .map_err(|error| ProjectionError::sqlite("decode authoritative aggregate identity", error))?;
            let sequence: i64 = row
                .get(2)
                .map_err(|error| ProjectionError::sqlite("decode authoritative aggregate sequence", error))?;
            let event_hash: Vec<u8> = row
                .get(3)
                .map_err(|error| ProjectionError::sqlite("decode authoritative event hash", error))?;
            let kind = u16::try_from(kind).ok().filter(|kind| (1..=18).contains(kind)).ok_or_else(
                || journal_binding_error(RecoveryClass::RepairJournal, "aggregate kind is invalid"),
            )?;
            let sequence = stored_positive_u64(sequence, "aggregate sequence")?;
            if aggregate_id.len() != 16 || aggregate_id.iter().all(|byte| *byte == 0) {
                return Err(journal_binding_error(
                    RecoveryClass::RepairJournal,
                    "aggregate identity is invalid",
                ));
            }
            if event_hash.len() != 32 {
                return Err(journal_binding_error(
                    RecoveryClass::RepairJournal,
                    "aggregate event hash is invalid",
                ));
            }
            bytes.extend_from_slice(&kind.to_be_bytes());
            bytes.extend_from_slice(&aggregate_id);
            bytes.extend_from_slice(&sequence.to_be_bytes());
            bytes.extend_from_slice(&event_hash);
            observed = observed.checked_add(1).ok_or_else(|| {
                journal_binding_error(
                    RecoveryClass::RepairJournal,
                    "aggregate-head count overflowed",
                )
            })?;
        }
        observed
    };
    if observed != aggregate_count {
        return Err(journal_binding_error(
            RecoveryClass::RepairJournal,
            "aggregate-head count changed inside the journal fence",
        ));
    }
    Ok(sha256(&bytes))
}

fn backfill_receipts(
    transaction: &Transaction<'_>,
    owner: StoreId,
) -> Result<(), ProjectionError> {
    transaction
        .execute(
            "INSERT OR IGNORE INTO peritus_projection_receipts(projection_name, projection_version, generation, owner_store_id, last_position, journal_head_digest, payload_digest, schema_digest, invariant_digest, frontier_digest, record_count) SELECT g.projection_name, g.projection_version, g.generation, ?1, g.last_position, g.journal_head_digest, g.payload_digest, g.schema_digest, g.invariant_digest, f.frontier_digest, g.record_count FROM peritus_projection_generations AS g LEFT JOIN peritus_projection_frontiers AS f ON f.projection_name = g.projection_name AND f.projection_version = g.projection_version AND f.generation = g.generation",
            [owner.as_bytes().as_slice()],
        )
        .map(|_| ())
        .map_err(|error| ProjectionError::sqlite("migrate projection generation receipts", error))
}

fn ensure_receipt_owner(
    transaction: &Transaction<'_>,
    name: &str,
    version: i64,
    generation: u64,
    owner: StoreId,
) -> Result<(), ProjectionError> {
    let stored: Vec<u8> = transaction
        .query_row(
            "SELECT owner_store_id FROM peritus_projection_receipts WHERE projection_name = ?1 AND projection_version = ?2 AND generation = ?3",
            params![name, version, u64_to_i64(generation, "generation")?],
            |row| row.get(0),
        )
        .map_err(|error| ProjectionError::sqlite("read projection generation receipt", error))?;
    if stored.as_slice() == owner.as_bytes() {
        Ok(())
    } else {
        Err(ProjectionError::new(
            ProjectionErrorKind::StaleCheckpoint,
            RecoveryClass::CorrectInput,
            "bind projection generation owner",
            "generation receipt belongs to another durable journal owner",
        ))
    }
}

fn initialize_recovery_root(
    transaction: &Transaction<'_>,
    name: &str,
    version: i64,
    active: u64,
    owner: StoreId,
) -> Result<(), ProjectionError> {
    let existing: Option<i64> = transaction
        .query_row(
            "SELECT generation FROM peritus_projection_recovery_roots WHERE projection_name = ?1 AND projection_version = ?2",
            params![name, version],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| ProjectionError::sqlite("read projection recovery root", error))?;
    if existing.is_some() {
        return Ok(());
    }
    let prior: Option<i64> = transaction
        .query_row(
            "SELECT MAX(generation) FROM peritus_projection_generations WHERE projection_name = ?1 AND projection_version = ?2 AND generation < ?3",
            params![name, version, u64_to_i64(active, "active generation")?],
            |row| row.get(0),
        )
        .map_err(|error| ProjectionError::sqlite("select projection recovery root", error))?;
    if let Some(prior) = prior {
        let prior = stored_positive_u64(prior, "recovery generation")?;
        set_recovery_root(transaction, name, version, prior, owner)?;
    }
    Ok(())
}

fn set_recovery_root(
    transaction: &Transaction<'_>,
    name: &str,
    version: i64,
    generation: u64,
    owner: StoreId,
) -> Result<(), ProjectionError> {
    transaction
        .execute(
            "INSERT INTO peritus_projection_recovery_roots(projection_name, projection_version, generation, owner_store_id) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(projection_name, projection_version) DO UPDATE SET generation = excluded.generation, owner_store_id = excluded.owner_store_id",
            params![
                name,
                version,
                u64_to_i64(generation, "recovery generation")?,
                owner.as_bytes().as_slice(),
            ],
        )
        .map(|_| ())
        .map_err(|error| ProjectionError::sqlite("retain projection recovery root", error))
}

fn reclaim_unreferenced(
    transaction: &Transaction<'_>,
    name: &str,
    version: i64,
    owner: StoreId,
) -> Result<(), ProjectionError> {
    transaction
        .execute(
            "DELETE FROM peritus_projection_generations WHERE projection_name = ?1 AND projection_version = ?2 AND EXISTS (SELECT 1 FROM peritus_projection_receipts AS r WHERE r.projection_name = peritus_projection_generations.projection_name AND r.projection_version = peritus_projection_generations.projection_version AND r.generation = peritus_projection_generations.generation AND r.owner_store_id = ?3) AND NOT EXISTS (SELECT 1 FROM peritus_projection_catalog AS c WHERE c.projection_name = peritus_projection_generations.projection_name AND c.projection_version = peritus_projection_generations.projection_version AND c.active_generation = peritus_projection_generations.generation) AND NOT EXISTS (SELECT 1 FROM peritus_projection_work AS w WHERE w.projection_name = peritus_projection_generations.projection_name AND w.projection_version = peritus_projection_generations.projection_version AND w.source_generation = peritus_projection_generations.generation) AND NOT EXISTS (SELECT 1 FROM peritus_projection_recovery_roots AS rr WHERE rr.projection_name = peritus_projection_generations.projection_name AND rr.projection_version = peritus_projection_generations.projection_version AND rr.generation = peritus_projection_generations.generation)",
            params![name, version, owner.as_bytes().as_slice()],
        )
        .map(|_| ())
        .map_err(|error| ProjectionError::sqlite("reclaim unreferenced projection generations", error))
}

fn journal_binding_error(recovery: RecoveryClass, detail: &'static str) -> ProjectionError {
    ProjectionError::new(
        ProjectionErrorKind::StaleCheckpoint,
        recovery,
        "fence projection generation",
        detail,
    )
}
