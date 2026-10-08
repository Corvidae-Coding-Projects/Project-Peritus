//! Connection policy, generation reads, and startup repair planning.

use crate::catalog::plan_repair;
use crate::{
    ActiveGeneration, CatalogGeneration, Checkpoint, ProjectionError, ProjectionErrorKind,
    ProjectionIdentity, ProjectionSchema, RecoveryClass, RepairAction, RepairReason,
};
use peritus_codec::sha256;
use peritus_journal::{IntegrityReport, JournalCancellation};
use peritus_types::Sha256Digest;
use rusqlite::{Connection, OpenFlags, OptionalExtension, params, types::Value};
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

pub(crate) enum CatalogObservation {
    Missing,
    Active { generation: ActiveGeneration, diagnostic: Vec<u8> },
    Corrupt(CatalogCorruption),
}

#[derive(Clone, Copy)]
pub(crate) enum ContainmentReason {
    Malformed,
    Metadata,
    TypedCheckpoint,
}

impl ContainmentReason {
    pub(super) const fn tag(self) -> i64 {
        match self {
            Self::Malformed => 1,
            Self::Metadata => 2,
            Self::TypedCheckpoint => 3,
        }
    }
}

pub(crate) struct CatalogCorruption {
    diagnostic: Vec<u8>,
    diagnostic_digest: Sha256Digest,
    expected_generation: Option<CatalogGeneration>,
    reason: ContainmentReason,
}

impl CatalogCorruption {
    fn new(
        diagnostic: Vec<u8>,
        expected_generation: Option<CatalogGeneration>,
        reason: ContainmentReason,
    ) -> Self {
        let diagnostic_digest = sha256(&diagnostic);
        Self { diagnostic, diagnostic_digest, expected_generation, reason }
    }

    pub(crate) fn from_active(
        diagnostic: Vec<u8>,
        generation: CatalogGeneration,
        reason: ContainmentReason,
    ) -> Self {
        Self::new(diagnostic, Some(generation), reason)
    }

    pub(super) fn diagnostic(&self) -> &[u8] {
        &self.diagnostic
    }

    pub(super) const fn diagnostic_digest(&self) -> Sha256Digest {
        self.diagnostic_digest
    }

    pub(super) const fn expected_generation(&self) -> Option<CatalogGeneration> {
        self.expected_generation
    }

    pub(super) const fn reason(&self) -> ContainmentReason {
        self.reason
    }

    fn error(&self) -> ProjectionError {
        ProjectionError::new(
            ProjectionErrorKind::CorruptCatalog,
            RecoveryClass::Rebuild,
            "read projection catalog",
            format!("malformed active metadata diagnostic {:?}", self.diagnostic_digest),
        )
    }
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
        match self.observe_active(expected_schema)? {
            CatalogObservation::Missing => Ok(None),
            CatalogObservation::Active { generation, .. } => Ok(Some(generation)),
            CatalogObservation::Corrupt(corruption) => Err(corruption.error()),
        }
    }

    pub(crate) fn observe_active(
        &self,
        expected_schema: &ProjectionSchema,
    ) -> Result<CatalogObservation, ProjectionError> {
        contention::run(self.cancellation.as_ref(), || {
            let Some(raw) = read_raw_catalog(&self.connection, expected_schema)? else {
                return Ok(CatalogObservation::Missing);
            };
            let diagnostic = raw.diagnostic.clone();
            match parse_generation(expected_schema.identity().clone(), raw.values) {
                Ok(generation) => Ok(CatalogObservation::Active { generation, diagnostic }),
                Err(_) => Ok(CatalogObservation::Corrupt(CatalogCorruption::new(
                    diagnostic,
                    raw.expected_generation,
                    ContainmentReason::Malformed,
                ))),
            }
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
        match self.observe_active(schema)? {
            CatalogObservation::Missing => Ok(plan_repair(
                None,
                schema,
                journal.last_position(),
                journal.journal_head_digest(),
            )),
            CatalogObservation::Active { generation, .. } => Ok(plan_repair(
                Some(&generation),
                schema,
                journal.last_position(),
                journal.journal_head_digest(),
            )),
            CatalogObservation::Corrupt(_) => {
                Ok(RepairAction::RebuildFromGenesis(RepairReason::CatalogCorrupt))
            }
        }
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

pub(super) struct RawCatalog {
    values: RawGeneration,
    diagnostic: Vec<u8>,
    expected_generation: Option<CatalogGeneration>,
}

#[derive(Clone)]
struct RawGeneration {
    catalog_generation: Value,
    generation: Value,
    last_position: Value,
    journal_head: Value,
    payload_digest: Value,
    schema_digest: Value,
    invariant_digest: Value,
    record_count: Value,
    payload: Value,
    frontier_digest: Value,
    frontier_payload: Value,
    metadata_digest: Value,
}

pub(super) fn read_raw_catalog(
    connection: &Connection,
    expected_schema: &ProjectionSchema,
) -> Result<Option<RawCatalog>, ProjectionError> {
    let identity = expected_schema.identity();
    let raw = connection
        .query_row(
            "SELECT c.active_generation, g.generation, g.last_position, g.journal_head_digest, g.payload_digest, g.schema_digest, g.invariant_digest, g.record_count, g.payload, f.frontier_digest, f.frontier, b.metadata_digest FROM peritus_projection_catalog AS c LEFT JOIN peritus_projection_generations AS g ON g.projection_name = c.projection_name AND g.projection_version = c.projection_version AND g.generation = c.active_generation LEFT JOIN peritus_projection_frontiers AS f ON f.projection_name = g.projection_name AND f.projection_version = g.projection_version AND f.generation = g.generation LEFT JOIN peritus_projection_checkpoint_bindings AS b ON b.projection_name = g.projection_name AND b.projection_version = g.projection_version AND b.generation = g.generation WHERE c.projection_name = ?1 AND c.projection_version = ?2",
            params![
                identity.name().as_str(),
                u64_to_i64(identity.version().get(), "projection version")?,
            ],
            |row| {
                Ok(RawGeneration {
                    catalog_generation: row.get(0)?,
                    generation: row.get(1)?,
                    last_position: row.get(2)?,
                    journal_head: row.get(3)?,
                    payload_digest: row.get(4)?,
                    schema_digest: row.get(5)?,
                    invariant_digest: row.get(6)?,
                    record_count: row.get(7)?,
                    payload: row.get(8)?,
                    frontier_digest: row.get(9)?,
                    frontier_payload: row.get(10)?,
                    metadata_digest: row.get(11)?,
                })
            },
        )
        .optional()
        .map_err(|error| ProjectionError::sqlite("load raw active projection", error))?;
    Ok(raw.map(|values| {
        let diagnostic = encode_raw_generation(identity, &values);
        let expected_generation = value_positive_generation(&values.catalog_generation);
        RawCatalog { values, diagnostic, expected_generation }
    }))
}

impl RawCatalog {
    pub(super) fn diagnostic(&self) -> &[u8] {
        &self.diagnostic
    }
}

fn parse_generation(
    identity: ProjectionIdentity,
    raw: RawGeneration,
) -> Result<ActiveGeneration, ProjectionError> {
    let catalog_generation = value_i64(raw.catalog_generation, "catalog generation")?;
    let generation = value_i64(raw.generation, "generation")?;
    if catalog_generation != generation {
        return Err(corrupt("generation", "does not match the active catalog pointer"));
    }
    let generation = CatalogGeneration::from_u64(stored_positive_u64(generation, "generation")?)?;
    let last_position = nonnegative_u64(value_i64(raw.last_position, "last position")?, "last position")?;
    let record_count = nonnegative_u64(value_i64(raw.record_count, "record count")?, "record count")?;
    let schema_digest = digest(&value_blob(raw.schema_digest, "schema digest")?, "schema digest")?;
    let schema = ProjectionSchema::from_digest(identity, schema_digest);
    let checkpoint = Checkpoint::from_digests(
        schema,
        last_position,
        digest(&value_blob(raw.journal_head, "journal head digest")?, "journal head digest")?,
        digest(&value_blob(raw.payload_digest, "payload digest")?, "payload digest")?,
    );
    let frontier = match (raw.frontier_digest, raw.frontier_payload) {
        (Value::Null, Value::Null) => None,
        (Value::Blob(digest_bytes), Value::Blob(payload)) => {
            Some((digest(&digest_bytes, "frontier digest")?, payload))
        }
        _ => return Err(corrupt("frontier", "digest and payload have inconsistent types")),
    };
    let metadata_digest = match raw.metadata_digest {
        Value::Null => None,
        Value::Blob(bytes) => Some(digest(&bytes, "checkpoint metadata digest")?),
        _ => return Err(corrupt("checkpoint metadata digest", "must be a blob or null")),
    };
    Ok(ActiveGeneration::new(
        generation,
        checkpoint,
        digest(&value_blob(raw.invariant_digest, "invariant digest")?, "invariant digest")?,
        record_count,
        value_blob(raw.payload, "payload")?,
        frontier,
        metadata_digest,
    ))
}

fn value_i64(value: Value, field: &'static str) -> Result<i64, ProjectionError> {
    match value {
        Value::Integer(value) => Ok(value),
        _ => Err(corrupt(field, "must be an integer")),
    }
}

fn value_blob(value: Value, field: &'static str) -> Result<Vec<u8>, ProjectionError> {
    match value {
        Value::Blob(value) => Ok(value),
        _ => Err(corrupt(field, "must be a blob")),
    }
}

fn value_positive_generation(value: &Value) -> Option<CatalogGeneration> {
    let Value::Integer(value) = value else {
        return None;
    };
    u64::try_from(*value).ok().and_then(|value| CatalogGeneration::from_u64(value).ok())
}

fn encode_raw_generation(identity: &ProjectionIdentity, raw: &RawGeneration) -> Vec<u8> {
    let mut bytes = b"peritus-projection-raw-catalog-v1\0".to_vec();
    put_raw_bytes(&mut bytes, identity.name().as_str().as_bytes());
    bytes.extend_from_slice(&identity.version().get().to_be_bytes());
    for value in [
        &raw.catalog_generation,
        &raw.generation,
        &raw.last_position,
        &raw.journal_head,
        &raw.payload_digest,
        &raw.schema_digest,
        &raw.invariant_digest,
        &raw.record_count,
        &raw.payload,
        &raw.frontier_digest,
        &raw.frontier_payload,
        &raw.metadata_digest,
    ] {
        match value {
            Value::Null => bytes.push(0),
            Value::Integer(value) => {
                bytes.push(1);
                bytes.extend_from_slice(&value.to_be_bytes());
            }
            Value::Real(value) => {
                bytes.push(2);
                bytes.extend_from_slice(&value.to_bits().to_be_bytes());
            }
            Value::Text(value) => {
                bytes.push(3);
                put_raw_bytes(&mut bytes, value.as_bytes());
            }
            Value::Blob(value) => {
                bytes.push(4);
                put_raw_bytes(&mut bytes, value);
            }
        }
    }
    bytes
}

fn put_raw_bytes(output: &mut Vec<u8>, value: &[u8]) {
    output.extend_from_slice(&u64::try_from(value.len()).unwrap_or(u64::MAX).to_be_bytes());
    output.extend_from_slice(value);
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

pub(super) fn nonnegative_u64(value: i64, field: &'static str) -> Result<u64, ProjectionError> {
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
