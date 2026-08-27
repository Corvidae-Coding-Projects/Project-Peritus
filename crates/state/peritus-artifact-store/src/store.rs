//! High-level store owner and narrow filesystem/catalog orchestration.

use std::{fs, path::Path};

use crate::{
    ArtifactDigest, ArtifactMetadata, ArtifactReadHandle, ArtifactStoreError, ArtifactWriteHandle,
    ArtifactWriter, CollectionGeneration, ErrorCode, FinalizedArtifact, GcAction, GcApplication,
    GcPlan, QuarantineState, QuotaPlan, QuotaSnapshot, RecoveryClass, ReferenceOwner,
    ReferenceRoots, StoreConfig, StoreOperation, WriteRequest,
    catalog::Catalog,
    finalize::{read_finalized, verify_finalized},
    path::{StorePaths, io, sync_directory},
};

/// Filesystem capacity observed at the canonical store root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpaceObservation {
    available_bytes: u64,
    free_bytes: u64,
    total_bytes: u64,
    allocation_granularity: u64,
}

impl SpaceObservation {
    /// Returns bytes available to an unprivileged process.
    #[must_use]
    pub const fn available_bytes(self) -> u64 {
        self.available_bytes
    }
    /// Returns all free filesystem bytes.
    #[must_use]
    pub const fn free_bytes(self) -> u64 {
        self.free_bytes
    }
    /// Returns total filesystem bytes.
    #[must_use]
    pub const fn total_bytes(self) -> u64 {
        self.total_bytes
    }
    /// Returns filesystem allocation granularity.
    #[must_use]
    pub const fn allocation_granularity(self) -> u64 {
        self.allocation_granularity
    }
}

/// Single-owner content-addressed filesystem and durable metadata catalog.
pub struct ArtifactStore {
    pub(crate) config: StoreConfig,
    pub(crate) paths: StorePaths,
    pub(crate) catalog: Catalog,
}

impl ArtifactStore {
    /// Opens or initializes a store and runs idempotent restart recovery.
    ///
    /// # Errors
    ///
    /// Returns typed layout, catalog, recovery, or I/O errors.
    pub fn open(config: StoreConfig) -> Result<Self, ArtifactStoreError> {
        let paths = StorePaths::initialize(config.root(), config.database_path())?;
        let catalog = Catalog::open(paths.database())?;
        let mut store = Self { config, paths, catalog };
        store.recover()?;
        Ok(store)
    }

    /// Returns the canonicalized store root.
    #[must_use]
    pub fn root(&self) -> &Path {
        self.paths.root()
    }

    /// Creates an exclusive bounded streaming writer.
    ///
    /// # Errors
    ///
    /// Returns a catalog, quota, invalid-request, or temporary-file I/O error.
    pub fn begin_write(
        &self,
        request: WriteRequest,
    ) -> Result<ArtifactWriter<'_>, ArtifactStoreError> {
        let reservation = if self.catalog.metadata(request.expected_digest())?.is_some() {
            0
        } else {
            request.expected_size()
        };
        QuotaPlan::reserve(self.quota_snapshot(0)?, reservation)?;
        ArtifactWriter::create(
            &self.paths,
            &self.catalog,
            request,
            self.config.max_artifact_bytes(),
            self.config.quota_bytes(),
        )
    }

    /// Creates an owned exclusive streaming writer suitable for a long-lived transfer registry.
    ///
    /// # Errors
    ///
    /// Returns a catalog, quota, invalid-request, or temporary-file I/O error.
    pub fn begin_owned_write(
        &self,
        request: WriteRequest,
    ) -> Result<ArtifactWriteHandle, ArtifactStoreError> {
        let reservation = if self.catalog.metadata(request.expected_digest())?.is_some() {
            0
        } else {
            request.expected_size()
        };
        QuotaPlan::reserve(self.quota_snapshot(0)?, reservation)?;
        ArtifactWriteHandle::create(
            &self.paths,
            request,
            self.config.max_artifact_bytes(),
            self.config.quota_bytes(),
        )
    }

    /// Atomically publishes and catalogs one exact owned streaming writer.
    ///
    /// # Errors
    ///
    /// Returns exact writer, integrity, publication, catalog, or quota failures.
    pub fn complete_write(
        &self,
        writer: ArtifactWriteHandle,
    ) -> Result<FinalizedArtifact, ArtifactStoreError> {
        writer.complete(&self.paths, &self.catalog)
    }

    /// Opens one owned preverified streaming reader for finalized active content.
    ///
    /// # Errors
    ///
    /// Returns missing-artifact, catalog, I/O, or corruption errors.
    pub fn open_read(
        &self,
        digest: ArtifactDigest,
    ) -> Result<ArtifactReadHandle, ArtifactStoreError> {
        let metadata = self.catalog.metadata(digest)?.ok_or_else(missing_artifact)?;
        if !metadata.is_referenceable() {
            return Err(missing_artifact());
        }
        ArtifactReadHandle::open(&self.paths, metadata, self.config.max_artifact_bytes())
    }

    /// Loads validated durable artifact metadata.
    ///
    /// # Errors
    ///
    /// Returns a catalog or integrity error.
    pub fn metadata(
        &self,
        digest: ArtifactDigest,
    ) -> Result<Option<ArtifactMetadata>, ArtifactStoreError> {
        self.catalog.metadata(digest)
    }

    /// Re-hashes a finalized active object and checks its durable size.
    ///
    /// # Errors
    ///
    /// Returns missing-artifact, catalog, I/O, or corruption errors.
    pub fn verify(&self, digest: ArtifactDigest) -> Result<ArtifactMetadata, ArtifactStoreError> {
        let metadata = self.catalog.metadata(digest)?.ok_or_else(missing_artifact)?;
        if !metadata.is_referenceable() {
            return Err(missing_artifact());
        }
        verify_finalized(&self.paths.object(digest), digest, metadata.size())?;
        Ok(metadata)
    }

    /// Reads one finalized active artifact into a bounded owned buffer and verifies the exact
    /// durable size and digest against the bytes returned to the caller.
    ///
    /// # Errors
    ///
    /// Returns a byte-limit, missing-artifact, catalog, I/O, or corruption error.
    pub fn read(
        &self,
        digest: ArtifactDigest,
        maximum_bytes: u64,
    ) -> Result<Vec<u8>, ArtifactStoreError> {
        let metadata = self.catalog.metadata(digest)?.ok_or_else(missing_artifact)?;
        if !metadata.is_referenceable() {
            return Err(missing_artifact());
        }
        read_finalized(&self.paths.object(digest), digest, metadata.size(), maximum_bytes)
    }

    /// Adds an idempotent durable reference to a finalized active artifact.
    ///
    /// # Errors
    ///
    /// Returns missing-artifact, invalid-state, catalog, or integrity errors.
    pub fn add_reference(
        &self,
        owner: ReferenceOwner,
        digest: ArtifactDigest,
    ) -> Result<(), ArtifactStoreError> {
        self.catalog.add_reference(owner, digest)
    }

    /// Removes one exact durable reference.
    ///
    /// # Errors
    ///
    /// Returns a catalog I/O or integrity error.
    pub fn remove_reference(
        &self,
        owner: ReferenceOwner,
        digest: ArtifactDigest,
    ) -> Result<bool, ArtifactStoreError> {
        self.catalog.remove_reference(owner, digest)
    }

    /// Loads canonical durable journal/evidence root sets.
    ///
    /// # Errors
    ///
    /// Returns a catalog I/O or integrity error.
    pub fn reference_roots(&self) -> Result<ReferenceRoots, ArtifactStoreError> {
        self.catalog.roots()
    }

    /// Creates a checked quota observation from durable usage and the configured limit.
    ///
    /// # Errors
    ///
    /// Returns overflow or quota exhaustion for an invalid observation.
    pub fn quota_snapshot(&self, reserved_bytes: u64) -> Result<QuotaSnapshot, ArtifactStoreError> {
        QuotaSnapshot::new(self.catalog.used_bytes()?, reserved_bytes, self.config.quota_bytes())
    }

    /// Plans a quota reservation against durable artifact accounting.
    ///
    /// # Errors
    ///
    /// Returns catalog, arithmetic-overflow, or quota-exhaustion errors.
    pub fn plan_quota(&self, reservation_bytes: u64) -> Result<QuotaPlan, ArtifactStoreError> {
        QuotaPlan::reserve(self.quota_snapshot(0)?, reservation_bytes)
    }

    /// Loads durable inventory and roots, then computes a pure deterministic collection plan.
    ///
    /// # Errors
    ///
    /// Returns catalog, integrity, or invalid-plan errors.
    pub fn plan_gc(&self, generation: CollectionGeneration) -> Result<GcPlan, ArtifactStoreError> {
        GcPlan::build(generation, self.catalog.inventory()?, &self.catalog.roots()?)
    }

    /// Applies an explicit collection plan in canonical action order.
    ///
    /// Each action is restart-recoverable. If an error interrupts a plan, reopening the store
    /// reconciles the action at its durable metadata boundary before a fresh plan is computed.
    ///
    /// # Errors
    ///
    /// Returns stale-plan, catalog, I/O, missing-file, or corruption errors.
    pub fn apply_gc_plan(&mut self, plan: &GcPlan) -> Result<GcApplication, ArtifactStoreError> {
        let mut application = GcApplication::default();
        for &action in plan.actions() {
            self.apply_action(action)?;
            application.observe(action)?;
        }
        Ok(application)
    }

    /// Observes capacity without making quota or acceptance decisions.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when filesystem statistics are unavailable.
    pub fn observe_space(&self) -> Result<SpaceObservation, ArtifactStoreError> {
        let stats = fs4::statvfs(self.paths.root())
            .map_err(|error| io(StoreOperation::ObserveSpace, error))?;
        Ok(SpaceObservation {
            available_bytes: stats.available_space(),
            free_bytes: stats.free_space(),
            total_bytes: stats.total_space(),
            allocation_granularity: stats.allocation_granularity(),
        })
    }

    fn apply_action(&mut self, action: GcAction) -> Result<(), ArtifactStoreError> {
        match action {
            GcAction::Quarantine { digest, size, generation } => {
                let metadata = self.require_state(digest, size, QuarantineState::Active)?;
                if !metadata.is_referenceable() {
                    return Err(stale_plan());
                }
                verify_finalized(&self.paths.object(digest), digest, size)?;
                self.catalog
                    .set_quarantine(digest, QuarantineState::Quarantined { since: generation })?;
                self.move_to_quarantine(digest, size)
            }
            GcAction::Restore { digest, size, since } => {
                self.require_state(digest, size, QuarantineState::Quarantined { since })?;
                verify_finalized(&self.paths.quarantine(digest), digest, size)?;
                self.move_to_objects(digest, size)?;
                self.catalog.set_quarantine(digest, QuarantineState::Active)
            }
            GcAction::Delete { digest, size, since } => {
                self.require_state(digest, size, QuarantineState::Quarantined { since })?;
                verify_finalized(&self.paths.quarantine(digest), digest, size)?;
                self.catalog.delete_record(digest)?;
                fs::remove_file(self.paths.quarantine(digest))
                    .map_err(|error| io(StoreOperation::Remove, error))?;
                sync_directory(&self.paths.ensure_quarantine_parent(digest)?)
            }
        }
    }

    fn require_state(
        &self,
        digest: ArtifactDigest,
        size: u64,
        quarantine: QuarantineState,
    ) -> Result<ArtifactMetadata, ArtifactStoreError> {
        let metadata = self.catalog.metadata(digest)?.ok_or_else(stale_plan)?;
        if metadata.size() != size || metadata.quarantine() != quarantine {
            return Err(stale_plan());
        }
        Ok(metadata)
    }

    pub(crate) fn move_to_quarantine(
        &self,
        digest: ArtifactDigest,
        size: u64,
    ) -> Result<(), ArtifactStoreError> {
        let source = self.paths.object(digest);
        let destination = self.paths.quarantine(digest);
        let destination_parent = self.paths.ensure_quarantine_parent(digest)?;
        let source_parent = self.paths.ensure_object_parent(digest)?;
        transfer_no_replace(
            &source,
            &destination,
            &source_parent,
            &destination_parent,
            digest,
            size,
        )
    }

    pub(crate) fn move_to_objects(
        &self,
        digest: ArtifactDigest,
        size: u64,
    ) -> Result<(), ArtifactStoreError> {
        let source = self.paths.quarantine(digest);
        let destination = self.paths.object(digest);
        let destination_parent = self.paths.ensure_object_parent(digest)?;
        let source_parent = self.paths.ensure_quarantine_parent(digest)?;
        transfer_no_replace(
            &source,
            &destination,
            &source_parent,
            &destination_parent,
            digest,
            size,
        )
    }
}

fn transfer_no_replace(
    source: &Path,
    destination: &Path,
    source_parent: &Path,
    destination_parent: &Path,
    digest: ArtifactDigest,
    size: u64,
) -> Result<(), ArtifactStoreError> {
    match fs::hard_link(source, destination) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            verify_finalized(destination, digest, size)?;
            verify_finalized(source, digest, size)?;
        }
        Err(error) => return Err(io(StoreOperation::MoveQuarantine, error)),
    }
    sync_directory(destination_parent)?;
    fs::remove_file(source).map_err(|error| io(StoreOperation::MoveQuarantine, error))?;
    sync_directory(source_parent)
}

const fn missing_artifact() -> ArtifactStoreError {
    ArtifactStoreError::message(
        ErrorCode::MissingArtifact,
        RecoveryClass::CorrectRequest,
        "finalized artifact does not exist",
    )
}

const fn stale_plan() -> ArtifactStoreError {
    ArtifactStoreError::message(
        ErrorCode::InvalidCollectionPlan,
        RecoveryClass::CorrectRequest,
        "collection plan no longer matches durable artifact state",
    )
}
