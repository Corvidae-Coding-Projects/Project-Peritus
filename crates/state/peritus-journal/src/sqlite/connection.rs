//! `SQLite` connection ownership and hardened configuration.

use std::{path::Path, sync::Arc, time::Duration};

use super::contention::{self, ContentionPolicy, JournalCancellation};
use crate::{JournalError, JournalErrorKind, StoreId};
use rusqlite::{
    Connection, OpenFlags, TransactionBehavior, config::DbConfig, limits::Limit, params,
};

/// `SQLite` connection configuration for a journal owner.
///
/// The default has no elapsed contention deadline. Callers that need to abandon a blocked open
/// use [`SqliteJournal::open_waiting`] with a [`JournalCancellation`] they retain. A finite timeout
/// is available only as an explicit policy for isolated diagnostics and bounded tests.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SqliteJournalOptions {
    contention: ContentionPolicy,
    maximum_pages: Option<u64>,
}

impl Default for SqliteJournalOptions {
    fn default() -> Self {
        Self::native()
    }
}

impl SqliteJournalOptions {
    /// Selects SQLite's native database page ceiling.
    #[must_use]
    pub const fn native() -> Self {
        Self { contention: ContentionPolicy::WaitForCancellation, maximum_pages: None }
    }

    /// Selects an explicit finite contention deadline.
    ///
    /// Production durable owners should normally use [`Self::default`] and establish cancellation
    /// with [`SqliteJournal::open_waiting`]. This constructor exists for callers whose contract
    /// deliberately bounds an isolated attempt, including deterministic contention fixtures.
    #[must_use]
    pub const fn with_timeout(busy_timeout: Duration) -> Self {
        Self { contention: ContentionPolicy::Timeout(busy_timeout), maximum_pages: None }
    }

    /// Applies an explicit positive SQLite database page ceiling whenever the journal opens.
    ///
    /// The value is validated against SQLite's signed representation and the database's current
    /// allocation during open. A ceiling may later be raised or lowered above current allocation
    /// through [`SqliteJournal::limit_storage_pages`].
    #[must_use]
    pub const fn with_maximum_pages(mut self, maximum_pages: u64) -> Self {
        self.maximum_pages = Some(maximum_pages);
        self
    }

    /// Returns the selected page ceiling, or `None` for SQLite's native ceiling.
    #[must_use]
    pub const fn maximum_pages(self) -> Option<u64> {
        self.maximum_pages
    }
}

/// Observed safety-critical `SQLite` settings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SqliteSettings {
    /// Active journal mode, expected to be `wal` for file stores.
    pub journal_mode: String,
    /// `SQLite` synchronous level, expected to be `2` (`FULL`).
    pub synchronous: i64,
    /// Whether foreign-key enforcement is active.
    pub foreign_keys: bool,
    /// Configured busy timeout in milliseconds.
    pub busy_timeout_ms: u64,
    /// Whether defensive connection mode is active.
    pub defensive: bool,
}

/// Current physical page accounting and page ceiling for one authoritative connection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqliteStoragePages {
    page_count: u64,
    page_size: u64,
    maximum_pages: u64,
}

impl SqliteStoragePages {
    /// Returns the pages currently allocated by `SQLite`.
    #[must_use]
    pub const fn page_count(self) -> u64 {
        self.page_count
    }

    /// Returns the configured bytes in one `SQLite` page.
    #[must_use]
    pub const fn page_size(self) -> u64 {
        self.page_size
    }

    /// Returns the `SQLite` page ceiling observed by this connection.
    #[must_use]
    pub const fn maximum_pages(self) -> u64 {
        self.maximum_pages
    }

    /// Returns the maximum database bytes admitted by the current ceiling.
    #[must_use]
    pub const fn maximum_bytes(self) -> u64 {
        self.maximum_pages.saturating_mul(self.page_size)
    }
}

/// Single-owner writable `SQLite` journal.
///
/// Mutating operations require `&mut self`, making the authoritative connection's serialized
/// ownership explicit. Separate values may still exercise `SQLite`'s real stale-CAS behavior.
pub struct SqliteJournal {
    pub(crate) connection: Connection,
    pub(crate) store_id: StoreId,
    pub(crate) replay_generation: Arc<()>,
}

impl SqliteJournal {
    /// Opens or creates a file-backed journal and installs the current schema version.
    ///
    /// # Errors
    ///
    /// Returns typed storage, schema, or store-identity errors.
    pub fn open(
        path: impl AsRef<Path>,
        store_id: StoreId,
        options: SqliteJournalOptions,
    ) -> Result<Self, JournalError> {
        Self::open_configured(path.as_ref(), store_id, options)
    }

    /// Opens a journal whose transient writer contention waits until release or cancellation.
    ///
    /// The supplied token applies to opening and schema reconciliation. Subsequent operations on
    /// the returned journal wait without a deadline unless their caller establishes a different
    /// [`JournalCancellation::run`] scope.
    ///
    /// # Errors
    ///
    /// Returns a typed busy failure when cancellation wins, or the same storage, schema, and
    /// identity failures as [`Self::open`].
    pub fn open_waiting(
        path: impl AsRef<Path>,
        store_id: StoreId,
        cancellation: &JournalCancellation,
    ) -> Result<Self, JournalError> {
        Self::open_waiting_with_options(path, store_id, SqliteJournalOptions::native(), cancellation)
    }

    /// Opens with cancellation-aware contention and an explicit storage-ceiling policy.
    ///
    /// # Errors
    ///
    /// Returns the same failures as [`Self::open_waiting`], including invalid or already-exceeded
    /// page ceilings.
    pub fn open_waiting_with_options(
        path: impl AsRef<Path>,
        store_id: StoreId,
        options: SqliteJournalOptions,
        cancellation: &JournalCancellation,
    ) -> Result<Self, JournalError> {
        cancellation.run(|| {
            Self::open_configured(
                path.as_ref(),
                store_id,
                SqliteJournalOptions {
                    contention: ContentionPolicy::WaitForCancellation,
                    maximum_pages: options.maximum_pages,
                },
            )
        })
    }

    fn open_configured(
        path: &Path,
        store_id: StoreId,
        options: SqliteJournalOptions,
    ) -> Result<Self, JournalError> {
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let mut connection = Connection::open_with_flags(path, flags)
            .map_err(|error| JournalError::sqlite("open journal", error))?;
        configure(&connection, options.contention)?;
        if let Some(maximum_pages) = options.maximum_pages {
            apply_page_ceiling(&connection, maximum_pages)?;
        }
        // Installation and identity/version checks are one transaction: rejecting an older store
        // must not leave new tables behind and prevent its explicit forward migration.
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| JournalError::sqlite("begin journal schema installation", error))?;
        transaction
            .execute_batch(super::schema::INSTALL_SCHEMA)
            .map_err(|error| JournalError::sqlite("install journal schema", error))?;
        peritus_artifact_store::sqlite_interop::install_schema(&transaction)
            .map_err(|error| JournalError::sqlite("install artifact catalog schema", error))?;
        bind_store(&transaction, store_id)?;
        bind_migration_version(&transaction)?;
        transaction
            .commit()
            .map_err(|error| JournalError::sqlite("commit journal schema installation", error))?;
        Ok(Self { connection, store_id, replay_generation: Arc::new(()) })
    }

    /// Returns the journal's exact store identity.
    #[must_use]
    pub const fn store_id(&self) -> StoreId {
        self.store_id
    }

    /// Reads safety-critical connection settings.
    ///
    /// # Errors
    ///
    /// Returns a storage error if `SQLite` cannot report a setting.
    pub fn settings(&self) -> Result<SqliteSettings, JournalError> {
        let journal_mode = self
            .connection
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .map_err(|error| JournalError::sqlite("read journal mode", error))?;
        let synchronous = self
            .connection
            .pragma_query_value(None, "synchronous", |row| row.get(0))
            .map_err(|error| JournalError::sqlite("read synchronous mode", error))?;
        let foreign_keys: i64 = self
            .connection
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .map_err(|error| JournalError::sqlite("read foreign keys", error))?;
        let busy_timeout_ms: i64 = self
            .connection
            .pragma_query_value(None, "busy_timeout", |row| row.get(0))
            .map_err(|error| JournalError::sqlite("read busy timeout", error))?;
        let defensive = self
            .connection
            .db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE)
            .map_err(|error| JournalError::sqlite("read defensive mode", error))?;
        Ok(SqliteSettings {
            journal_mode,
            synchronous,
            foreign_keys: foreign_keys == 1,
            busy_timeout_ms: u64::try_from(busy_timeout_ms).map_err(|_| {
                JournalError::new(
                    JournalErrorKind::CorruptJournal,
                    "read busy timeout",
                    "SQLite returned a negative busy timeout",
                )
            })?,
            defensive,
        })
    }

    /// Reads `SQLite`'s current database size and active page ceiling.
    ///
    /// # Errors
    ///
    /// Returns a typed storage error if `SQLite` cannot report canonical positive values.
    pub fn storage_pages(&self) -> Result<SqliteStoragePages, JournalError> {
        storage_pages(&self.connection)
    }

    /// Adjusts this connection's `SQLite` database page ceiling without shrinking existing storage.
    ///
    /// Future transactions on this connection that require another page fail atomically with
    /// `SQLite`'s `SQLITE_FULL` result. Callers that require a process-lifetime budget must apply
    /// it whenever they establish the owning connection.
    ///
    /// # Errors
    ///
    /// Rejects zero, unrepresentable, or below-current ceilings and returns a typed storage error
    /// when `SQLite` cannot apply or re-observe the requested limit.
    pub fn limit_storage_pages(
        &mut self,
        maximum_pages: u64,
    ) -> Result<SqliteStoragePages, JournalError> {
        apply_page_ceiling(&self.connection, maximum_pages)?;
        let observed = storage_pages(&self.connection)?;
        if observed.maximum_pages != maximum_pages {
            return Err(JournalError::new(
                JournalErrorKind::Storage,
                "configure journal page ceiling",
                "SQLite did not retain the requested page ceiling",
            ));
        }
        Ok(observed)
    }
}

fn apply_page_ceiling(
    connection: &Connection,
    maximum_pages: u64,
) -> Result<(), JournalError> {
    let maximum = i64::try_from(maximum_pages).map_err(|_| invalid_page_limit())?;
    if maximum_pages == 0 {
        return Err(invalid_page_limit());
    }
    let current: i64 = connection
        .pragma_query_value(None, "page_count", |row| row.get(0))
        .map_err(|error| JournalError::sqlite("read journal page count", error))?;
    if current < 0 || u64::try_from(current).ok().is_none_or(|pages| maximum_pages < pages) {
        return Err(invalid_page_limit());
    }
    connection
        .pragma_update(None, "max_page_count", maximum)
        .map_err(|error| JournalError::sqlite("configure journal page ceiling", error))?;
    let observed: i64 = connection
        .pragma_query_value(None, "max_page_count", |row| row.get(0))
        .map_err(|error| JournalError::sqlite("observe journal page ceiling", error))?;
    if u64::try_from(observed).ok() != Some(maximum_pages) {
        return Err(JournalError::new(
            JournalErrorKind::Storage,
            "configure journal page ceiling",
            "SQLite did not retain the requested page ceiling",
        ));
    }
    Ok(())
}

fn storage_pages(connection: &Connection) -> Result<SqliteStoragePages, JournalError> {
    let page_count: i64 = connection
        .pragma_query_value(None, "page_count", |row| row.get(0))
        .map_err(|error| JournalError::sqlite("read journal page count", error))?;
    let page_size: i64 = connection
        .pragma_query_value(None, "page_size", |row| row.get(0))
        .map_err(|error| JournalError::sqlite("read journal page size", error))?;
    let maximum_pages: i64 = connection
        .pragma_query_value(None, "max_page_count", |row| row.get(0))
        .map_err(|error| JournalError::sqlite("read journal page ceiling", error))?;
    Ok(SqliteStoragePages {
        page_count: positive_page_value(page_count)?,
        page_size: positive_page_value(page_size)?,
        maximum_pages: positive_page_value(maximum_pages)?,
    })
}

fn positive_page_value(value: i64) -> Result<u64, JournalError> {
    u64::try_from(value).ok().filter(|value| *value > 0).ok_or_else(|| {
        JournalError::new(
            JournalErrorKind::CorruptJournal,
            "read journal storage pages",
            "SQLite returned a nonpositive storage-page value",
        )
    })
}

const fn invalid_page_limit() -> JournalError {
    JournalError::new(
        JournalErrorKind::InvalidInput,
        "configure journal page ceiling",
        "journal page ceiling is zero, unrepresentable, or below current allocation",
    )
}

fn configure(connection: &Connection, contention: ContentionPolicy) -> Result<(), JournalError> {
    contention::configure(connection, contention)?;
    let mode: String = connection
        .pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))
        .map_err(|error| JournalError::sqlite("configure WAL", error))?;
    if !mode.eq_ignore_ascii_case("wal") {
        return Err(JournalError::new(
            JournalErrorKind::Storage,
            "configure WAL",
            "SQLite did not activate WAL mode",
        ));
    }
    connection
        .pragma_update(None, "synchronous", "FULL")
        .map_err(|error| JournalError::sqlite("configure synchronous FULL", error))?;
    connection
        .pragma_update(None, "foreign_keys", true)
        .map_err(|error| JournalError::sqlite("configure foreign keys", error))?;
    connection
        .set_db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)
        .map_err(|error| JournalError::sqlite("configure defensive mode", error))?;
    connection
        .set_db_config(DbConfig::SQLITE_DBCONFIG_TRUSTED_SCHEMA, false)
        .map_err(|error| JournalError::sqlite("disable trusted schema", error))?;
    connection
        .set_limit(Limit::SQLITE_LIMIT_ATTACHED, 0)
        .map_err(|error| JournalError::sqlite("disable attached databases", error))?;
    Ok(())
}

fn bind_store(connection: &Connection, store_id: StoreId) -> Result<(), JournalError> {
    connection
        .execute(
            "INSERT OR IGNORE INTO store_meta(singleton, store_id, schema_version) VALUES (1, ?1, ?2)",
            params![store_id.as_bytes().as_slice(), super::schema::SCHEMA_VERSION],
        )
        .map_err(|error| JournalError::sqlite("bind store identity", error))?;
    let (stored, mut version): (Vec<u8>, i64) = connection
        .query_row(
            "SELECT store_id, schema_version FROM store_meta WHERE singleton = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| JournalError::sqlite("observe store identity", error))?;
    if stored.as_slice() != store_id.as_bytes() {
        return Err(JournalError::new(
            JournalErrorKind::InvalidInput,
            "open journal",
            "store identity does not match the existing database",
        ));
    }
    if version < super::schema::SCHEMA_VERSION {
        let migration_owned: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = 'schema_migrations')",
            [], |row| row.get(0),
        ).map_err(|error| JournalError::sqlite("inspect migration ownership", error))?;
        if migration_owned {
            return Err(JournalError::new(
                JournalErrorKind::UnsupportedSchema,
                "open journal",
                "run the application migration owner before opening this journal",
            ));
        }
    }
    if matches!(version, 1 | 2) {
        // Rebuild the exact application-artifact catalog under exclusive ownership. Version 3
        // removes only the legacy media-text ceiling; all existing identities and receipts must
        // cross the replacement frontier byte-for-byte before the previous table is retired.
        connection
            .execute_batch("ALTER TABLE app_artifacts RENAME TO app_artifacts_previous;")
            .map_err(|error| JournalError::sqlite("begin artifact metadata migration", error))?;
        connection
            .execute_batch(super::schema::INSTALL_SCHEMA)
            .map_err(|error| JournalError::sqlite("install artifact metadata schema", error))?;
        connection
            .execute_batch(
                "INSERT INTO app_artifacts(
                    artifact_id, digest, byte_size, media_type, state, producing_position
                 )
                 SELECT artifact_id, digest, byte_size, media_type, state, producing_position
                   FROM app_artifacts_previous;",
            )
            .map_err(|error| JournalError::sqlite("copy artifact metadata", error))?;
        let replacement_complete: bool = connection
            .query_row(
                "SELECT
                    NOT EXISTS(
                        SELECT artifact_id, digest, byte_size, media_type, state, producing_position
                          FROM app_artifacts_previous
                        EXCEPT
                        SELECT artifact_id, digest, byte_size, media_type, state, producing_position
                          FROM app_artifacts
                    )
                    AND NOT EXISTS(
                        SELECT artifact_id, digest, byte_size, media_type, state, producing_position
                          FROM app_artifacts
                        EXCEPT
                        SELECT artifact_id, digest, byte_size, media_type, state, producing_position
                          FROM app_artifacts_previous
                    )",
                [],
                |row| row.get(0),
            )
            .map_err(|error| JournalError::sqlite("verify artifact metadata migration", error))?;
        if !replacement_complete {
            return Err(JournalError::new(
                JournalErrorKind::CorruptJournal,
                "migrate artifact metadata",
                "replacement artifact metadata frontier is incomplete",
            ));
        }
        connection
            .execute_batch(
                "DROP TABLE app_artifacts_previous;
                 UPDATE store_meta SET schema_version = 3 WHERE singleton = 1;",
            )
            .map_err(|error| JournalError::sqlite("publish artifact metadata migration", error))?;
        version = 3;
    }
    if version == 3 {
        // Version four keeps every accepted legacy inline decoder but makes all new large journal
        // content use bounded physical chunks. Nullable reference columns are the unambiguous
        // representation discriminator; no existing bytes or canonical digests are rewritten.
        connection
            .execute_batch(
                "ALTER TABLE events ADD COLUMN frame_byte_length INTEGER
                    CHECK (frame_byte_length BETWEEN 1 AND 1073741823);
                 ALTER TABLE state_records ADD COLUMN root_digest BLOB
                    CHECK (root_digest IS NULL OR length(root_digest) = 32);
                 ALTER TABLE state_records ADD COLUMN value_bytes INTEGER
                    CHECK (value_bytes BETWEEN 0 AND 16777216);
                 ALTER TABLE outbox ADD COLUMN payload_digest BLOB
                    CHECK (payload_digest IS NULL OR length(payload_digest) = 32);
                 ALTER TABLE outbox ADD COLUMN payload_byte_length INTEGER
                    CHECK (payload_byte_length BETWEEN 0 AND 16777216);
                 ALTER TABLE credential_registry ADD COLUMN snapshot_byte_length INTEGER
                    CHECK (snapshot_byte_length BETWEEN 1 AND 1073741823);
                 ALTER TABLE credential_registry ADD COLUMN snapshot_content_digest BLOB
                    CHECK (snapshot_content_digest IS NULL
                        OR length(snapshot_content_digest) = 32);
                 ALTER TABLE app_commands ADD COLUMN envelope_digest BLOB
                    CHECK (envelope_digest IS NULL OR length(envelope_digest) = 32);
                 ALTER TABLE app_commands ADD COLUMN envelope_byte_length INTEGER
                    CHECK (envelope_byte_length BETWEEN 1 AND 1073741823);
                 ALTER TABLE app_commands ADD COLUMN domain_command_byte_length INTEGER
                    CHECK (domain_command_byte_length BETWEEN 1 AND 1073741823);
                 ALTER TABLE app_prompt_targets ADD COLUMN binding_byte_length INTEGER
                    CHECK (binding_byte_length BETWEEN 1 AND 16777216);
                 ALTER TABLE app_prompt_targets ADD COLUMN settlement_byte_length INTEGER
                    CHECK (settlement_byte_length BETWEEN 1 AND 16777216);
                 ALTER TABLE app_workspaces ADD COLUMN registration_byte_length INTEGER
                    CHECK (registration_byte_length BETWEEN 1 AND 1048576);
                 UPDATE store_meta SET schema_version = 4 WHERE singleton = 1;",
            )
            .map_err(|error| JournalError::sqlite("publish paged-content schema migration", error))?;
        version = 4;
    }
    if version == 4 {
        // Version five separates transport delivery from semantic retry budgets. Existing
        // debugger model/report rows retain their exact identity, payload, producing position,
        // attempt count, and fence; unacknowledged exhausted rows become reclaimable in place.
        connection
            .execute_batch(
                "ALTER TABLE outbox ADD COLUMN persistent INTEGER NOT NULL DEFAULT 0
                    CHECK (persistent IN (0, 1));
                 UPDATE outbox
                    SET persistent = 1,
                        state = CASE WHEN state = 4 THEN 1 ELSE state END
                  WHERE destination IN (
                    'peritus.debugger.model-analysis.v1',
                    'peritus.debugger.publish-report.v1'
                  );
                 UPDATE store_meta SET schema_version = 5 WHERE singleton = 1;",
            )
            .map_err(|error| {
                JournalError::sqlite("publish persistent-outbox schema migration", error)
            })?;
        version = 5;
    }
    if version == 5 {
        // Version six extends persistent transport delivery to every E3 effect lane. Existing
        // schedule, execution, and publication rows retain their exact semantic identity and
        // claim history; unacknowledged legacy exhaustion becomes pending in place.
        connection
            .execute_batch(
                "UPDATE outbox
                    SET persistent = 1,
                        state = CASE WHEN state = 4 THEN 1 ELSE state END
                  WHERE destination IN (
                    'peritus.eval.schedule-rollout.v1',
                    'peritus.eval.execute-rollout.v1',
                    'peritus.eval.publish-report.v1'
                  );
                 UPDATE store_meta SET schema_version = 6 WHERE singleton = 1;",
            )
            .map_err(|error| {
                JournalError::sqlite("publish evaluation persistent-outbox migration", error)
            })?;
        version = 6;
    }
    if version == 6 {
        // Version seven extends persistent transport delivery to F0 publication directives.
        // Existing campaign-decision and harness-activation rows retain their exact identity,
        // payload, producing position, attempt count, and fence; unacknowledged legacy
        // exhaustion becomes pending in place.
        connection
            .execute_batch(
                "UPDATE outbox
                    SET persistent = 1,
                        state = CASE WHEN state = 4 THEN 1 ELSE state END
                  WHERE destination = 'peritus.evolution.publish.v1';
                 UPDATE store_meta SET schema_version = 7 WHERE singleton = 1;",
            )
            .map_err(|error| {
                JournalError::sqlite("publish evolution persistent-outbox migration", error)
            })?;
        version = 7;
    }
    if version != super::schema::SCHEMA_VERSION {
        return Err(JournalError::new(
            JournalErrorKind::UnsupportedSchema,
            "open journal",
            "database schema version is unsupported",
        ));
    }
    Ok(())
}

fn bind_migration_version(connection: &Connection) -> Result<(), JournalError> {
    connection
        .pragma_update(None, "user_version", super::schema::SCHEMA_VERSION)
        .map_err(|error| JournalError::sqlite("bind migration schema version", error))?;
    let observed: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|error| JournalError::sqlite("observe migration schema version", error))?;
    if observed != super::schema::SCHEMA_VERSION {
        return Err(JournalError::new(
            JournalErrorKind::CorruptJournal,
            "bind migration schema version",
            "SQLite user_version does not match the installed journal schema",
        ));
    }
    Ok(())
}
