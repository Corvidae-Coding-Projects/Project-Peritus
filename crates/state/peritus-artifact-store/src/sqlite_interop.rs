//! Shared SQLite schema and transaction operations.
//!
//! These functions contain no filesystem behavior and make no authority decision. They let the
//! journal install the artifact catalog and atomically add references in its own transaction.

use rusqlite::{Connection, Transaction, params};

use crate::{ArtifactDigest, ReferenceOwner, catalog::schema::SCHEMA};

/// Installs the idempotent artifact catalog schema on an existing connection.
///
/// # Errors
///
/// Returns the underlying `SQLite` failure.
pub fn install_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(SCHEMA)
}

/// Observes whether exact finalized, active artifact metadata exists in this transaction.
///
/// # Errors
///
/// Returns the underlying `SQLite` failure.
pub fn is_referenceable(
    transaction: &Transaction<'_>,
    digest: ArtifactDigest,
) -> rusqlite::Result<bool> {
    transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM artifact_records
          WHERE digest = ?1 AND finalization_state = 2 AND quarantine_state = 1)",
        [digest.as_bytes().as_slice()],
        |row| row.get(0),
    )
}

/// Inserts an idempotent reference owned by a committed journal or evidence record.
///
/// The caller must first check [`is_referenceable`] in the same transaction.
///
/// # Errors
///
/// Returns the underlying `SQLite` failure.
pub fn insert_reference(
    transaction: &Transaction<'_>,
    owner: ReferenceOwner,
    digest: ArtifactDigest,
) -> rusqlite::Result<()> {
    transaction.execute(
        "INSERT OR IGNORE INTO artifact_references(owner_kind, owner_identity, artifact_digest)
         VALUES (?1, ?2, ?3)",
        params![
            owner.kind().database_tag(),
            owner.identity().as_bytes().as_slice(),
            digest.as_bytes().as_slice(),
        ],
    )?;
    Ok(())
}
