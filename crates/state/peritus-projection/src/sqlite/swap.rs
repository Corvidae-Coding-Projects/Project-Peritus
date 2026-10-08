//! Transactional shadow-generation install and active-pointer swap.

use super::ProjectionStore;
use super::contention;
use super::store::{stored_positive_u64, u64_to_i64};
use crate::{
    CatalogGeneration, ProjectionError, ProjectionErrorKind, RebuildCandidate, RecoveryClass,
};
use peritus_codec::sha256;
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};

impl ProjectionStore {
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
            transaction
                .execute(
                    "INSERT INTO peritus_projection_catalog(projection_name, projection_version, active_generation) VALUES (?1, ?2, ?3) ON CONFLICT(projection_name, projection_version) DO UPDATE SET active_generation = excluded.active_generation",
                    params![identity.name().as_str(), version, next_sql],
                )
                .map_err(|error| ProjectionError::sqlite("activate projection generation", error))?;
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
            "SELECT MAX(generation) FROM peritus_projection_generations WHERE projection_name = ?1 AND projection_version = ?2",
            params![name, version],
            |row| row.get(0),
        )
        .map_err(|error| ProjectionError::sqlite("read highest projection generation", error))?;
    value
        .map(|value| stored_positive_u64(value, "highest generation"))
        .transpose()
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
    Ok(())
}
