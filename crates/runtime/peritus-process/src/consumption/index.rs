//! Bounded SQLite projection of the authoritative claim, manifest, and tombstone files.
//!
//! This private index is rebuilt one record at a time on open. Its cache and query pages are
//! physical allocations, never limits on the number of accepted or unresolved executions.

use std::path::{Path, PathBuf};

use peritus_types::ProcessId;
use rusqlite::{Connection, OptionalExtension as _, params};
use tempfile::NamedTempFile;

use crate::{
    ProcessError,
    native::observation::NativeObservationState,
    recovery::{claim::ConsumptionClaim, manifest::ExecutionManifest},
    registry_storage::{
        load_canonical_tombstone, load_claim, load_manifest, load_quarantine,
        load_quarantined_identity, quarantine_path, validate_native_observation_frontier,
        visit_quarantined_identities, visit_registry_identities,
    },
};

use super::{store_cause, store_error};

mod ownership;
use ownership::{IndexOwner, retire_abandoned_indexes};

pub(super) const RECORD_PAGE_SIZE: usize = 256;

pub(super) struct RegistryIndex {
    connection: Connection,
    // Declaration order closes SQLite before removing its private file, including on Windows.
    _file: NamedTempFile,
    _owner: IndexOwner,
}

#[derive(Clone)]
pub(crate) struct IndexedRecord {
    pub(crate) process_id: ProcessId,
    pub(crate) claim: Option<ConsumptionClaim>,
    pub(crate) manifest: Option<ExecutionManifest>,
    pub(crate) retired: bool,
}

impl RegistryIndex {
    pub(super) fn rebuild(root: &Path) -> Result<(Self, Vec<PathBuf>), ProcessError> {
        let mut index = Self::new(root)?;
        let quarantine = root.join("quarantine-v1");
        let quarantined_identities = root.join("quarantined-identities-v1");
        let retained_owners = root.join("retained-owners-v1");
        let mut quarantined = load_quarantine(&quarantine)?;
        for (directory, extension) in [
            (root.join("claims-v1"), "claim"),
            (root.join("manifests-v1"), "manifest"),
            (root.join("tombstones-v1"), "tombstone"),
        ] {
            visit_registry_identities(
                &directory,
                extension,
                &retained_owners,
                &quarantine,
                &quarantined_identities,
                &mut quarantined,
                |process_id, quarantined| {
                    index.reconcile_identity(root, process_id, quarantined)
                },
            )?;
        }
        visit_quarantined_identities(
            &quarantined_identities,
            &retained_owners,
            |process_id| index.reconcile_identity(root, process_id, &mut quarantined),
        )?;
        Ok((index, quarantined))
    }

    pub(super) fn reconcile_identity(
        &mut self,
        root: &Path,
        process_id: ProcessId,
        quarantined: &mut Vec<PathBuf>,
    ) -> Result<(), ProcessError> {
        let quarantine = root.join("quarantine-v1");
        let quarantined_identities = root.join("quarantined-identities-v1");
        if let Some((claim, manifest)) = load_canonical_tombstone(
            &root.join("tombstones-v1"),
            &quarantine,
            &quarantined_identities,
            quarantined,
            process_id,
        )? {
            // A crash may have published the consumed marker immediately before moving a
            // corrupt active record. Finish that diagnostic isolation while retaining the exact
            // tombstone as terminal authority.
            let _ = load_claim(
                &root.join("claims-v1"),
                &quarantine,
                &quarantined_identities,
                quarantined,
                process_id,
            )?;
            let _ = load_manifest(
                &root.join("manifests-v1"),
                &quarantine,
                &quarantined_identities,
                quarantined,
                process_id,
            )?;
            if validate_manifest_observations(&root.join("native-observations-v1"), &manifest)
                .is_err()
            {
                quarantine_path(
                    &root.join("tombstones-v1")
                        .join(format!("{}.tombstone", crate::registry_storage::hex(process_id.as_bytes()))),
                    &quarantine,
                    &quarantined_identities,
                    quarantined,
                )?;
                return self.quarantine(process_id);
            }
            return self.replace(process_id, Some(claim), Some(manifest), true, true);
        }
        let claim = load_claim(
            &root.join("claims-v1"),
            &quarantine,
            &quarantined_identities,
            quarantined,
            process_id,
        )?;
        let manifest = load_manifest(
            &root.join("manifests-v1"),
            &quarantine,
            &quarantined_identities,
            quarantined,
            process_id,
        )?;
        if load_quarantined_identity(&quarantined_identities, process_id)? {
            return self.quarantine(process_id);
        }
        if let Some(manifest) = &manifest {
            if validate_manifest_observations(&root.join("native-observations-v1"), manifest)
                .is_err()
            {
                quarantine_path(
                    &root.join("manifests-v1")
                        .join(format!("{}.manifest", crate::registry_storage::hex(process_id.as_bytes()))),
                    &quarantine,
                    &quarantined_identities,
                    quarantined,
                )?;
                return self.quarantine(process_id);
            }
        }
        let consumed = claim.is_some() || manifest.is_some();
        self.replace(process_id, claim, manifest, false, consumed)
    }

    pub(super) fn new(root: &Path) -> Result<Self, ProcessError> {
        if let Err(error) = retire_abandoned_indexes(root) {
            eprintln!("process index cleanup deferred: {error}");
        }
        let file = tempfile::Builder::new().prefix(".process-index-data-").tempfile_in(root)
            .map_err(|error| store_cause("process index file cannot be created", error))?;
        let owner = IndexOwner::acquire(root, file.path())?;
        let connection = Connection::open(file.path())
            .map_err(|error| store_cause("process index cannot be opened", error))?;
        connection.execute_batch(
            // This private projection has no durability authority. Avoid one fsync per old row.
            "PRAGMA journal_mode=OFF;
             PRAGMA synchronous=OFF;
             PRAGMA temp_store=FILE;
             PRAGMA cache_size=-8192;
             CREATE TABLE records (
                 process_id BLOB PRIMARY KEY CHECK(length(process_id)=16),
                 claim BLOB,
                 manifest BLOB,
                 settled INTEGER NOT NULL DEFAULT 0,
                 retired INTEGER NOT NULL DEFAULT 0
             ) WITHOUT ROWID;",
        ).map_err(|error| store_cause("process index cannot be initialized", error))?;
        Ok(Self { connection, _file: file, _owner: owner })
    }

    pub(super) fn quarantine(&mut self, process_id: ProcessId) -> Result<(), ProcessError> {
        self.replace(process_id, None, None, false, true)
    }

    fn replace(
        &mut self,
        process_id: ProcessId,
        claim: Option<ConsumptionClaim>,
        manifest: Option<ExecutionManifest>,
        retired: bool,
        consumed: bool,
    ) -> Result<(), ProcessError> {
        if retired
            && !claim
                .as_ref()
                .zip(manifest.as_ref())
                .is_some_and(|(claim, manifest)| {
                    claim.matches_manifest(manifest) && retirable(manifest)
                })
        {
            return Err(store_error("process index retirement binding is invalid"));
        }
        let settled = manifest.as_ref().is_some_and(|manifest| {
            manifest.ownership_settled()
                && claim
                    .as_ref()
                    .is_some_and(|claim| claim.matches_manifest(manifest))
        });
        let claim = claim.as_ref().map(|claim| (*claim).encode());
        let manifest = manifest
            .as_ref()
            .map(ExecutionManifest::encode)
            .transpose()?;
        let transaction = self
            .connection
            .transaction()
            .map_err(|error| store_cause("process identity refresh cannot begin", error))?;
        transaction
            .execute(
                "DELETE FROM records WHERE process_id=?1",
                [process_id.as_bytes().as_slice()],
            )
            .map_err(|error| store_cause("process identity refresh cannot clear", error))?;
        if consumed {
            transaction
                .execute(
                    "INSERT INTO records(process_id,claim,manifest,settled,retired)
                     VALUES(?1,?2,?3,?4,?5)",
                    params![
                        process_id.as_bytes().as_slice(),
                        claim,
                        manifest,
                        settled,
                        retired,
                    ],
                )
                .map_err(|error| store_cause("process identity refresh cannot publish", error))?;
        }
        transaction
            .commit()
            .map_err(|error| store_cause("process identity refresh cannot commit", error))?;
        Ok(())
    }

    pub(super) fn claim(&mut self, claim: ConsumptionClaim) -> Result<(), ProcessError> {
        if let Some(existing) = self.get(claim.process_id())?
            && existing.claim.is_some_and(|existing| existing != claim)
        {
            return Err(store_error("process index claim conflicts with its durable identity"));
        }
        self.connection.execute(
            "INSERT INTO records(process_id,claim) VALUES(?1,?2)
             ON CONFLICT(process_id) DO UPDATE SET claim=excluded.claim",
            params![claim.process_id().as_bytes().as_slice(), claim.encode()],
        ).map_err(|error| store_cause("process claim index cannot be updated", error))?;
        Ok(())
    }

    pub(super) fn manifest(&mut self, manifest: &ExecutionManifest) -> Result<(), ProcessError> {
        let process_id = manifest.identity.process_id();
        if let Some(existing) = self.get(process_id)?
            && existing.retired
        {
            if existing.manifest.as_ref() != Some(manifest) {
                return Err(store_error("process manifest conflicts with an immutable tombstone"));
            }
            return Ok(());
        }
        let settled = manifest.ownership_settled()
            && self.get(process_id)?.and_then(|record| record.claim)
                .is_some_and(|claim| claim.matches_manifest(manifest));
        self.connection.execute(
            "INSERT INTO records(process_id,manifest,settled) VALUES(?1,?2,?3)
             ON CONFLICT(process_id) DO UPDATE SET
                 manifest=excluded.manifest,settled=excluded.settled",
            params![process_id.as_bytes().as_slice(), manifest.encode()?, settled],
        ).map_err(|error| store_cause("process manifest index cannot be updated", error))?;
        Ok(())
    }

    pub(super) fn tombstone(
        &mut self,
        claim: ConsumptionClaim,
        manifest: &ExecutionManifest,
    ) -> Result<(), ProcessError> {
        if !claim.matches_manifest(manifest) || !retirable(manifest) {
            return Err(store_error("process tombstone lacks settled published ownership"));
        }
        self.claim(claim)?;
        self.manifest(manifest)?;
        self.connection.execute(
            "UPDATE records SET retired=1 WHERE process_id=?1",
            [claim.process_id().as_bytes().as_slice()],
        ).map_err(|error| store_cause("process tombstone index cannot be updated", error))?;
        Ok(())
    }

    pub(super) fn get(&self, process_id: ProcessId) -> Result<Option<IndexedRecord>, ProcessError> {
        let row = self.connection.query_row(
            "SELECT claim,manifest,retired FROM records WHERE process_id=?1",
            [process_id.as_bytes().as_slice()],
            |row| Ok((row.get::<_, Option<Vec<u8>>>(0)?, row.get::<_, Option<Vec<u8>>>(1)?, row.get::<_, bool>(2)?)),
        ).optional().map_err(|error| store_cause("process index identity cannot be read", error))?;
        row.map(|(claim, manifest, retired)| decode_record(process_id, claim, manifest, retired)).transpose()
    }

    pub(super) fn page(
        &self,
        after: Option<ProcessId>,
    ) -> Result<Vec<IndexedRecord>, ProcessError> {
        let after = after.map(|id| id.as_bytes().to_vec());
        let mut statement = self.connection.prepare(
            "SELECT process_id,claim,manifest,retired FROM records
             WHERE (?1 IS NULL OR process_id>?1) ORDER BY process_id LIMIT ?2",
        ).map_err(|error| store_cause("process index page cannot be prepared", error))?;
        let page_size = i64::try_from(RECORD_PAGE_SIZE)
            .map_err(|error| store_cause("process index page size is not representable", error))?;
        let mut rows = statement.query(params![after, page_size])
            .map_err(|error| store_cause("process index page cannot be read", error))?;
        let mut page = Vec::with_capacity(RECORD_PAGE_SIZE);
        while let Some(row) = rows.next().map_err(|error| store_cause("process index page cannot advance", error))? {
            let id: Vec<u8> = row.get(0).map_err(|_| store_error("process index identity is invalid"))?;
            let bytes: [u8; 16] = id.try_into().map_err(|_| store_error("process index identity is invalid"))?;
            let process_id = ProcessId::new(bytes).map_err(|_| store_error("process index identity is zero"))?;
            let claim = row.get(1).map_err(|error| store_cause("process index claim is invalid", error))?;
            let manifest = row.get(2).map_err(|error| store_cause("process index manifest is invalid", error))?;
            let retired = row.get(3).map_err(|error| store_cause("process index retirement is invalid", error))?;
            page.push(decode_record(process_id, claim, manifest, retired)?);
        }
        Ok(page)
    }

    pub(super) fn active_count(&self) -> Result<usize, ProcessError> {
        let count: i64 = self.connection.query_row(
            "SELECT count(*) FROM records WHERE retired=0", [], |row| row.get(0),
        ).map_err(|error| store_cause("active process index cannot be counted", error))?;
        usize::try_from(count).map_err(|error| store_cause("active process count is not representable", error))
    }
}

fn validate_manifest_observations(
    root: &Path,
    manifest: &ExecutionManifest,
) -> Result<(), ProcessError> {
    if let NativeObservationState::Recorded(frontier) = manifest.native_observations {
        validate_native_observation_frontier(root, manifest.identity.process_id(), frontier)?;
    }
    Ok(())
}

pub(crate) fn retirable(manifest: &ExecutionManifest) -> bool {
    manifest.ownership_settled()
        && manifest.terminal.as_ref().is_some_and(|result| result.artifact_publication_complete())
}

fn decode_record(
    process_id: ProcessId,
    claim: Option<Vec<u8>>,
    manifest: Option<Vec<u8>>,
    retired: bool,
) -> Result<IndexedRecord, ProcessError> {
    let claim = claim.as_deref().map(ConsumptionClaim::decode).transpose()?;
    let manifest = manifest.as_deref().map(ExecutionManifest::decode).transpose()?;
    if claim.is_some_and(|claim| claim.process_id() != process_id)
        || manifest.as_ref().is_some_and(|manifest| manifest.identity.process_id() != process_id)
        || retired && !manifest.as_ref().is_some_and(retirable)
        || retired && !claim.zip(manifest.as_ref()).is_some_and(|(claim, manifest)| claim.matches_manifest(manifest))
    {
        return Err(store_error("process index record binding is invalid"));
    }
    Ok(IndexedRecord { process_id, claim, manifest, retired })
}
