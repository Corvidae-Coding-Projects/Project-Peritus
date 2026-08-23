//! SQLite-backed durable artifact metadata and references.

pub mod schema;
mod value;

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

use crate::{
    ArtifactDigest, ArtifactMetadata, ArtifactReferenceSet, ArtifactStoreError, ErrorCode,
    GcInventoryEntry, QuarantineState, RecoveryClass, ReferenceOwner, ReferenceRoots,
    StoreOperation,
};
use value::{
    RawMetadata, array, corrupt_catalog, decode_quarantine, encode_quarantine, missing_artifact,
    sqlite_integer,
};

pub struct Catalog {
    connection: Connection,
}

impl Catalog {
    pub(crate) fn open(path: &Path) -> Result<Self, ArtifactStoreError> {
        let connection = Connection::open(path).map_err(catalog_io)?;
        connection.busy_timeout(std::time::Duration::from_secs(5)).map_err(catalog_io)?;
        connection.pragma_update(None, "journal_mode", "WAL").map_err(catalog_io)?;
        connection.pragma_update(None, "synchronous", "FULL").map_err(catalog_io)?;
        connection.pragma_update(None, "foreign_keys", true).map_err(catalog_io)?;
        crate::sqlite_interop::install_schema(&connection).map_err(catalog_io)?;
        Ok(Self { connection })
    }

    pub(crate) fn record_finalized(
        &self,
        metadata: &ArtifactMetadata,
        quota_limit: u64,
    ) -> Result<bool, ArtifactStoreError> {
        let size = sqlite_integer(metadata.size())?;
        let (algorithm, key_reference, parameters_digest) =
            metadata.encryption().algorithm().map_or((None, None, None), |algorithm| {
                (
                    Some(algorithm),
                    metadata.encryption().key_reference(),
                    metadata.encryption().parameters_digest(),
                )
            });
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
                .map_err(catalog_io)?;
        let existing = transaction
            .query_row(
                "SELECT size, finalization_state, quarantine_state
               FROM artifact_records WHERE digest = ?1",
                [metadata.digest().as_bytes().as_slice()],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?)),
            )
            .optional()
            .map_err(catalog_io)?;
        if let Some((existing_size, finalization, quarantine)) = existing {
            if existing_size != size || finalization != 2 {
                return Err(corrupt_catalog(
                    "durable artifact metadata disagrees with finalized content",
                ));
            }
            let restored = quarantine == 2;
            if restored {
                transaction
                    .execute(
                        "UPDATE artifact_records
                        SET quarantine_state = 1, quarantine_generation = NULL
                      WHERE digest = ?1",
                        [metadata.digest().as_bytes().as_slice()],
                    )
                    .map_err(catalog_io)?;
            } else if quarantine != 1 {
                return Err(corrupt_catalog("unknown durable quarantine state"));
            }
            transaction.commit().map_err(catalog_io)?;
            return Ok(restored);
        }
        let used: i64 = transaction
            .query_row("SELECT COALESCE(SUM(size), 0) FROM artifact_records", [], |row| row.get(0))
            .map_err(catalog_io)?;
        let used =
            u64::try_from(used).map_err(|_| corrupt_catalog("invalid durable quota total"))?;
        let attempted = used.checked_add(metadata.size()).ok_or_else(|| {
            ArtifactStoreError::message(
                ErrorCode::ArithmeticOverflow,
                RecoveryClass::RecoverStore,
                "durable quota accounting overflowed",
            )
        })?;
        if attempted > quota_limit {
            return Err(ArtifactStoreError::limit(
                ErrorCode::QuotaExceeded,
                attempted,
                quota_limit,
            ));
        }
        transaction
            .execute(
                "INSERT INTO artifact_records (
                digest, size, media_type, encryption_algorithm, encryption_key_reference,
                encryption_parameters_digest, finalization_state, creating_event,
                quarantine_state, quarantine_generation
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 2, ?7, 1, NULL)",
                params![
                    metadata.digest().as_bytes().as_slice(),
                    size,
                    metadata.media_type().as_str(),
                    algorithm,
                    key_reference.map(|digest| digest.into_bytes().to_vec()),
                    parameters_digest.map(|digest| digest.into_bytes().to_vec()),
                    metadata.creating_event().as_bytes().as_slice(),
                ],
            )
            .map_err(catalog_io)?;
        transaction.commit().map_err(catalog_io)?;
        Ok(false)
    }

    pub(crate) fn metadata(
        &self,
        digest: ArtifactDigest,
    ) -> Result<Option<ArtifactMetadata>, ArtifactStoreError> {
        let row = self
            .connection
            .query_row(
                "SELECT size, media_type, encryption_algorithm, encryption_key_reference,
                    encryption_parameters_digest, finalization_state, creating_event,
                    quarantine_state, quarantine_generation
               FROM artifact_records WHERE digest = ?1",
                [digest.as_bytes().as_slice()],
                |row| {
                    Ok(RawMetadata {
                        size: row.get(0)?,
                        media_type: row.get(1)?,
                        algorithm: row.get(2)?,
                        key_reference: row.get(3)?,
                        parameters_digest: row.get(4)?,
                        finalization: row.get(5)?,
                        creating_event: row.get(6)?,
                        quarantine: row.get(7)?,
                        quarantine_generation: row.get(8)?,
                    })
                },
            )
            .optional()
            .map_err(catalog_io)?;
        row.map(|raw| raw.validate(digest)).transpose()
    }

    pub(crate) fn add_reference(
        &self,
        owner: ReferenceOwner,
        digest: ArtifactDigest,
    ) -> Result<(), ArtifactStoreError> {
        let metadata = self.metadata(digest)?.ok_or_else(missing_artifact)?;
        if !metadata.is_referenceable() {
            return Err(ArtifactStoreError::message(
                ErrorCode::InvalidCollectionPlan,
                RecoveryClass::CorrectRequest,
                "only finalized active artifacts may be referenced",
            ));
        }
        self.connection.execute(
            "INSERT OR IGNORE INTO artifact_references(owner_kind, owner_identity, artifact_digest)
             VALUES (?1, ?2, ?3)",
            params![
                owner.kind().database_tag(),
                owner.identity().as_bytes().as_slice(),
                digest.as_bytes().as_slice(),
            ],
        ).map_err(catalog_io)?;
        Ok(())
    }

    pub(crate) fn remove_reference(
        &self,
        owner: ReferenceOwner,
        digest: ArtifactDigest,
    ) -> Result<bool, ArtifactStoreError> {
        self.connection
            .execute(
                "DELETE FROM artifact_references
              WHERE owner_kind = ?1 AND owner_identity = ?2 AND artifact_digest = ?3",
                params![
                    owner.kind().database_tag(),
                    owner.identity().as_bytes().as_slice(),
                    digest.as_bytes().as_slice(),
                ],
            )
            .map(|changed| changed != 0)
            .map_err(catalog_io)
    }

    pub(crate) fn roots(&self) -> Result<ReferenceRoots, ArtifactStoreError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT owner_kind, artifact_digest FROM artifact_references
              ORDER BY artifact_digest, owner_kind",
            )
            .map_err(catalog_io)?;
        let rows = statement
            .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?)))
            .map_err(catalog_io)?;
        let mut journal = ArtifactReferenceSet::new();
        let mut evidence = ArtifactReferenceSet::new();
        for row in rows {
            let (kind, bytes) = row.map_err(catalog_io)?;
            let digest = ArtifactDigest::new(array::<32>(&bytes)?);
            match kind {
                1 => {
                    journal.insert(digest);
                }
                2 => {
                    evidence.insert(digest);
                }
                _ => return Err(corrupt_catalog("unknown artifact reference owner kind")),
            }
        }
        Ok(ReferenceRoots::new(journal, evidence))
    }

    pub(crate) fn inventory(&self) -> Result<Vec<GcInventoryEntry>, ArtifactStoreError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT digest, size, quarantine_state, quarantine_generation
               FROM artifact_records WHERE finalization_state = 2 ORDER BY digest",
            )
            .map_err(catalog_io)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                ))
            })
            .map_err(catalog_io)?;
        let mut inventory = Vec::new();
        for row in rows {
            let (digest, size, state, generation) = row.map_err(catalog_io)?;
            let quarantine = decode_quarantine(state, generation)?;
            inventory.push(GcInventoryEntry::new(
                ArtifactDigest::new(array::<32>(&digest)?),
                u64::try_from(size).map_err(|_| corrupt_catalog("negative artifact size"))?,
                quarantine,
            ));
        }
        Ok(inventory)
    }

    pub(crate) fn used_bytes(&self) -> Result<u64, ArtifactStoreError> {
        let used: i64 = self
            .connection
            .query_row("SELECT COALESCE(SUM(size), 0) FROM artifact_records", [], |row| row.get(0))
            .map_err(catalog_io)?;
        u64::try_from(used).map_err(|_| corrupt_catalog("invalid durable quota total"))
    }

    pub(crate) fn set_quarantine(
        &mut self,
        digest: ArtifactDigest,
        state: QuarantineState,
    ) -> Result<(), ArtifactStoreError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(catalog_io)?;
        let reference_count: i64 = transaction
            .query_row(
                "SELECT count(*) FROM artifact_references WHERE artifact_digest = ?1",
                [digest.as_bytes().as_slice()],
                |row| row.get(0),
            )
            .map_err(catalog_io)?;
        if matches!(state, QuarantineState::Quarantined { .. }) && reference_count != 0 {
            return Err(ArtifactStoreError::message(
                ErrorCode::InvalidCollectionPlan,
                RecoveryClass::CorrectRequest,
                "a referenced artifact cannot be quarantined",
            ));
        }
        let (tag, generation) = encode_quarantine(state)?;
        let changed = transaction
            .execute(
                "UPDATE artifact_records SET quarantine_state = ?2, quarantine_generation = ?3
              WHERE digest = ?1 AND finalization_state = 2",
                params![digest.as_bytes().as_slice(), tag, generation],
            )
            .map_err(catalog_io)?;
        if changed != 1 {
            return Err(missing_artifact());
        }
        transaction.commit().map_err(catalog_io)
    }

    pub(crate) fn delete_record(
        &mut self,
        digest: ArtifactDigest,
    ) -> Result<(), ArtifactStoreError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(catalog_io)?;
        let changed = transaction
            .execute(
                "DELETE FROM artifact_records
              WHERE digest = ?1 AND quarantine_state = 2
                AND NOT EXISTS (
                    SELECT 1 FROM artifact_references WHERE artifact_digest = ?1
                )",
                [digest.as_bytes().as_slice()],
            )
            .map_err(catalog_io)?;
        if changed != 1 {
            return Err(ArtifactStoreError::message(
                ErrorCode::InvalidCollectionPlan,
                RecoveryClass::CorrectRequest,
                "sweep requires an unreferenced quarantined durable record",
            ));
        }
        transaction.commit().map_err(catalog_io)
    }
}

fn catalog_io(error: rusqlite::Error) -> ArtifactStoreError {
    ArtifactStoreError::io(StoreOperation::Initialize, std::io::Error::other(error))
}
