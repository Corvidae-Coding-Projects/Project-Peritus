//! Connection policy, generation reads, and startup repair planning.

use crate::catalog::plan_repair;
use crate::{
    ActiveGeneration, CatalogGeneration, Checkpoint, ProjectionError, ProjectionErrorKind,
    ProjectionIdentity, ProjectionSchema, RecoveryClass, RepairAction,
};
use peritus_journal::{IntegrityReport, JournalCancellation};
use peritus_types::Sha256Digest;
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use std::{num::NonZeroU64, path::Path, time::Duration};

use super::contention::{self, ContentionPolicy};

/// `SQLite` connection policy for the projection adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StoreOptions {
    busy_timeout: Option<Duration>,
}

impl StoreOptions {
    /// Creates options with an explicit finite busy timeout.
    #[must_use]
    pub const fn new(busy_timeout: Duration) -> Self {
        Self::with_timeout(busy_timeout)
    }

    /// Selects an explicit finite contention deadline for an isolated attempt.
    #[must_use]
    pub const fn with_timeout(busy_timeout: Duration) -> Self {
        Self { busy_timeout: Some(busy_timeout) }
    }

    /// Selects immediate failure on database contention.
    #[must_use]
    pub const fn fail_fast() -> Self {
        Self { busy_timeout: None }
    }

    /// Returns the explicit timeout, or `None` for fail-fast operation.
    #[must_use]
    pub const fn busy_timeout(self) -> Option<Duration> {
        self.busy_timeout
    }

    const fn contention(self) -> ContentionPolicy {
        match self.busy_timeout {
            Some(timeout) => ContentionPolicy::Timeout(timeout),
            None => ContentionPolicy::FailFast,
        }
    }
}

impl Default for StoreOptions {
    fn default() -> Self {
        Self::fail_fast()
    }
}

/// Projection-owned catalog in a caller-selected `SQLite` file.
pub struct ProjectionStore {
    pub(super) connection: Connection,
    pub(super) cancellation: Option<JournalCancellation>,
}

impl ProjectionStore {
    /// Opens the caller-selected database, applies durable connection policy, and installs the
    /// projection-owned schema without touching journal or artifact tables.
    ///
    /// # Errors
    ///
    /// Returns a typed storage error when `SQLite` cannot open, configure, or install the schema.
    pub fn open(path: impl AsRef<Path>, options: StoreOptions) -> Result<Self, ProjectionError> {
        Self::open_configured(path.as_ref(), options.contention(), None)
    }

    /// Opens a projection catalog whose startup and later contention waits belong to the supplied
    /// cancellation owner.
    ///
    /// This form has no elapsed patience deadline. Cancellation ends a pending wait with its
    /// original busy or locked classification, and exact install retries retain their candidate
    /// identity and compare-and-swap semantics.
    ///
    /// # Errors
    ///
    /// Returns a typed busy, locked, configuration, or schema-installation failure.
    pub fn open_waiting(
        path: impl AsRef<Path>,
        cancellation: &JournalCancellation,
    ) -> Result<Self, ProjectionError> {
        contention::run(Some(cancellation), || {
            Self::open_configured(
                path.as_ref(),
                ContentionPolicy::WaitForCancellation,
                Some(cancellation.clone()),
            )
        })
    }

    fn open_configured(
        path: &Path,
        contention_policy: ContentionPolicy,
        cancellation: Option<JournalCancellation>,
    ) -> Result<Self, ProjectionError> {
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let connection = Connection::open_with_flags(path, flags)
            .map_err(|error| ProjectionError::sqlite("open projection database", error))?;
        contention::configure(&connection, contention_policy)?;
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(|error| ProjectionError::sqlite("enable projection WAL", error))?;
        connection
            .pragma_update(None, "synchronous", "FULL")
            .map_err(|error| ProjectionError::sqlite("set projection synchronous", error))?;
        connection
            .pragma_update(None, "foreign_keys", true)
            .map_err(|error| ProjectionError::sqlite("enable projection foreign keys", error))?;
        connection
            .execute_batch(super::schema::INSTALL)
            .map_err(|error| ProjectionError::sqlite("install projection schema", error))?;
        Ok(Self { connection, cancellation })
    }

    /// Loads and validates the active generation for an expected projection identity.
    ///
    /// # Errors
    ///
    /// Returns a typed storage or corrupt-catalog error for malformed durable values.
    pub fn load_active(
        &self,
        expected_schema: &ProjectionSchema,
    ) -> Result<Option<ActiveGeneration>, ProjectionError> {
        contention::run(self.cancellation.as_ref(), || {
            let identity = expected_schema.identity();
            let raw = self
                .connection
                .query_row(
                    "SELECT g.generation, g.last_position, g.journal_head_digest, g.payload_digest, g.schema_digest, g.invariant_digest, g.record_count, g.payload, f.frontier_digest, f.frontier FROM peritus_projection_catalog AS c JOIN peritus_projection_generations AS g ON g.projection_name = c.projection_name AND g.projection_version = c.projection_version AND g.generation = c.active_generation LEFT JOIN peritus_projection_frontiers AS f ON f.projection_name = g.projection_name AND f.projection_version = g.projection_version AND f.generation = g.generation WHERE c.projection_name = ?1 AND c.projection_version = ?2",
                    params![identity.name().as_str(), u64_to_i64(identity.version().get(), "projection version")?],
                    |row| {
                        Ok(RawGeneration {
                            generation: row.get(0)?,
                            last_position: row.get(1)?,
                            journal_head: row.get(2)?,
                            payload_digest: row.get(3)?,
                            schema_digest: row.get(4)?,
                            invariant_digest: row.get(5)?,
                            record_count: row.get(6)?,
                            payload: row.get(7)?,
                            frontier_digest: row.get(8)?,
                            frontier_payload: row.get(9)?,
                        })
                    },
                )
                .optional()
                .map_err(|error| ProjectionError::sqlite("load active projection", error))?;
            raw.map(|raw| parse_generation(identity.clone(), raw)).transpose()
        })
    }

    /// Plans startup reuse or rebuild against an exact checked journal report.
    ///
    /// # Errors
    ///
    /// Returns a catalog read or validation failure.
    pub fn plan_startup(
        &self,
        schema: &ProjectionSchema,
        journal: &IntegrityReport,
    ) -> Result<RepairAction, ProjectionError> {
        let active = self.load_active(schema)?;
        Ok(plan_repair(
            active.as_ref(),
            schema,
            journal.last_position(),
            journal.journal_head_digest(),
        ))
    }

    /// Returns the number of durable generations retained for one projection identity.
    ///
    /// # Errors
    ///
    /// Returns a typed storage or corrupt-catalog error.
    pub fn generation_count(&self, schema: &ProjectionSchema) -> Result<u64, ProjectionError> {
        contention::run(self.cancellation.as_ref(), || {
            let identity = schema.identity();
            let count: i64 = self
                .connection
                .query_row(
                    "SELECT COUNT(*) FROM peritus_projection_generations WHERE projection_name = ?1 AND projection_version = ?2",
                    params![identity.name().as_str(), u64_to_i64(identity.version().get(), "projection version")?],
                    |row| row.get(0),
                )
                .map_err(|error| ProjectionError::sqlite("count projection generations", error))?;
            nonnegative_u64(count, "generation count")
        })
    }
}

struct RawGeneration {
    generation: i64,
    last_position: i64,
    journal_head: Vec<u8>,
    payload_digest: Vec<u8>,
    schema_digest: Vec<u8>,
    invariant_digest: Vec<u8>,
    record_count: i64,
    payload: Vec<u8>,
    frontier_digest: Option<Vec<u8>>,
    frontier_payload: Option<Vec<u8>>,
}

fn parse_generation(
    identity: ProjectionIdentity,
    raw: RawGeneration,
) -> Result<ActiveGeneration, ProjectionError> {
    let generation =
        CatalogGeneration::from_u64(stored_positive_u64(raw.generation, "generation")?)?;
    let last_position = nonnegative_u64(raw.last_position, "last position")?;
    let record_count = nonnegative_u64(raw.record_count, "record count")?;
    let schema_digest = digest(&raw.schema_digest, "schema digest")?;
    let schema = ProjectionSchema::from_digest(identity, schema_digest);
    let checkpoint = Checkpoint::from_digests(
        schema,
        last_position,
        digest(&raw.journal_head, "journal head digest")?,
        digest(&raw.payload_digest, "payload digest")?,
    );
    let frontier = match (raw.frontier_digest, raw.frontier_payload) {
        (Some(digest_bytes), Some(payload)) => {
            digest(&digest_bytes, "frontier digest").ok().map(|digest| (digest, payload))
        }
        _ => None,
    };
    Ok(ActiveGeneration::new(
        generation,
        checkpoint,
        digest(&raw.invariant_digest, "invariant digest")?,
        record_count,
        raw.payload,
        frontier,
    ))
}

pub(super) fn digest(bytes: &[u8], field: &'static str) -> Result<Sha256Digest, ProjectionError> {
    let array: [u8; 32] = bytes.try_into().map_err(|_| corrupt(field, "must be 32 bytes"))?;
    Ok(Sha256Digest::new(array))
}

pub(super) fn u64_to_i64(value: u64, field: &'static str) -> Result<i64, ProjectionError> {
    i64::try_from(value).map_err(|_| {
        ProjectionError::new(
            ProjectionErrorKind::InvalidInput,
            RecoveryClass::CorrectInput,
            "encode projection SQLite value",
            format!("{field} does not fit SQLite INTEGER"),
        )
    })
}

pub(super) fn stored_positive_u64(
    value: i64,
    field: &'static str,
) -> Result<u64, ProjectionError> {
    let value = nonnegative_u64(value, field)?;
    if NonZeroU64::new(value).is_none() {
        Err(corrupt(field, "must be positive"))
    } else {
        Ok(value)
    }
}

fn nonnegative_u64(value: i64, field: &'static str) -> Result<u64, ProjectionError> {
    u64::try_from(value).map_err(|_| corrupt(field, "must be nonnegative"))
}

fn corrupt(field: &'static str, detail: &'static str) -> ProjectionError {
    ProjectionError::new(
        ProjectionErrorKind::CorruptCatalog,
        RecoveryClass::Rebuild,
        "read projection catalog",
        format!("{field} {detail}"),
    )
}
