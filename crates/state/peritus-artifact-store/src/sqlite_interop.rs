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
          WHERE digest = ?1 AND finalization_state = 2
            AND quarantine_state = 1 AND integrity_state = 1)",
        [digest.as_bytes().as_slice()],
        |row| row.get(0),
    )
}

/// Iteratively inserts idempotent flat references for one root and its complete dependency graph.
///
/// Each expansion writes at most 256 rows. A rowid keyset walks the final reference rows, which are
/// also the traversal set, so no recursive Rust stack or separately allocated whole-closure
/// collection is required. Returns `false` if any reachable artifact is absent, inactive, corrupt,
/// or has inconsistent child metadata; the caller must then roll back the transaction.
///
/// # Errors
///
/// Returns the underlying `SQLite` failure.
pub fn insert_reference(
    transaction: &Transaction<'_>,
    owner: ReferenceOwner,
    digest: ArtifactDigest,
) -> rusqlite::Result<bool> {
    if !is_referenceable(transaction, digest)? {
        return Ok(false);
    }
    transaction.execute(
        "INSERT OR IGNORE INTO artifact_references(
            owner_kind, owner_identity, artifact_digest
         ) VALUES (?1, ?2, ?3)",
        params![
            owner.kind().database_tag(),
            owner.identity().as_bytes().as_slice(),
            digest.as_bytes().as_slice(),
        ],
    )?;
    let mut after_rowid = 0_i64;
    loop {
        let Some(page_end) = reference_page_end(transaction, &owner, after_rowid)? else {
            transaction.execute(
                "DELETE FROM artifact_collection_candidates
                  WHERE artifact_digest IN (
                    SELECT artifact_digest FROM artifact_references
                     WHERE owner_kind = ?1 AND owner_identity = ?2
                  )",
                params![
                    owner.kind().database_tag(),
                    owner.identity().as_bytes().as_slice(),
                ],
            )?;
            return Ok(true);
        };
        if !reference_page_is_valid(transaction, &owner, after_rowid, page_end)? {
            return Ok(false);
        }
        expand_reference_page(transaction, &owner, after_rowid, page_end)?;
        after_rowid = page_end;
    }
}

fn reference_page_end(
    transaction: &Transaction<'_>,
    owner: &ReferenceOwner,
    after_rowid: i64,
) -> rusqlite::Result<Option<i64>> {
    transaction.query_row(
        "SELECT max(rowid) FROM (
            SELECT rowid
              FROM artifact_references
             WHERE owner_kind = ?1 AND owner_identity = ?2 AND rowid > ?3
             ORDER BY rowid LIMIT 256
         )",
        params![
            owner.kind().database_tag(),
            owner.identity().as_bytes().as_slice(),
            after_rowid,
        ],
        |row| row.get(0),
    )
}

fn reference_page_is_valid(
    transaction: &Transaction<'_>,
    owner: &ReferenceOwner,
    after_rowid: i64,
    page_end: i64,
) -> rusqlite::Result<bool> {
    transaction.query_row(
        "SELECT
            NOT EXISTS (
                SELECT 1
                  FROM artifact_references AS reference
                  LEFT JOIN artifact_records AS record
                    ON record.digest = reference.artifact_digest
                 WHERE reference.owner_kind = ?1 AND reference.owner_identity = ?2
                   AND reference.rowid > ?3 AND reference.rowid <= ?4
                   AND (record.digest IS NULL OR record.finalization_state != 2
                     OR record.quarantine_state != 1 OR record.integrity_state != 1)
            )
            AND NOT EXISTS (
                SELECT 1
                  FROM artifact_references AS reference
                  JOIN artifact_bundles AS bundle
                    ON bundle.parent_digest = reference.artifact_digest
                 WHERE reference.owner_kind = ?1 AND reference.owner_identity = ?2
                   AND reference.rowid > ?3 AND reference.rowid <= ?4
                   AND (bundle.child_count != (
                           SELECT count(*) FROM artifact_dependencies AS dependency
                            WHERE dependency.parent_digest = bundle.parent_digest
                       )
                     OR bundle.child_count != COALESCE((
                           SELECT max(dependency.child_index) + 1
                             FROM artifact_dependencies AS dependency
                            WHERE dependency.parent_digest = bundle.parent_digest
                       ), 0))
            )
            AND NOT EXISTS (
                SELECT 1
                  FROM artifact_references AS reference
                  JOIN artifact_dependencies AS dependency
                    ON dependency.parent_digest = reference.artifact_digest
                  LEFT JOIN artifact_records AS child
                    ON child.digest = dependency.child_digest
                 WHERE reference.owner_kind = ?1 AND reference.owner_identity = ?2
                   AND reference.rowid > ?3 AND reference.rowid <= ?4
                   AND (child.digest IS NULL OR child.size != dependency.child_size
                     OR child.finalization_state != 2 OR child.quarantine_state != 1
                     OR child.integrity_state != 1)
            )",
        params![
            owner.kind().database_tag(),
            owner.identity().as_bytes().as_slice(),
            after_rowid,
            page_end,
        ],
        |row| row.get(0),
    )
}

fn expand_reference_page(
    transaction: &Transaction<'_>,
    owner: &ReferenceOwner,
    after_rowid: i64,
    page_end: i64,
) -> rusqlite::Result<()> {
    loop {
        let inserted = transaction.execute(
            "INSERT OR IGNORE INTO artifact_references(
                owner_kind, owner_identity, artifact_digest
             )
             SELECT ?1, ?2, dependency.child_digest
               FROM artifact_references AS parent
               JOIN artifact_dependencies AS dependency
                 ON dependency.parent_digest = parent.artifact_digest
               LEFT JOIN artifact_references AS child
                 ON child.owner_kind = parent.owner_kind
                AND child.owner_identity = parent.owner_identity
                AND child.artifact_digest = dependency.child_digest
              WHERE parent.owner_kind = ?1 AND parent.owner_identity = ?2
                AND parent.rowid > ?3 AND parent.rowid <= ?4
                AND child.artifact_digest IS NULL
              ORDER BY parent.rowid, dependency.child_index LIMIT 256",
            params![
                owner.kind().database_tag(),
                owner.identity().as_bytes().as_slice(),
                after_rowid,
                page_end,
            ],
        )?;
        if inserted == 0 {
            return Ok(());
        }
    }
}
