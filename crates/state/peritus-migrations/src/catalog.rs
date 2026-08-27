//! Migration-owned `SQLite` schema and typed durable queries.

use peritus_types::Sha256Digest;
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::{
    MigrationError, MigrationErrorCode, MigrationOperationId, MigrationPlan, MigrationRegistry,
    MigrationVersion, RecoveryClass, RecoveryOperation, RecoveryState, recovery::corrupt,
};

const INSTALL: &str = r"
CREATE TABLE IF NOT EXISTS schema_migrations (
    version INTEGER PRIMARY KEY NOT NULL CHECK(version > 0),
    source_digest BLOB NOT NULL CHECK(length(source_digest) = 32),
    release TEXT NOT NULL CHECK(length(release) BETWEEN 1 AND 128),
    applied_operation BLOB NOT NULL CHECK(length(applied_operation) = 16)
) STRICT;
CREATE TABLE IF NOT EXISTS recovery_operations (
    operation_id BLOB PRIMARY KEY NOT NULL CHECK(length(operation_id) = 16),
    from_version INTEGER NOT NULL CHECK(from_version >= 0),
    target_version INTEGER NOT NULL CHECK(target_version > 0),
    registry_digest BLOB NOT NULL CHECK(length(registry_digest) = 32),
    backup_required INTEGER NOT NULL CHECK(backup_required IN (0, 1)),
    backup_digest BLOB CHECK(backup_digest IS NULL OR length(backup_digest) = 32),
    state INTEGER NOT NULL CHECK(state BETWEEN 1 AND 7),
    application_release TEXT NOT NULL CHECK(length(application_release) BETWEEN 1 AND 128),
    failure_code TEXT
) STRICT;
";

pub fn install(connection: &Connection) -> Result<(), MigrationError> {
    connection
        .execute_batch(INSTALL)
        .map_err(|error| MigrationError::sqlite("install migration-owned schema", error))
}

pub fn current_version(
    connection: &Connection,
    registry: MigrationRegistry,
) -> Result<u64, MigrationError> {
    registry.validate()?;
    let mut statement = connection
        .prepare("SELECT version, source_digest, release FROM schema_migrations ORDER BY version")
        .map_err(|error| MigrationError::sqlite("read applied migrations", error))?;
    let mut rows = statement
        .query([])
        .map_err(|error| MigrationError::sqlite("query applied migrations", error))?;
    let mut current = 0_u64;
    while let Some(row) =
        rows.next().map_err(|error| MigrationError::sqlite("iterate applied migrations", error))?
    {
        let version: i64 =
            row.get(0).map_err(|error| MigrationError::sqlite("decode applied version", error))?;
        let version = u64::try_from(version).map_err(|_| corrupt("negative applied version"))?;
        let expected = registry
            .descriptors()
            .get(
                usize::try_from(
                    version.checked_sub(1).ok_or_else(|| corrupt("zero applied version"))?,
                )
                .map_err(|_| corrupt("applied version index overflow"))?,
            )
            .ok_or_else(|| {
                MigrationError::message(
                    MigrationErrorCode::UnsupportedVersion,
                    RecoveryClass::Terminal,
                    "validate applied migrations",
                    "database contains a migration newer than this binary",
                )
            })?;
        let stored_digest: Vec<u8> =
            row.get(1).map_err(|error| MigrationError::sqlite("decode applied digest", error))?;
        let stored_release: String =
            row.get(2).map_err(|error| MigrationError::sqlite("decode applied release", error))?;
        if version != current.checked_add(1).ok_or_else(|| corrupt("version overflow"))?
            || bytes::<32>(&stored_digest)? != *expected.source_digest().as_bytes()
            || stored_release != expected.release()
        {
            return Err(MigrationError::message(
                MigrationErrorCode::DigestDrift,
                RecoveryClass::Terminal,
                "validate applied migrations",
                "applied migration history differs from compiled registry",
            ));
        }
        current = version;
    }
    drop(rows);
    drop(statement);
    let user_version: i64 =
        connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(|error| MigrationError::sqlite("read SQLite user version", error))?;
    if u64::try_from(user_version).map_err(|_| corrupt("negative SQLite user version"))? != current
    {
        return Err(corrupt("SQLite user version disagrees with applied migration history"));
    }
    Ok(current)
}

pub fn adopt_current_install(
    connection: &mut Connection,
    registry: MigrationRegistry,
    operation: MigrationOperationId,
) -> Result<bool, MigrationError> {
    registry.validate()?;
    let applied: i64 = connection
        .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| row.get(0))
        .map_err(|error| MigrationError::sqlite("count applied migrations", error))?;
    if applied != 0 {
        return Ok(false);
    }
    let pending: i64 = connection
        .query_row("SELECT COUNT(*) FROM recovery_operations", [], |row| row.get(0))
        .map_err(|error| MigrationError::sqlite("count migration recovery records", error))?;
    if pending != 0 {
        return Err(corrupt("current schema without migration history has recovery records"));
    }
    let latest = registry.latest()?.get();
    let user_version: i64 =
        connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(|error| MigrationError::sqlite("read SQLite user version", error))?;
    if u64::try_from(user_version).map_err(|_| corrupt("negative SQLite user version"))? != latest {
        return Ok(false);
    }
    let installed: Option<i64> = connection
        .query_row("SELECT schema_version FROM store_meta WHERE singleton = 1", [], |row| {
            row.get(0)
        })
        .optional()
        .map_err(|error| MigrationError::sqlite("observe installed schema version", error))?;
    if installed.and_then(|value| u64::try_from(value).ok()) != Some(latest) {
        return Err(corrupt("current SQLite version lacks matching journal schema metadata"));
    }
    let transaction = connection
        .transaction()
        .map_err(|error| MigrationError::sqlite("begin current-schema adoption", error))?;
    for descriptor in registry.descriptors() {
        record_step(&transaction, operation, *descriptor)?;
    }
    transaction
        .commit()
        .map_err(|error| MigrationError::sqlite("commit current-schema adoption", error))?;
    current_version(connection, registry)?;
    Ok(true)
}

pub fn begin_operation(
    connection: &Connection,
    operation: MigrationOperationId,
    plan: &MigrationPlan,
    release: &str,
) -> Result<RecoveryOperation, MigrationError> {
    connection
        .execute(
            "INSERT OR IGNORE INTO recovery_operations(
            operation_id, from_version, target_version, registry_digest, backup_required,
            backup_digest, state, application_release, failure_code
         ) VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?7, NULL)",
            params![
                operation.as_bytes().as_slice(),
                sqlite(plan.current_version())?,
                sqlite(plan.target_version().get())?,
                plan.registry_digest().as_bytes().as_slice(),
                i64::from(plan.backup_required()),
                RecoveryState::Planned.tag(),
                release,
            ],
        )
        .map_err(|error| MigrationError::sqlite("record migration operation", error))?;
    let stored = load_operation(connection, operation)?
        .ok_or_else(|| corrupt("migration operation vanished after insertion"))?;
    let same_from = stored.from_version() == plan.current_version();
    let same_target = stored.target_version() == plan.target_version();
    let same_registry = stored.registry_digest() == plan.registry_digest();
    let same_backup_policy = stored.backup_required() == plan.backup_required();
    if !(same_from && same_target && same_registry && same_backup_policy) {
        return Err(MigrationError::message(
            MigrationErrorCode::RecoveryRequired,
            RecoveryClass::Reconcile,
            "reuse migration operation",
            "operation identity is already bound to a different plan",
        ));
    }
    Ok(stored)
}

pub fn update_state(
    connection: &Connection,
    operation: MigrationOperationId,
    state: RecoveryState,
    backup_digest: Option<Sha256Digest>,
    failure_code: Option<&str>,
) -> Result<(), MigrationError> {
    let changed = connection
        .execute(
            "UPDATE recovery_operations
            SET state = ?2, backup_digest = COALESCE(?3, backup_digest), failure_code = ?4
          WHERE operation_id = ?1",
            params![
                operation.as_bytes().as_slice(),
                state.tag(),
                backup_digest.map(|digest| digest.into_bytes().to_vec()),
                failure_code,
            ],
        )
        .map_err(|error| MigrationError::sqlite("update migration recovery state", error))?;
    if changed != 1 {
        return Err(corrupt("migration operation is missing"));
    }
    Ok(())
}

pub fn record_step(
    transaction: &Transaction<'_>,
    operation: MigrationOperationId,
    descriptor: crate::MigrationDescriptor,
) -> Result<(), MigrationError> {
    transaction
        .execute(
            "INSERT INTO schema_migrations(version, source_digest, release, applied_operation)
         VALUES (?1, ?2, ?3, ?4)",
            params![
                sqlite(descriptor.version().get())?,
                descriptor.source_digest().as_bytes().as_slice(),
                descriptor.release(),
                operation.as_bytes().as_slice(),
            ],
        )
        .map_err(MigrationError::migration_sql)?;
    Ok(())
}

pub fn load_operation(
    connection: &Connection,
    operation: MigrationOperationId,
) -> Result<Option<RecoveryOperation>, MigrationError> {
    connection
        .query_row(
            "SELECT from_version, target_version, registry_digest, backup_required,
                backup_digest, state
           FROM recovery_operations WHERE operation_id = ?1",
            [operation.as_bytes().as_slice()],
            move |row| {
                let registry: Vec<u8> = row.get(2)?;
                build_operation(
                    operation,
                    row.get(0)?,
                    row.get(1)?,
                    &registry,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                )
            },
        )
        .optional()
        .map_err(|error| MigrationError::sqlite("load recovery operation", error))
}

pub fn pending_operations(
    connection: &Connection,
) -> Result<Vec<RecoveryOperation>, MigrationError> {
    let mut statement = connection
        .prepare(
            "SELECT operation_id, from_version, target_version, registry_digest, backup_required,
                backup_digest, state
           FROM recovery_operations WHERE state IN (1, 2, 3, 5, 7)
          ORDER BY operation_id",
        )
        .map_err(|error| MigrationError::sqlite("prepare recovery scan", error))?;
    let rows = statement
        .query_map([], |row| {
            let raw_id: Vec<u8> = row.get(0)?;
            let id_bytes: [u8; 16] =
                raw_id.as_slice().try_into().map_err(|_| rusqlite::Error::InvalidQuery)?;
            let id =
                MigrationOperationId::new(id_bytes).map_err(|_| rusqlite::Error::InvalidQuery)?;
            let registry: Vec<u8> = row.get(3)?;
            build_operation(
                id,
                row.get(1)?,
                row.get(2)?,
                &registry,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
            )
        })
        .map_err(|error| MigrationError::sqlite("query recovery scan", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| MigrationError::sqlite("decode recovery scan", error))
}

fn build_operation(
    id: MigrationOperationId,
    from: i64,
    target: i64,
    registry: &[u8],
    backup_required: i64,
    backup_digest: Option<Vec<u8>>,
    state: i64,
) -> rusqlite::Result<RecoveryOperation> {
    Ok(RecoveryOperation::new(
        id,
        u64::try_from(from).map_err(|_| rusqlite::Error::InvalidQuery)?,
        MigrationVersion::new(u64::try_from(target).map_err(|_| rusqlite::Error::InvalidQuery)?)
            .map_err(|_| rusqlite::Error::InvalidQuery)?,
        Sha256Digest::new(bytes::<32>(registry).map_err(|_| rusqlite::Error::InvalidQuery)?),
        match backup_required {
            0 => false,
            1 => true,
            _ => return Err(rusqlite::Error::InvalidQuery),
        },
        backup_digest
            .map(|value| bytes::<32>(&value).map(Sha256Digest::new))
            .transpose()
            .map_err(|_| rusqlite::Error::InvalidQuery)?,
        RecoveryState::from_tag(state).map_err(|_| rusqlite::Error::InvalidQuery)?,
    ))
}

fn bytes<const N: usize>(value: &[u8]) -> Result<[u8; N], MigrationError> {
    value.try_into().map_err(|_| corrupt("stored fixed-width field has invalid length"))
}

fn sqlite(value: u64) -> Result<i64, MigrationError> {
    i64::try_from(value).map_err(|_| corrupt("version exceeds SQLite integer"))
}
