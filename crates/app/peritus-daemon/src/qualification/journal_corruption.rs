//! Controlled authoritative-journal corruption and fail-closed startup qualification.

use peritus_codec::sha256;
use peritus_journal::JournalErrorKind;
use peritus_types::Sha256Digest;
use rusqlite::{Connection, OptionalExtension as _, params};

use crate::outbox::stage_gate_after_crash;
use crate::{DaemonConfig, DaemonError, DaemonErrorCode, DaemonRecovery, DaemonRuntime};

use super::journal::open_journal;

const CORRUPT_FRAME: &[u8] = b"peritus/h1/deliberately-corrupt-journal-frame/v1";

/// Exact facts retained after changing one committed event frame without changing its hash.
pub struct JournalCorruptionCheckpoint {
    request_sha256: String,
    original_frame_sha256: Sha256Digest,
    corrupt_frame_sha256: Sha256Digest,
    event_count: u64,
    corruption_detected: bool,
}

impl JournalCorruptionCheckpoint {
    pub fn request_sha256(&self) -> &str {
        &self.request_sha256
    }
    pub const fn original_frame_sha256(&self) -> Sha256Digest {
        self.original_frame_sha256
    }
    pub const fn corrupt_frame_sha256(&self) -> Sha256Digest {
        self.corrupt_frame_sha256
    }
    pub const fn event_count(&self) -> u64 {
        self.event_count
    }
    pub const fn corruption_detected(&self) -> bool {
        self.corruption_detected
    }
}

/// Fresh-process facts after startup refused the corrupt authoritative journal.
pub struct JournalCorruptionObservation {
    startup_error_code: &'static str,
    corrupt_frame_sha256: Sha256Digest,
    event_count: u64,
    aggregate_heads: u64,
    state_records: u64,
    authority_epochs: u64,
    application_principals: u64,
    corruption_detected: bool,
    mutation_admitted: bool,
}

impl JournalCorruptionObservation {
    pub const fn startup_error_code(&self) -> &'static str {
        self.startup_error_code
    }
    pub const fn corrupt_frame_sha256(&self) -> Sha256Digest {
        self.corrupt_frame_sha256
    }
    pub const fn event_count(&self) -> u64 {
        self.event_count
    }
    pub const fn aggregate_heads(&self) -> u64 {
        self.aggregate_heads
    }
    pub const fn state_records(&self) -> u64 {
        self.state_records
    }
    pub const fn authority_epochs(&self) -> u64 {
        self.authority_epochs
    }
    pub const fn application_principals(&self) -> u64 {
        self.application_principals
    }
    pub const fn corruption_detected(&self) -> bool {
        self.corruption_detected
    }
    pub const fn mutation_admitted(&self) -> bool {
        self.mutation_admitted
    }
}

/// Commits one real D1 gate event, then changes its stored frame without updating its digest.
pub fn stage_corruption(config: &DaemonConfig) -> Result<JournalCorruptionCheckpoint, DaemonError> {
    let gate = stage_gate_after_crash(config)?;
    let request_sha256 = gate.request_sha256().to_owned();
    let connection = open_database(config)?;
    let original_frame = frame(&connection)?;
    let recorded_digest = recorded_frame_digest(&connection)?;
    let original_frame_sha256 = sha256(&original_frame);
    if original_frame_sha256.as_bytes() != recorded_digest.as_slice() {
        return Err(qualification_error("committed journal frame was invalid before injection"));
    }
    let changed = connection
        .execute("UPDATE events SET frame = ?1 WHERE global_position = 1", params![CORRUPT_FRAME])
        .map_err(sqlite_error)?;
    if changed != 1 {
        return Err(qualification_error("journal fault injection changed the wrong row count"));
    }
    let corrupt_frame_sha256 = sha256(&frame(&connection)?);
    let event_count = count(&connection, "events")?;
    drop(connection);
    drop(gate);
    if event_count != 1 || corrupt_frame_sha256 == original_frame_sha256 {
        return Err(qualification_error("journal fault injection was not exact"));
    }
    let store_id = config.store_identity()?;
    let mut journal = open_journal(config, store_id)?;
    let error = journal.integrity_scan().expect_err("injected journal must be corrupt");
    if error.kind() != JournalErrorKind::CorruptJournal {
        return Err(qualification_error("journal fault produced the wrong failure category"));
    }
    Ok(JournalCorruptionCheckpoint {
        request_sha256,
        original_frame_sha256,
        corrupt_frame_sha256,
        event_count,
        corruption_detected: true,
    })
}

/// Starts the real daemon and proves corruption stops startup before authority mutation.
pub async fn recover_corruption(
    config: DaemonConfig,
) -> Result<JournalCorruptionObservation, DaemonError> {
    let before = inspect(&config)?;
    let startup = DaemonRuntime::start(config.clone()).await;
    let error = match startup {
        Ok(runtime) => {
            let _ = runtime.shutdown().await;
            return Err(qualification_error("daemon startup admitted a corrupt journal"));
        }
        Err(error) => error,
    };
    if error.code_kind() != DaemonErrorCode::CorruptState
        || error.recovery() != DaemonRecovery::ReadOnly
    {
        return Err(qualification_error("daemon startup reported the wrong corruption failure"));
    }
    let after = inspect(&config)?;
    let mutation_admitted = before != after;
    if mutation_admitted
        || after.event_count != 1
        || after.aggregate_heads != 1
        || after.state_records != 1
        || after.authority_epochs != 0
        || after.application_principals != 0
    {
        return Err(qualification_error("failed startup changed authoritative journal state"));
    }
    let store_id = config.store_identity()?;
    let mut journal = open_journal(&config, store_id)?;
    let corruption_detected = journal
        .integrity_scan()
        .is_err_and(|failure| failure.kind() == JournalErrorKind::CorruptJournal);
    if !corruption_detected {
        return Err(qualification_error("corrupt journal became admissible after failed startup"));
    }
    Ok(JournalCorruptionObservation {
        startup_error_code: error.code(),
        corrupt_frame_sha256: after.frame_sha256,
        event_count: after.event_count,
        aggregate_heads: after.aggregate_heads,
        state_records: after.state_records,
        authority_epochs: after.authority_epochs,
        application_principals: after.application_principals,
        corruption_detected,
        mutation_admitted,
    })
}

#[derive(Eq, PartialEq)]
struct JournalState {
    frame_sha256: Sha256Digest,
    event_count: u64,
    aggregate_heads: u64,
    state_records: u64,
    authority_epochs: u64,
    application_principals: u64,
}

fn inspect(config: &DaemonConfig) -> Result<JournalState, DaemonError> {
    let connection = open_database(config)?;
    Ok(JournalState {
        frame_sha256: sha256(&frame(&connection)?),
        event_count: count(&connection, "events")?,
        aggregate_heads: count(&connection, "aggregate_heads")?,
        state_records: count(&connection, "state_records")?,
        authority_epochs: count(&connection, "authority_clock")?,
        application_principals: count(&connection, "app_principals")?,
    })
}

fn open_database(config: &DaemonConfig) -> Result<Connection, DaemonError> {
    Connection::open(config.paths().database()).map_err(sqlite_error)
}

fn frame(connection: &Connection) -> Result<Vec<u8>, DaemonError> {
    connection
        .query_row("SELECT frame FROM events WHERE global_position = 1", [], |row| row.get(0))
        .optional()
        .map_err(sqlite_error)?
        .ok_or_else(|| qualification_error("qualification journal event is absent"))
}

fn recorded_frame_digest(connection: &Connection) -> Result<Vec<u8>, DaemonError> {
    connection
        .query_row("SELECT frame_digest FROM events WHERE global_position = 1", [], |row| {
            row.get(0)
        })
        .optional()
        .map_err(sqlite_error)?
        .ok_or_else(|| qualification_error("qualification journal digest is absent"))
}

fn count(connection: &Connection, table: &'static str) -> Result<u64, DaemonError> {
    let statement = format!("SELECT COUNT(*) FROM {table}");
    let value =
        connection.query_row(&statement, [], |row| row.get::<_, i64>(0)).map_err(sqlite_error)?;
    u64::try_from(value).map_err(|_| qualification_error("journal row count is negative"))
}

fn sqlite_error(error: rusqlite::Error) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Storage,
        DaemonRecovery::Reconcile,
        "qualify journal corruption",
        error.to_string(),
        error,
    )
}

fn qualification_error(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::Operator,
        "qualify journal corruption fail-closed startup",
        detail,
    )
}
