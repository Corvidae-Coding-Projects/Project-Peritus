//! SQLite-backed durable artifact metadata and references.

pub mod schema;
mod value;

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use peritus_types::EventId;

use crate::{
    ArtifactDigest, ArtifactMetadata, ArtifactReferenceSet, ArtifactRepairReason,
    ArtifactStoreError, CatalogFailure, ContainedLayoutNamespace, ErrorCode, FinalizationState,
    GcInventoryEntry, IntegrityState, QuarantineState, RecoveryClass, ReferenceOwner,
    ReferenceRoots,
};
use value::{
    RawMetadata, array, corrupt_catalog, decode_quarantine, encode_quarantine, missing_artifact,
    sqlite_integer,
};

pub struct Catalog {
    connection: Connection,
}

const MAX_BUNDLE_CHILDREN: usize = 256;

impl Catalog {
    pub(crate) fn read_only(path: &Path) -> Result<Self, ArtifactStoreError> {
        let connection = Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(catalog_error)?;
        crate::contention::configure(&connection)?;
        connection
            .set_db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)
            .map_err(catalog_error)?;
        connection.pragma_update(None, "trusted_schema", false).map_err(catalog_error)?;
        Ok(Self { connection })
    }

    pub(crate) fn open(path: &Path) -> Result<Self, ArtifactStoreError> {
        let connection = Connection::open(path).map_err(catalog_error)?;
        crate::contention::configure(&connection)?;
        connection.pragma_update(None, "journal_mode", "WAL").map_err(catalog_error)?;
        connection.pragma_update(None, "synchronous", "FULL").map_err(catalog_error)?;
        connection.pragma_update(None, "foreign_keys", true).map_err(catalog_error)?;
        crate::sqlite_interop::install_schema(&connection).map_err(catalog_error)?;
        Ok(Self { connection })
    }

    pub(crate) fn allocate_operation_identity(&self) -> Result<u64, ArtifactStoreError> {
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
                .map_err(catalog_error)?;
        let current: i64 = transaction
            .query_row(
                "SELECT last_identity FROM artifact_operation_sequence WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .map_err(catalog_error)?;
        let next = current.checked_add(1).ok_or_else(|| {
            ArtifactStoreError::message(
                ErrorCode::ArithmeticOverflow,
                RecoveryClass::RecoverStore,
                "artifact operation identity space is exhausted",
            )
        })?;
        let changed = transaction
            .execute(
                "UPDATE artifact_operation_sequence SET last_identity = ?1
                  WHERE singleton = 1 AND last_identity = ?2",
                params![next, current],
            )
            .map_err(catalog_error)?;
        if changed != 1 {
            return Err(corrupt_catalog("artifact operation identity changed concurrently"));
        }
        transaction.commit().map_err(catalog_error)?;
        u64::try_from(next)
            .map_err(|_| corrupt_catalog("artifact operation identity is negative"))
    }

    pub(crate) fn reserve_publication(
        &self,
        metadata: &ArtifactMetadata,
        quota_limit: Option<u64>,
    ) -> Result<(), ArtifactStoreError> {
        if metadata.finalization() != FinalizationState::Partial
            || metadata.quarantine() != QuarantineState::Active
            || metadata.integrity() != IntegrityState::Healthy
        {
            return Err(corrupt_catalog("publication reservation has an invalid durable state"));
        }
        let size = sqlite_integer(metadata.size())?;
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
                .map_err(catalog_error)?;
        if let Some(existing) = metadata_in(&transaction, metadata.digest())? {
            match existing.finalization() {
                FinalizationState::Finalized if same_physical_artifact(&existing, metadata) => {}
                FinalizationState::Partial
                    if same_physical_artifact(&existing, metadata)
                        && existing.quarantine() == QuarantineState::Active => {}
                _ => {
                    return Err(corrupt_catalog(
                        "durable publication reservation disagrees with accepted metadata",
                    ));
                }
            }
            if existing.quarantine() == QuarantineState::Active {
                transaction
                    .execute(
                        "DELETE FROM artifact_collection_candidates WHERE artifact_digest = ?1",
                        [metadata.digest().as_bytes().as_slice()],
                    )
                    .map_err(catalog_error)?;
            }
            transaction.commit().map_err(catalog_error)?;
            return Ok(());
        }
        enforce_quota(&transaction, metadata.size(), quota_limit)?;
        let (algorithm, key_reference, parameters_digest) = encryption_fields(metadata);
        transaction
            .execute(
                "INSERT INTO artifact_records (
                    digest, size, media_type, encryption_algorithm, encryption_key_reference,
                    encryption_parameters_digest, finalization_state, creating_event,
                    quarantine_state, quarantine_generation
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, 1, NULL)",
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
            .map_err(catalog_error)?;
        transaction.commit().map_err(catalog_error)?;
        Ok(())
    }

    pub(crate) fn record_finalized(
        &self,
        metadata: &ArtifactMetadata,
        quota_limit: Option<u64>,
    ) -> Result<bool, ArtifactStoreError> {
        let size = sqlite_integer(metadata.size())?;
        let (algorithm, key_reference, parameters_digest) = encryption_fields(metadata);
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
                .map_err(catalog_error)?;
        if let Some(existing) = metadata_in(&transaction, metadata.digest())? {
            if !same_physical_artifact(&existing, metadata) {
                return Err(corrupt_catalog("durable artifact metadata disagrees with finalized content"));
            }
            if existing.finalization() == FinalizationState::Partial {
                if existing.quarantine() != QuarantineState::Active {
                    return Err(corrupt_catalog(
                        "durable publication reservation disagrees with finalized content",
                    ));
                }
                transaction
                    .execute(
                        "UPDATE artifact_records SET finalization_state = 2 WHERE digest = ?1 AND finalization_state = 1",
                        [metadata.digest().as_bytes().as_slice()],
                    )
                    .map_err(catalog_error)?;
            }
            let restored = matches!(existing.quarantine(), QuarantineState::Quarantined { .. });
            let repaired = existing.integrity() == IntegrityState::Corrupt;
            if restored || repaired {
                transaction
                    .execute(
                        "UPDATE artifact_records
                        SET quarantine_state = 1, quarantine_generation = NULL,
                            integrity_state = 1
                      WHERE digest = ?1",
                        [metadata.digest().as_bytes().as_slice()],
                    )
                    .map_err(catalog_error)?;
            }
            transaction
                .execute(
                    "DELETE FROM artifact_repair_obligations WHERE artifact_digest = ?1",
                    [metadata.digest().as_bytes().as_slice()],
                )
                .map_err(catalog_error)?;
            transaction
                .execute(
                    "DELETE FROM artifact_collection_candidates WHERE artifact_digest = ?1",
                    [metadata.digest().as_bytes().as_slice()],
                )
                .map_err(catalog_error)?;
            transaction.commit().map_err(catalog_error)?;
            return Ok(restored || repaired);
        }
        enforce_quota(&transaction, metadata.size(), quota_limit)?;
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
            .map_err(catalog_error)?;
        transaction.commit().map_err(catalog_error)?;
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
                    quarantine_state, quarantine_generation, integrity_state
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
                        integrity: row.get(9)?,
                    })
                },
            )
            .optional()
            .map_err(catalog_io)?;
        row.map(|raw| raw.validate(digest)).transpose()
    }

    pub(crate) fn bind_dependencies<I>(
        &self,
        parent: ArtifactDigest,
        children: I,
    ) -> Result<(), ArtifactStoreError>
    where
        I: Clone + ExactSizeIterator<Item = (ArtifactDigest, u64)>,
    {
        validate_bundle_request(parent, &children)?;
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
                .map_err(catalog_error)?;
        let metadata = metadata_in(&transaction, parent)?.ok_or_else(missing_artifact)?;
        require_referenceable(&metadata, "artifact bundle parent is not finalized and active")?;
        validate_bundle_children(&transaction, children.clone())?;

        let existing_count: Option<i64> = transaction
            .query_row(
                "SELECT child_count FROM artifact_bundles WHERE parent_digest = ?1",
                [parent.as_bytes().as_slice()],
                |row| row.get(0),
            )
            .optional()
            .map_err(catalog_error)?;
        if let Some(existing_count) = existing_count {
            verify_existing_bundle(&transaction, parent, existing_count, children)?;
            transaction.commit().map_err(catalog_error)?;
            return Ok(());
        }

        let referenced: bool = transaction
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM artifact_references WHERE artifact_digest = ?1
                )",
                [parent.as_bytes().as_slice()],
                |row| row.get(0),
            )
            .map_err(catalog_error)?;
        if referenced {
            return Err(invalid_bundle(
                "a referenced artifact cannot acquire dependency edges",
            ));
        }

        let is_child: bool = transaction
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM artifact_dependencies WHERE child_digest = ?1
                )",
                [parent.as_bytes().as_slice()],
                |row| row.get(0),
            )
            .map_err(catalog_error)?;
        if is_child {
            return Err(invalid_bundle(
                "an artifact already used as a child cannot acquire dependency edges",
            ));
        }
        insert_bundle(&transaction, parent, children)?;
        transaction.commit().map_err(catalog_error)
    }

    pub(crate) fn partial_metadata_after(
        &self,
        after: Option<ArtifactDigest>,
        maximum: usize,
    ) -> Result<Vec<ArtifactMetadata>, ArtifactStoreError> {
        self.metadata_after_where(after, maximum, "finalization_state = 1")
    }

    pub(crate) fn finalized_metadata_after(
        &self,
        after: Option<ArtifactDigest>,
        maximum: usize,
    ) -> Result<Vec<ArtifactMetadata>, ArtifactStoreError> {
        self.metadata_after_where(after, maximum, "finalization_state = 2")
    }

    pub(crate) fn repair_obligations_after(
        &self,
        after: Option<ArtifactDigest>,
        maximum: usize,
    ) -> Result<Vec<(ArtifactMetadata, ArtifactRepairReason)>, ArtifactStoreError> {
        validate_page(maximum)?;
        let mut statement = self
            .connection
            .prepare(
                "SELECT artifact_digest, reason FROM artifact_repair_obligations
                  WHERE artifact_digest > ?1 ORDER BY artifact_digest LIMIT ?2",
            )
            .map_err(catalog_io)?;
        let rows = statement
            .query_map(
                params![
                    after.map_or_else(Vec::new, |digest| digest.as_bytes().to_vec()),
                    i64::try_from(maximum).map_err(|_| {
                        corrupt_catalog("artifact repair-obligation page cannot be represented")
                    })?
                ],
                |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?)),
            )
            .map_err(catalog_io)?;
        let mut raw = Vec::new();
        for row in rows {
            raw.push(row.map_err(catalog_io)?);
        }
        drop(statement);
        let mut obligations = Vec::with_capacity(raw.len());
        for (digest, reason) in raw {
            let digest = ArtifactDigest::new(array::<32>(&digest)?);
            let metadata = self.metadata(digest)?.ok_or_else(|| {
                corrupt_catalog("artifact repair obligation has no durable metadata")
            })?;
            if metadata.finalization() != FinalizationState::Finalized
                || metadata.integrity() != IntegrityState::Corrupt
            {
                return Err(corrupt_catalog(
                    "artifact repair obligation points to referenceable metadata",
                ));
            }
            obligations.push((metadata, decode_repair_reason(reason)?));
        }
        Ok(obligations)
    }

    pub(crate) fn record_repair_obligation(
        &self,
        digest: ArtifactDigest,
        reason: ArtifactRepairReason,
    ) -> Result<ArtifactRepairReason, ArtifactStoreError> {
        self.connection
            .execute(
                "INSERT INTO artifact_repair_obligations(artifact_digest, reason)
                 VALUES (?1, ?2)
                 ON CONFLICT(artifact_digest) DO UPDATE SET
                    reason = MAX(artifact_repair_obligations.reason, excluded.reason)",
                params![digest.as_bytes().as_slice(), encode_repair_reason(reason)],
            )
            .map_err(catalog_io)?;
        let persisted: i64 = self
            .connection
            .query_row(
                "SELECT reason FROM artifact_repair_obligations WHERE artifact_digest = ?1",
                [digest.as_bytes().as_slice()],
                |row| row.get(0),
            )
            .map_err(catalog_io)?;
        decode_repair_reason(persisted)
    }

    pub(crate) fn fence_finalized_repair(
        &self,
        digest: ArtifactDigest,
        observed_reason: ArtifactRepairReason,
    ) -> Result<ArtifactRepairReason, ArtifactStoreError> {
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
                .map_err(catalog_error)?;
        let changed = transaction
            .execute(
                "UPDATE artifact_records SET integrity_state = 2
                  WHERE digest = ?1 AND finalization_state = 2",
                [digest.as_bytes().as_slice()],
            )
            .map_err(catalog_error)?;
        if changed != 1 {
            return Err(corrupt_catalog(
                "repair fence does not identify one finalized artifact",
            ));
        }
        transaction
            .execute(
                "INSERT INTO artifact_repair_obligations(artifact_digest, reason)
                 VALUES (?1, ?2)
                 ON CONFLICT(artifact_digest) DO UPDATE SET
                    reason = MAX(artifact_repair_obligations.reason, excluded.reason)",
                params![
                    digest.as_bytes().as_slice(),
                    encode_repair_reason(observed_reason)
                ],
            )
            .map_err(catalog_error)?;
        let persisted: i64 = transaction
            .query_row(
                "SELECT reason FROM artifact_repair_obligations WHERE artifact_digest = ?1",
                [digest.as_bytes().as_slice()],
                |row| row.get(0),
            )
            .map_err(catalog_error)?;
        let persisted = decode_repair_reason(persisted)?;
        transaction.commit().map_err(catalog_error)?;
        Ok(persisted)
    }

    pub(crate) fn record_repair_containment(
        &self,
        digest: ArtifactDigest,
        identity: u64,
        namespace: ContainedLayoutNamespace,
    ) -> Result<Option<bool>, ArtifactStoreError> {
        let identity = sqlite_integer(identity)?;
        let namespace = namespace.database_tag().ok_or_else(|| {
            corrupt_catalog("temporary layout cannot contain durable artifact content")
        })?;
        let changed = self
            .connection
            .execute(
                "INSERT OR IGNORE INTO artifact_repair_containments(
                    operation_identity, artifact_digest, namespace
                 )
                 SELECT ?1, ?2, ?3
                  WHERE EXISTS (
                    SELECT 1 FROM artifact_repair_obligations WHERE artifact_digest = ?2
                  )",
                params![identity, digest.as_bytes().as_slice(), namespace],
            )
            .map_err(catalog_io)?;
        let persisted: Option<(Vec<u8>, i64)> = self
            .connection
            .query_row(
                "SELECT artifact_digest, namespace FROM artifact_repair_containments
                  WHERE operation_identity = ?1",
                [identity],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(catalog_io)?;
        let Some(persisted) = persisted else {
            return Ok(None);
        };
        if persisted.0.as_slice() != digest.as_bytes() || persisted.1 != namespace {
            return Err(corrupt_catalog(
                "artifact repair containment identity has conflicting provenance",
            ));
        }
        Ok(Some(changed == 1))
    }

    fn metadata_after_where(
        &self,
        after: Option<ArtifactDigest>,
        maximum: usize,
        condition: &'static str,
    ) -> Result<Vec<ArtifactMetadata>, ArtifactStoreError> {
        validate_page(maximum)?;
        let query = format!(
            "SELECT digest, size, media_type, encryption_algorithm, encryption_key_reference,
                encryption_parameters_digest, finalization_state, creating_event,
                quarantine_state, quarantine_generation, integrity_state
               FROM artifact_records
              WHERE {condition} AND digest > ?1
              ORDER BY digest LIMIT ?2"
        );
        let mut statement = self
            .connection
            .prepare(&query)
            .map_err(catalog_io)?;
        let rows = statement
            .query_map(
                params![
                    after.map_or_else(Vec::new, |digest| digest.as_bytes().to_vec()),
                    i64::try_from(maximum).map_err(|_| {
                        corrupt_catalog(
                            "partial-publication recovery page cannot be represented",
                        )
                    })?
                ],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        RawMetadata {
                            size: row.get(1)?,
                            media_type: row.get(2)?,
                            algorithm: row.get(3)?,
                            key_reference: row.get(4)?,
                            parameters_digest: row.get(5)?,
                            finalization: row.get(6)?,
                            creating_event: row.get(7)?,
                            quarantine: row.get(8)?,
                            quarantine_generation: row.get(9)?,
                            integrity: row.get(10)?,
                        },
                    ))
                },
            )
            .map_err(catalog_io)?;
        let mut metadata = Vec::new();
        for row in rows {
            let (digest, raw) = row.map_err(catalog_io)?;
            metadata.push(raw.validate(ArtifactDigest::new(array::<32>(&digest)?))?);
        }
        Ok(metadata)
    }

    pub(crate) fn delete_partial(
        &self,
        digest: ArtifactDigest,
    ) -> Result<(), ArtifactStoreError> {
        let changed = self
            .connection
            .execute(
                "DELETE FROM artifact_records WHERE digest = ?1 AND finalization_state = 1",
                [digest.as_bytes().as_slice()],
            )
            .map_err(catalog_io)?;
        if changed != 1 {
            return Err(corrupt_catalog("publication reservation changed during recovery"));
        }
        Ok(())
    }

    pub(crate) fn add_reference(
        &self,
        owner: ReferenceOwner,
        digest: ArtifactDigest,
    ) -> Result<(), ArtifactStoreError> {
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
                .map_err(catalog_io)?;
        if !crate::sqlite_interop::insert_reference(&transaction, owner, digest)
            .map_err(catalog_io)?
        {
            return Err(ArtifactStoreError::message(
                ErrorCode::InvalidCollectionPlan,
                RecoveryClass::CorrectRequest,
                "only a complete finalized active artifact graph may be referenced",
            ));
        }
        transaction.commit().map_err(catalog_io)
    }

    pub(crate) fn remove_reference(
        &self,
        owner: ReferenceOwner,
        digest: ArtifactDigest,
    ) -> Result<bool, ArtifactStoreError> {
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
                .map_err(catalog_io)?;
        let changed = transaction
            .execute(
                "DELETE FROM artifact_references
              WHERE owner_kind = ?1 AND owner_identity = ?2 AND artifact_digest = ?3",
                params![
                    owner.kind().database_tag(),
                    owner.identity().as_bytes().as_slice(),
                    digest.as_bytes().as_slice(),
                ],
            )
            .map_err(catalog_io)?;
        if changed != 0 {
            transaction
                .execute(
                    "INSERT OR IGNORE INTO artifact_collection_candidates(artifact_digest)
                     SELECT ?1 WHERE NOT EXISTS (
                        SELECT 1 FROM artifact_references WHERE artifact_digest = ?1
                     )",
                    [digest.as_bytes().as_slice()],
                )
                .map_err(catalog_io)?;
        }
        transaction.commit().map_err(catalog_io)?;
        Ok(changed != 0)
    }

    pub(crate) fn retire_reference_owner(
        &self,
        owner: ReferenceOwner,
    ) -> Result<u64, ArtifactStoreError> {
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
                .map_err(catalog_io)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO artifact_collection_candidates(artifact_digest)
                 SELECT retiring.artifact_digest
                   FROM artifact_references AS retiring
                  WHERE retiring.owner_kind = ?1 AND retiring.owner_identity = ?2
                    AND NOT EXISTS (
                        SELECT 1 FROM artifact_references AS retained
                         WHERE retained.artifact_digest = retiring.artifact_digest
                           AND (retained.owner_kind != ?1 OR retained.owner_identity != ?2)
                    )",
                params![owner.kind().database_tag(), owner.identity().as_bytes().as_slice()],
            )
            .map_err(catalog_io)?;
        let changed = transaction
            .execute(
                "DELETE FROM artifact_references
                  WHERE owner_kind = ?1 AND owner_identity = ?2",
                params![owner.kind().database_tag(), owner.identity().as_bytes().as_slice()],
            )
            .map_err(catalog_io)?;
        transaction.commit().map_err(catalog_io)?;
        u64::try_from(changed).map_err(|_| {
            corrupt_catalog("retired artifact reference count cannot be represented")
        })
    }

    pub(crate) fn migrate_reference_owner(
        &self,
        previous: ReferenceOwner,
        replacement: ReferenceOwner,
    ) -> Result<u64, ArtifactStoreError> {
        if previous == replacement {
            return Ok(0);
        }
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
                .map_err(catalog_io)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO artifact_references(
                    owner_kind, owner_identity, artifact_digest
                 )
                 SELECT ?3, ?4, artifact_digest
                   FROM artifact_references
                  WHERE owner_kind = ?1 AND owner_identity = ?2",
                params![
                    previous.kind().database_tag(),
                    previous.identity().as_bytes().as_slice(),
                    replacement.kind().database_tag(),
                    replacement.identity().as_bytes().as_slice(),
                ],
            )
            .map_err(catalog_io)?;
        transaction
            .execute(
                "DELETE FROM artifact_collection_candidates
                  WHERE artifact_digest IN (
                    SELECT artifact_digest FROM artifact_references
                     WHERE owner_kind = ?1 AND owner_identity = ?2
                  )",
                params![
                    replacement.kind().database_tag(),
                    replacement.identity().as_bytes().as_slice(),
                ],
            )
            .map_err(catalog_io)?;
        let changed = transaction
            .execute(
                "DELETE FROM artifact_references
                  WHERE owner_kind = ?1 AND owner_identity = ?2",
                params![
                    previous.kind().database_tag(),
                    previous.identity().as_bytes().as_slice(),
                ],
            )
            .map_err(catalog_io)?;
        transaction.commit().map_err(catalog_io)?;
        u64::try_from(changed).map_err(|_| {
            corrupt_catalog("migrated artifact reference count cannot be represented")
        })
    }

    pub(crate) fn release_publication(
        &self,
        digest: ArtifactDigest,
        creating_event: EventId,
    ) -> Result<bool, ArtifactStoreError> {
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
                .map_err(catalog_io)?;
        let metadata = metadata_in(&transaction, digest)?.ok_or_else(missing_artifact)?;
        if !metadata.is_referenceable() || metadata.creating_event() != creating_event {
            return Err(ArtifactStoreError::message(
                ErrorCode::InvalidCollectionPlan,
                RecoveryClass::CorrectRequest,
                "publication release does not match finalized active ownership",
            ));
        }
        let referenced: bool = transaction
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM artifact_references WHERE artifact_digest = ?1
                )",
                [digest.as_bytes().as_slice()],
                |row| row.get(0),
            )
            .map_err(catalog_io)?;
        if referenced {
            return Err(ArtifactStoreError::message(
                ErrorCode::InvalidCollectionPlan,
                RecoveryClass::CorrectRequest,
                "referenced publication ownership must be retired by exact owner",
            ));
        }
        let changed = transaction
            .execute(
                "INSERT OR IGNORE INTO artifact_collection_candidates(artifact_digest)
                 VALUES (?1)",
                [digest.as_bytes().as_slice()],
            )
            .map_err(catalog_io)?;
        transaction.commit().map_err(catalog_io)?;
        Ok(changed != 0)
    }

    pub(crate) fn has_references(
        &self,
        digest: ArtifactDigest,
    ) -> Result<bool, ArtifactStoreError> {
        self.connection
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM artifact_references WHERE artifact_digest = ?1
                )",
                [digest.as_bytes().as_slice()],
                |row| row.get(0),
            )
            .map_err(catalog_io)
    }

    pub(crate) fn reference_owners_after(
        &self,
        digest: ArtifactDigest,
        after: Option<ReferenceOwner>,
        maximum: usize,
    ) -> Result<Vec<ReferenceOwner>, ArtifactStoreError> {
        validate_page(maximum)?;
        let (after_kind, after_identity) = after.map_or_else(
            || (0_i64, Vec::new()),
            |owner| {
                (
                    owner.kind().database_tag(),
                    owner.identity().as_bytes().to_vec(),
                )
            },
        );
        let mut statement = self
            .connection
            .prepare(
                "SELECT owner_kind, owner_identity FROM artifact_references
                  WHERE artifact_digest = ?1
                    AND (owner_kind > ?2 OR (owner_kind = ?2 AND owner_identity > ?3))
                  ORDER BY owner_kind, owner_identity LIMIT ?4",
            )
            .map_err(catalog_io)?;
        let rows = statement
            .query_map(
                params![
                    digest.as_bytes().as_slice(),
                    after_kind,
                    after_identity,
                    i64::try_from(maximum).map_err(|_| {
                        corrupt_catalog("artifact reference-owner page cannot be represented")
                    })?
                ],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )
            .map_err(catalog_io)?;
        let mut owners = Vec::new();
        for row in rows {
            let (kind, identity) = row.map_err(catalog_io)?;
            let identity = peritus_types::Sha256Digest::new(array::<32>(&identity)?);
            owners.push(match kind {
                1 => ReferenceOwner::journal(identity),
                2 => ReferenceOwner::evidence(identity),
                _ => return Err(corrupt_catalog("unknown artifact reference owner kind")),
            });
        }
        Ok(owners)
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
                "SELECT record.digest, record.size, record.quarantine_state,
                    record.quarantine_generation, record.integrity_state,
                    candidate.artifact_digest IS NOT NULL
               FROM artifact_records AS record
               LEFT JOIN artifact_collection_candidates AS candidate
                 ON candidate.artifact_digest = record.digest
              WHERE record.finalization_state = 2
              ORDER BY record.digest",
            )
            .map_err(catalog_io)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, bool>(5)?,
                ))
            })
            .map_err(catalog_io)?;
        let mut inventory = Vec::new();
        for row in rows {
            let (digest, size, state, generation, integrity, collection_eligible) =
                row.map_err(catalog_io)?;
            let quarantine = decode_quarantine(state, generation)?;
            let repair_required = match integrity {
                1 => false,
                2 => true,
                _ => return Err(corrupt_catalog("unknown artifact integrity state")),
            };
            inventory.push(GcInventoryEntry::with_collection_state(
                ArtifactDigest::new(array::<32>(&digest)?),
                u64::try_from(size).map_err(|_| corrupt_catalog("negative artifact size"))?,
                quarantine,
                collection_eligible,
                repair_required,
            ));
        }
        Ok(inventory)
    }

    pub(crate) fn repair_containments(
        &self,
        digest: ArtifactDigest,
    ) -> Result<Vec<(u64, ContainedLayoutNamespace)>, ArtifactStoreError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT operation_identity, namespace
                   FROM artifact_repair_containments
                  WHERE artifact_digest = ?1
                  ORDER BY operation_identity",
            )
            .map_err(catalog_io)?;
        let rows = statement
            .query_map([digest.as_bytes().as_slice()], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(catalog_io)?;
        let mut containments = Vec::new();
        for row in rows {
            let (identity, namespace) = row.map_err(catalog_io)?;
            let identity = u64::try_from(identity)
                .map_err(|_| corrupt_catalog("negative repair containment identity"))?;
            let namespace = match namespace {
                1 => ContainedLayoutNamespace::Objects,
                2 => ContainedLayoutNamespace::Quarantine,
                _ => return Err(corrupt_catalog("unknown repair containment namespace")),
            };
            containments.push((identity, namespace));
        }
        Ok(containments)
    }

    pub(crate) fn quota_bytes(&self) -> Result<(u64, u64), ArtifactStoreError> {
        checked_quota_bytes(&self.connection)
    }

    pub(crate) fn set_quarantine(
        &self,
        digest: ArtifactDigest,
        state: QuarantineState,
    ) -> Result<(), ArtifactStoreError> {
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
                .map_err(catalog_io)?;
        let referenced: i64 = transaction
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM artifact_references WHERE artifact_digest = ?1
                )",
                [digest.as_bytes().as_slice()],
                |row| row.get(0),
            )
            .map_err(catalog_io)?;
        if matches!(state, QuarantineState::Quarantined { .. }) && referenced != 0 {
            return Err(ArtifactStoreError::message(
                ErrorCode::InvalidCollectionPlan,
                RecoveryClass::CorrectRequest,
                "a referenced artifact cannot be quarantined",
            ));
        }
        if matches!(state, QuarantineState::Quarantined { .. }) {
            let eligible: bool = transaction
                .query_row(
                    "SELECT EXISTS(
                        SELECT 1 FROM artifact_collection_candidates
                         WHERE artifact_digest = ?1
                    )",
                    [digest.as_bytes().as_slice()],
                    |row| row.get(0),
                )
                .map_err(catalog_io)?;
            if !eligible {
                return Err(ArtifactStoreError::message(
                    ErrorCode::InvalidCollectionPlan,
                    RecoveryClass::CorrectRequest,
                    "collection requires explicitly released publication ownership",
                ));
            }
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
        if state == QuarantineState::Active {
            transaction
                .execute(
                    "DELETE FROM artifact_collection_candidates WHERE artifact_digest = ?1",
                    [digest.as_bytes().as_slice()],
                )
                .map_err(catalog_io)?;
        }
        transaction.commit().map_err(catalog_io)
    }

    pub(crate) fn delete_record(
        &self,
        digest: ArtifactDigest,
    ) -> Result<(), ArtifactStoreError> {
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
                .map_err(catalog_io)?;
        let changed = transaction
            .execute(
                "DELETE FROM artifact_records
              WHERE digest = ?1 AND quarantine_state = 2
                AND EXISTS (
                    SELECT 1 FROM artifact_collection_candidates
                     WHERE artifact_digest = ?1
                )
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

fn metadata_in(
    connection: &Connection,
    digest: ArtifactDigest,
) -> Result<Option<ArtifactMetadata>, ArtifactStoreError> {
    connection
        .query_row(
            "SELECT size, media_type, encryption_algorithm, encryption_key_reference,
                encryption_parameters_digest, finalization_state, creating_event,
                quarantine_state, quarantine_generation, integrity_state
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
                    integrity: row.get(9)?,
                })
            },
        )
        .optional()
        .map_err(catalog_error)?
        .map(|raw| raw.validate(digest))
        .transpose()
}

fn validate_bundle_request<I>(
    parent: ArtifactDigest,
    children: &I,
) -> Result<(), ArtifactStoreError>
where
    I: Clone + ExactSizeIterator<Item = (ArtifactDigest, u64)>,
{
    if children.len() > MAX_BUNDLE_CHILDREN {
        return Err(invalid_bundle("artifact bundle has too many direct children"));
    }
    for (index, (digest, _)) in (*children).clone().enumerate() {
        if digest == parent {
            return Err(invalid_bundle("artifact bundle cannot contain itself"));
        }
        if (*children).clone().take(index).any(|(prior, _)| prior == digest) {
            return Err(invalid_bundle("artifact bundle contains a duplicate child"));
        }
    }
    Ok(())
}

fn validate_bundle_children<I>(
    connection: &Connection,
    children: I,
) -> Result<(), ArtifactStoreError>
where
    I: Iterator<Item = (ArtifactDigest, u64)>,
{
    for (child, size) in children {
        let metadata = metadata_in(connection, child)?.ok_or_else(missing_artifact)?;
        require_referenceable(
            &metadata,
            "artifact bundle child is not finalized and active",
        )?;
        if metadata.size() != size {
            return Err(ArtifactStoreError::mismatch(
                ErrorCode::SizeMismatch,
                metadata.size(),
                size,
            ));
        }
    }
    Ok(())
}

fn insert_bundle<I>(
    connection: &Connection,
    parent: ArtifactDigest,
    children: I,
) -> Result<(), ArtifactStoreError>
where
    I: ExactSizeIterator<Item = (ArtifactDigest, u64)>,
{
    connection
        .execute(
            "INSERT INTO artifact_bundles(parent_digest, child_count) VALUES (?1, ?2)",
            params![
                parent.as_bytes().as_slice(),
                i64::try_from(children.len()).map_err(|_| {
                    invalid_bundle("artifact bundle child count cannot be represented")
                })?
            ],
        )
        .map_err(catalog_error)?;
    for (index, (child, size)) in children.enumerate() {
        connection
            .execute(
                "INSERT INTO artifact_dependencies(
                    parent_digest, child_index, child_digest, child_size
                 ) VALUES (?1, ?2, ?3, ?4)",
                params![
                    parent.as_bytes().as_slice(),
                    i64::try_from(index).map_err(|_| {
                        invalid_bundle("artifact bundle child index cannot be represented")
                    })?,
                    child.as_bytes().as_slice(),
                    sqlite_integer(size)?,
                ],
            )
            .map_err(catalog_error)?;
    }
    Ok(())
}

fn verify_existing_bundle<I>(
    connection: &Connection,
    parent: ArtifactDigest,
    existing_count: i64,
    children: I,
) -> Result<(), ArtifactStoreError>
where
    I: ExactSizeIterator<Item = (ArtifactDigest, u64)>,
{
    let existing_count = usize::try_from(existing_count)
        .map_err(|_| corrupt_catalog("artifact bundle has a negative child count"))?;
    if existing_count != children.len() {
        return Err(invalid_bundle(
            "artifact bundle is already bound to another child list",
        ));
    }
    let stored_count: i64 = connection
        .query_row(
            "SELECT count(*) FROM artifact_dependencies WHERE parent_digest = ?1",
            [parent.as_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(catalog_error)?;
    let stored_count = usize::try_from(stored_count)
        .map_err(|_| corrupt_catalog("artifact bundle dependency count is negative"))?;
    if stored_count != existing_count {
        return Err(corrupt_catalog(
            "artifact bundle child count disagrees with dependency rows",
        ));
    }
    for (index, (child, size)) in children.enumerate() {
        let stored: Option<(Vec<u8>, i64)> = connection
            .query_row(
                "SELECT child_digest, child_size FROM artifact_dependencies
                  WHERE parent_digest = ?1 AND child_index = ?2",
                params![
                    parent.as_bytes().as_slice(),
                    i64::try_from(index).map_err(|_| {
                        invalid_bundle("artifact bundle child index cannot be represented")
                    })?
                ],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(catalog_error)?;
        let expected_size = sqlite_integer(size)?;
        let Some((stored_digest, stored_size)) = stored else {
            return Err(invalid_bundle(
                "artifact bundle is already bound to another child list",
            ));
        };
        if stored_digest.as_slice() != child.as_bytes() {
            return Err(invalid_bundle(
                "artifact bundle is already bound to another child list",
            ));
        }
        if stored_size != expected_size {
            return Err(invalid_bundle(
                "artifact bundle is already bound to another child list",
            ));
        }
    }
    Ok(())
}

const fn require_referenceable(
    metadata: &ArtifactMetadata,
    message: &'static str,
) -> Result<(), ArtifactStoreError> {
    if metadata.is_referenceable() {
        Ok(())
    } else {
        Err(invalid_bundle(message))
    }
}

const fn invalid_bundle(message: &'static str) -> ArtifactStoreError {
    ArtifactStoreError::message(
        ErrorCode::InvalidWriteRequest,
        RecoveryClass::CorrectRequest,
        message,
    )
}

fn encryption_fields(
    metadata: &ArtifactMetadata,
) -> (Option<&str>, Option<peritus_types::Sha256Digest>, Option<peritus_types::Sha256Digest>) {
    metadata.encryption().algorithm().map_or((None, None, None), |algorithm| {
        (
            Some(algorithm),
            metadata.encryption().key_reference(),
            metadata.encryption().parameters_digest(),
        )
    })
}

fn same_physical_artifact(left: &ArtifactMetadata, right: &ArtifactMetadata) -> bool {
    left.digest() == right.digest()
        && left.size() == right.size()
        && left.encryption() == right.encryption()
}

fn enforce_quota(
    connection: &Connection,
    size: u64,
    quota_limit: Option<u64>,
) -> Result<(), ArtifactStoreError> {
    let Some(quota_limit) = quota_limit else {
        return Ok(());
    };
    let (committed, reserved) = checked_quota_bytes(connection)?;
    let attempted = committed
        .checked_add(reserved)
        .and_then(|total| total.checked_add(size))
        .ok_or_else(|| {
        ArtifactStoreError::message(
            ErrorCode::ArithmeticOverflow,
            RecoveryClass::RecoverStore,
            "durable quota accounting overflowed",
        )
    })?;
    if attempted > quota_limit {
        return Err(ArtifactStoreError::limit(ErrorCode::QuotaExceeded, attempted, quota_limit));
    }
    Ok(())
}

const fn validate_page(maximum: usize) -> Result<(), ArtifactStoreError> {
    if maximum == 0 || maximum > 4_096 {
        return Err(ArtifactStoreError::message(
            ErrorCode::InvalidConfiguration,
            RecoveryClass::CorrectRequest,
            "artifact catalog page is outside production bounds",
        ));
    }
    Ok(())
}

const fn encode_repair_reason(reason: ArtifactRepairReason) -> i64 {
    match reason {
        ArtifactRepairReason::Missing => 1,
        ArtifactRepairReason::Corrupt => 2,
    }
}

const fn decode_repair_reason(value: i64) -> Result<ArtifactRepairReason, ArtifactStoreError> {
    match value {
        1 => Ok(ArtifactRepairReason::Missing),
        2 => Ok(ArtifactRepairReason::Corrupt),
        _ => Err(corrupt_catalog("unknown artifact repair-obligation reason")),
    }
}

fn checked_quota_bytes(connection: &Connection) -> Result<(u64, u64), ArtifactStoreError> {
    const PAGE_ROWS: usize = 1_024;
    let mut after = Vec::new();
    let mut committed = 0_u64;
    let mut reserved = 0_u64;
    loop {
        let mut statement = connection
            .prepare(
                "SELECT digest, size, finalization_state FROM artifact_records
                  WHERE digest > ?1 ORDER BY digest LIMIT ?2",
            )
            .map_err(catalog_error)?;
        let rows = statement
            .query_map(
                params![after.as_slice(), i64::try_from(PAGE_ROWS).map_err(|_| {
                    corrupt_catalog("artifact accounting page size cannot be represented")
                })?],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .map_err(catalog_error)?;
        let mut page_rows = 0_usize;
        for row in rows {
            let (digest, size, finalization) = row.map_err(catalog_error)?;
            let _ = array::<32>(&digest)?;
            let size = u64::try_from(size)
                .map_err(|_| corrupt_catalog("artifact accounting contains a negative size"))?;
            let total = match finalization {
                1 => &mut reserved,
                2 => &mut committed,
                _ => return Err(corrupt_catalog("artifact accounting has invalid finalization")),
            };
            *total = total.checked_add(size).ok_or_else(|| {
                ArtifactStoreError::message(
                    ErrorCode::ArithmeticOverflow,
                    RecoveryClass::RecoverStore,
                    "segmented artifact byte accounting overflowed",
                )
            })?;
            after = digest;
            page_rows = page_rows.checked_add(1).ok_or_else(|| {
                corrupt_catalog("artifact accounting page row count overflowed")
            })?;
        }
        if page_rows < PAGE_ROWS {
            return Ok((committed, reserved));
        }
    }
}

pub(super) fn catalog_error(error: rusqlite::Error) -> ArtifactStoreError {
    use rusqlite::ErrorCode as SqliteErrorCode;

    let failure = match error.sqlite_error_code() {
        Some(SqliteErrorCode::DatabaseBusy) => CatalogFailure::Busy,
        Some(SqliteErrorCode::DatabaseLocked) => CatalogFailure::Locked,
        Some(SqliteErrorCode::DiskFull) => CatalogFailure::StorageFull,
        Some(
            SqliteErrorCode::PermissionDenied
            | SqliteErrorCode::AuthorizationForStatementDenied,
        ) => CatalogFailure::PermissionDenied,
        Some(SqliteErrorCode::ReadOnly) => CatalogFailure::ReadOnly,
        Some(SqliteErrorCode::SchemaChanged) => CatalogFailure::SchemaChanged,
        Some(SqliteErrorCode::DatabaseCorrupt | SqliteErrorCode::NotADatabase) => {
            CatalogFailure::Integrity
        }
        _ => CatalogFailure::Other,
    };
    let cancelled = crate::contention::active_wait_cancelled()
        && matches!(failure, CatalogFailure::Busy | CatalogFailure::Locked);
    let (code, recovery) = if cancelled {
        (ErrorCode::CatalogWaitCancelled, RecoveryClass::Retry)
    } else {
        match failure {
            CatalogFailure::Busy => (ErrorCode::CatalogBusy, RecoveryClass::Retry),
            CatalogFailure::Locked => (ErrorCode::CatalogLocked, RecoveryClass::Retry),
            CatalogFailure::StorageFull => (ErrorCode::StoragePressure, RecoveryClass::Retry),
            CatalogFailure::PermissionDenied => {
                (ErrorCode::CatalogPermissionDenied, RecoveryClass::RecoverStore)
            }
            CatalogFailure::ReadOnly => {
                (ErrorCode::CatalogReadOnly, RecoveryClass::RecoverStore)
            }
            CatalogFailure::SchemaChanged => {
                (ErrorCode::CatalogSchemaChanged, RecoveryClass::RecoverStore)
            }
            CatalogFailure::Integrity => {
                (ErrorCode::CatalogIntegrity, RecoveryClass::TerminalIntegrity)
            }
            CatalogFailure::Other => (ErrorCode::Io, RecoveryClass::RecoverStore),
        }
    };
    ArtifactStoreError::catalog(code, recovery, failure, error)
}

fn catalog_io(error: rusqlite::Error) -> ArtifactStoreError {
    catalog_error(error)
}
