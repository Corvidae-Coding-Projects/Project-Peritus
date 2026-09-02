//! Controlled corruption and real startup repair of a durable derived projection.

use std::time::Duration;

use peritus_codec::sha256;
use peritus_projection::{
    JournalCatalogProjection, Projection, ProjectionStore, RepairAction, StoreOptions,
};
use peritus_types::Sha256Digest;
use rusqlite::{Connection, params};

use crate::instance::InstanceGuard;
use crate::{DaemonConfig, DaemonError, DaemonErrorCode, DaemonRecovery};

use super::{acquire_instance, journal_error, open_journal};

const CORRUPT_PAYLOAD: &[u8] = b"peritus/h1/deliberately-corrupt-projection/v1";

/// Direct facts retained after controlled projection corruption.
pub struct ProjectionCorruptionCheckpoint {
    projection_name: String,
    generation: u64,
    original_payload_sha256: Sha256Digest,
    corrupt_payload_sha256: Sha256Digest,
    payload_bytes: u64,
    _instance: InstanceGuard,
}

impl ProjectionCorruptionCheckpoint {
    pub fn projection_name(&self) -> &str {
        &self.projection_name
    }
    pub const fn generation(&self) -> u64 {
        self.generation
    }
    pub const fn original_payload_sha256(&self) -> Sha256Digest {
        self.original_payload_sha256
    }
    pub const fn corrupt_payload_sha256(&self) -> Sha256Digest {
        self.corrupt_payload_sha256
    }
    pub const fn payload_bytes(&self) -> u64 {
        self.payload_bytes
    }
}

/// Fresh-process facts after startup replaced the corrupt active generation.
pub struct ProjectionRepairObservation {
    projection_name: String,
    previous_generation: u64,
    repaired_generation: u64,
    corrupt_payload_sha256: Sha256Digest,
    repaired_payload_sha256: Sha256Digest,
    payload_bytes: u64,
    generation_count: u64,
    event_count: u64,
    aggregate_heads: u64,
    payload_valid: bool,
    reusable: bool,
}

impl ProjectionRepairObservation {
    pub fn projection_name(&self) -> &str {
        &self.projection_name
    }
    pub const fn previous_generation(&self) -> u64 {
        self.previous_generation
    }
    pub const fn repaired_generation(&self) -> u64 {
        self.repaired_generation
    }
    pub const fn corrupt_payload_sha256(&self) -> Sha256Digest {
        self.corrupt_payload_sha256
    }
    pub const fn repaired_payload_sha256(&self) -> Sha256Digest {
        self.repaired_payload_sha256
    }
    pub const fn payload_bytes(&self) -> u64 {
        self.payload_bytes
    }
    pub const fn generation_count(&self) -> u64 {
        self.generation_count
    }
    pub const fn event_count(&self) -> u64 {
        self.event_count
    }
    pub const fn aggregate_heads(&self) -> u64 {
        self.aggregate_heads
    }
    pub const fn payload_valid(&self) -> bool {
        self.payload_valid
    }
    pub const fn reusable(&self) -> bool {
        self.reusable
    }
}

pub fn stage_corruption(
    config: &DaemonConfig,
) -> Result<ProjectionCorruptionCheckpoint, DaemonError> {
    let store_id = config.store_identity()?;
    let instance = acquire_instance(config, store_id)?;
    let mut journal = open_journal(config, store_id)?;
    ensure_empty(&mut journal)?;
    let projection = JournalCatalogProjection::new().map_err(projection_error)?;
    let store =
        crate::startup::ensure_projections_current(&mut journal, &config.paths().database())?;
    let active = store
        .load_active(projection.schema())
        .map_err(projection_error)?
        .ok_or_else(|| qualification_error("startup did not install the journal projection"))?;
    if !active.payload_is_valid() {
        return Err(qualification_error("journal projection was corrupt before fault injection"));
    }
    let projection_name = projection.schema().identity().name().as_str().to_owned();
    let generation = active.generation().get();
    let original_payload_sha256 = sha256(active.payload());
    let payload_bytes = u64::try_from(active.payload().len())
        .map_err(|_| qualification_error("projection payload length overflowed"))?;
    drop(store);
    drop(journal);

    let connection = open_database(config)?;
    let version = i64::try_from(projection.schema().identity().version().get())
        .map_err(|_| qualification_error("projection version does not fit SQLite"))?;
    let stored_generation = i64::try_from(generation)
        .map_err(|_| qualification_error("projection generation does not fit SQLite"))?;
    let changed = connection
        .execute(
            "UPDATE peritus_projection_generations SET payload = ?1 WHERE projection_name = ?2 AND projection_version = ?3 AND generation = ?4",
            params![
                CORRUPT_PAYLOAD,
                projection_name,
                version,
                stored_generation,
            ],
        )
        .map_err(sqlite_error)?;
    if changed != 1 {
        return Err(qualification_error("projection fault injection changed the wrong row count"));
    }
    drop(connection);
    let reopened = ProjectionStore::open(config.paths().database(), StoreOptions::default())
        .map_err(projection_error)?;
    let corrupt = reopened
        .load_active(projection.schema())
        .map_err(projection_error)?
        .ok_or_else(|| qualification_error("corrupt projection lost its active generation"))?;
    if corrupt.generation().get() != generation || corrupt.payload_is_valid() {
        return Err(qualification_error("projection fault injection was not observable"));
    }
    Ok(ProjectionCorruptionCheckpoint {
        projection_name,
        generation,
        original_payload_sha256,
        corrupt_payload_sha256: sha256(corrupt.payload()),
        payload_bytes,
        _instance: instance,
    })
}

pub fn recover_corruption(
    config: &DaemonConfig,
) -> Result<ProjectionRepairObservation, DaemonError> {
    let store_id = config.store_identity()?;
    let _instance = acquire_instance(config, store_id)?;
    let projection = JournalCatalogProjection::new().map_err(projection_error)?;
    let before = ProjectionStore::open(config.paths().database(), StoreOptions::default())
        .map_err(projection_error)?;
    let corrupt = before
        .load_active(projection.schema())
        .map_err(projection_error)?
        .ok_or_else(|| qualification_error("corrupt projection has no active generation"))?;
    if corrupt.payload_is_valid() {
        return Err(qualification_error("projection recovery did not begin from corrupt bytes"));
    }
    let previous_generation = corrupt.generation().get();
    let corrupt_payload_sha256 = sha256(corrupt.payload());
    drop(before);

    let mut journal = open_journal(config, store_id)?;
    ensure_empty(&mut journal)?;
    let store =
        crate::startup::ensure_projections_current(&mut journal, &config.paths().database())?;
    let report = journal.integrity_scan().map_err(journal_error)?;
    let repaired = store
        .load_active(projection.schema())
        .map_err(projection_error)?
        .ok_or_else(|| qualification_error("startup repair left no active projection"))?;
    let reusable = matches!(
        store.plan_startup(projection.schema(), &report).map_err(projection_error)?,
        RepairAction::Reuse(generation) if generation == repaired.generation()
    );
    let repaired_generation = repaired.generation().get();
    let payload_bytes = u64::try_from(repaired.payload().len())
        .map_err(|_| qualification_error("repaired projection payload length overflowed"))?;
    let generation_count = store.generation_count(projection.schema()).map_err(projection_error)?;
    if repaired_generation != previous_generation.saturating_add(1)
        || generation_count != 2
        || !repaired.payload_is_valid()
        || !reusable
    {
        return Err(qualification_error(
            "startup did not atomically replace the corrupt projection",
        ));
    }
    Ok(ProjectionRepairObservation {
        projection_name: projection.schema().identity().name().as_str().to_owned(),
        previous_generation,
        repaired_generation,
        corrupt_payload_sha256,
        repaired_payload_sha256: sha256(repaired.payload()),
        payload_bytes,
        generation_count,
        event_count: report.event_count(),
        aggregate_heads: report.aggregate_count(),
        payload_valid: true,
        reusable,
    })
}

fn ensure_empty(journal: &mut peritus_journal::SqliteJournal) -> Result<(), DaemonError> {
    let report = journal.integrity_scan().map_err(journal_error)?;
    if report.event_count() == 0 && report.aggregate_count() == 0 && report.last_position() == 0 {
        Ok(())
    } else {
        Err(qualification_error("projection qualification journal is not empty"))
    }
}

fn open_database(config: &DaemonConfig) -> Result<Connection, DaemonError> {
    let connection = Connection::open(config.paths().database()).map_err(sqlite_error)?;
    connection.busy_timeout(Duration::from_secs(5)).map_err(sqlite_error)?;
    Ok(connection)
}

fn projection_error(error: peritus_projection::ProjectionError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::ReadOnly,
        error.operation(),
        error.to_string(),
        error,
    )
}

fn sqlite_error(error: rusqlite::Error) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Storage,
        DaemonRecovery::Reconcile,
        "inject projection corruption",
        error.to_string(),
        error,
    )
}

fn qualification_error(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::Operator,
        "qualify projection corruption repair",
        detail,
    )
}
