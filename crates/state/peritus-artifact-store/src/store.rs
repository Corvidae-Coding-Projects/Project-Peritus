//! High-level store owner and narrow filesystem/catalog orchestration.

use std::{fs, path::Path};

use fs4::FileExt;
use peritus_types::EventId;

mod capacity;

pub use capacity::SpaceObservation;

use crate::{
    ArtifactCatalogCancellation, ArtifactDigest, ArtifactMetadata, ArtifactReadHandle,
    ArtifactStoreError, ArtifactWriteHandle, ArtifactWriter, CollectionGeneration, ErrorCode,
    FinalizedArtifact, GcAction, GcApplication, GcPlan, IntegrityState, QuarantineState, QuotaPlan,
    QuotaSnapshot, RecoveryClass, RecoveryObservation, RecoveryReport, RecoverySummary,
    ReferenceOwner, ReferenceRoots, StoragePolicy, StoreConfig, StoreOperation, WriteRequest,
    catalog::Catalog,
    finalize::{read_finalized, verify_finalized},
    path::{StorePaths, io, sync_directory},
};

/// Single-owner content-addressed filesystem and durable metadata catalog.
pub struct ArtifactStore {
    pub(crate) config: StoreConfig,
    pub(crate) paths: StorePaths,
    pub(crate) catalog: Catalog,
    _ownership: fs::File,
}

impl ArtifactStore {
    /// Reads existing finalized bytes without initializing, migrating, recovering, or writing.
    /// This diagnostic path verifies active catalog metadata and exact content on every read.
    ///
    /// # Errors
    /// Rejects absent or invalid layouts, missing/quarantined artifacts, and corrupt content.
    pub fn read_existing(
        config: &StoreConfig,
        digest: ArtifactDigest,
        maximum_bytes: u64,
    ) -> Result<Vec<u8>, ArtifactStoreError> {
        let paths = StorePaths::existing(config.root(), config.database_path())?;
        let catalog = Catalog::read_only(paths.database())?;
        let metadata = catalog.metadata(digest)?.ok_or_else(missing_artifact)?;
        if !metadata.is_referenceable() {
            return Err(missing_artifact());
        }
        read_finalized(
            &paths.object(digest),
            digest,
            metadata.size(),
            configured_read_limit(maximum_bytes, config.max_artifact_limit()),
        )
    }

    /// Opens an existing active object through read-only catalog state without recovery writes.
    /// The returned handle verifies the full immutable digest once, then supports bounded reads.
    ///
    /// # Errors
    /// Rejects absent layouts, missing/quarantined objects, and corrupt content.
    pub fn open_existing(
        config: &StoreConfig,
        digest: ArtifactDigest,
    ) -> Result<ArtifactReadHandle, ArtifactStoreError> {
        let paths = StorePaths::existing(config.root(), config.database_path())?;
        let catalog = Catalog::read_only(paths.database())?;
        let metadata = catalog.metadata(digest)?.ok_or_else(missing_artifact)?;
        if !metadata.is_referenceable() {
            return Err(missing_artifact());
        }
        ArtifactReadHandle::open(&paths, metadata, config.max_artifact_limit())
    }

    /// Opens or initializes a store and runs idempotent restart recovery.
    ///
    /// # Errors
    ///
    /// Returns typed layout, catalog, recovery, or I/O errors.
    pub fn open(config: StoreConfig) -> Result<Self, ArtifactStoreError> {
        Self::open_with_recovery_streaming(config, |_| {}).map(|(store, _)| store)
    }

    /// Opens or initializes a store and returns exact observations from this recovery pass.
    ///
    /// Callers that own startup diagnostics should use this form so contained layout entries and
    /// accepted publication recovery are not silently discarded.
    ///
    /// # Errors
    ///
    /// Returns typed layout, catalog, recovery, or I/O errors.
    pub fn open_with_recovery(
        config: StoreConfig,
    ) -> Result<(Self, RecoveryReport), ArtifactStoreError> {
        let paths = StorePaths::initialize(config.root(), config.database_path())?;
        let ownership = acquire_ownership(&paths)?;
        let catalog = Catalog::open(paths.database())?;
        let mut store = Self { config, paths, catalog, _ownership: ownership };
        let recovery = store.recover()?;
        Ok((store, recovery))
    }

    /// Opens or initializes a store and streams exact detailed recovery observations.
    ///
    /// Only bounded counters are retained. Callers that may encounter an unbounded number of
    /// recovery anomalies should use this form instead of [`Self::open_with_recovery`].
    ///
    /// # Errors
    ///
    /// Returns typed layout, catalog, recovery, or I/O errors.
    pub fn open_with_recovery_streaming(
        config: StoreConfig,
        observer: impl FnMut(RecoveryObservation),
    ) -> Result<(Self, RecoverySummary), ArtifactStoreError> {
        let paths = StorePaths::initialize(config.root(), config.database_path())?;
        let ownership = acquire_ownership(&paths)?;
        let catalog = Catalog::open(paths.database())?;
        let store = Self { config, paths, catalog, _ownership: ownership };
        let recovery = store.recover_streaming(observer)?;
        Ok((store, recovery))
    }

    /// Returns the canonicalized store root.
    #[must_use]
    pub fn root(&self) -> &Path {
        self.paths.root()
    }

    /// Returns the capacity authority selected when this store was opened.
    #[must_use]
    pub const fn storage_policy(&self) -> StoragePolicy {
        self.config.storage_policy()
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
        if self.config.quota_bytes().is_some() {
            QuotaPlan::reserve(self.quota_snapshot(0)?, reservation)?;
        }
        ArtifactWriter::create(
            &self.paths,
            &self.catalog,
            request,
            self.config.max_artifact_limit(),
            self.config.quota_bytes(),
            self.config.minimum_free_bytes(),
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
        if self.config.quota_bytes().is_some() {
            QuotaPlan::reserve(self.quota_snapshot(0)?, reservation)?;
        }
        ArtifactWriteHandle::create(
            &self.paths,
            &self.catalog,
            request,
            self.config.max_artifact_limit(),
            self.config.quota_bytes(),
            self.config.minimum_free_bytes(),
        )
    }

    /// Atomically publishes and catalogs one exact owned streaming writer.
    ///
    /// # Errors
    ///
    /// Returns exact writer, integrity, publication, catalog, or quota failures.
    pub fn complete_write(
        &self,
        mut writer: ArtifactWriteHandle,
    ) -> Result<FinalizedArtifact, ArtifactStoreError> {
        writer.complete(&self.paths, &self.catalog)
    }

    /// Attempts exact owned-writer completion under explicit catalog-wait cancellation.
    ///
    /// The writer retains its verified bytes, durable reservation, and publication receipt after
    /// any retryable error, including explicit cancellation. Retrying this method does not create
    /// a replacement operation.
    ///
    /// # Errors
    ///
    /// Returns exact writer, integrity, publication, catalog, quota, or cancellation failures.
    pub fn try_complete_write(
        &self,
        writer: &mut ArtifactWriteHandle,
        cancellation: &ArtifactCatalogCancellation,
    ) -> Result<FinalizedArtifact, ArtifactStoreError> {
        cancellation.run(|| writer.complete(&self.paths, &self.catalog))
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
        ArtifactReadHandle::open(&self.paths, metadata, self.config.max_artifact_limit())
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

    /// Immutably binds one finalized active artifact to an ordered, bounded direct-child list.
    ///
    /// Every child must be finalized and active at the exact supplied digest and length. Repeating
    /// the exact binding is idempotent. Changing it, introducing a self-edge, or binding a parent
    /// that already has a durable owner or appears as another bundle's child is rejected. This
    /// bottom-up rule makes dependency cycles impossible and freezes every published subgraph.
    ///
    /// # Errors
    ///
    /// Returns missing-artifact, size, invalid-binding, or catalog errors.
    pub fn bind_dependencies<I>(
        &self,
        parent: ArtifactDigest,
        children: I,
    ) -> Result<(), ArtifactStoreError>
    where
        I: Clone + ExactSizeIterator<Item = (ArtifactDigest, u64)>,
    {
        self.catalog.bind_dependencies(parent, children)
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
    /// When this was the last reference to the artifact, the same transaction explicitly releases
    /// it for the existing two-generation collection process.
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

    /// Atomically retires every artifact root owned by one obsolete durable segment.
    ///
    /// The caller must first prove that the owner is absent from the authoritative live and
    /// historical-retrieval frontier. Artifacts whose last reference belongs to this owner become
    /// collection candidates in the same transaction. Collection still uses the existing later
    /// quarantine and sweep generations.
    ///
    /// # Errors
    ///
    /// Returns a catalog or reference-count representation error.
    pub fn retire_reference_owner(
        &self,
        owner: ReferenceOwner,
    ) -> Result<u64, ArtifactStoreError> {
        self.catalog.retire_reference_owner(owner)
    }

    /// Atomically transfers every durable root from a legacy owner to its canonical replacement.
    ///
    /// Exact references, including repair obligations, remain continuously owned throughout the
    /// transaction. This is intended for format migrations performed while the store lock is held.
    ///
    /// # Errors
    ///
    /// Returns a catalog or reference-count representation error.
    pub fn migrate_reference_owner(
        &self,
        previous: ReferenceOwner,
        replacement: ReferenceOwner,
    ) -> Result<u64, ArtifactStoreError> {
        self.catalog.migrate_reference_owner(previous, replacement)
    }

    /// Explicitly releases a finalized publication that never acquired a durable reference.
    ///
    /// The exact creating event prevents one context from releasing a coincidentally known digest
    /// owned by another publication lineage. Referenced artifacts must instead be released through
    /// [`Self::remove_reference`] or [`Self::retire_reference_owner`]. The artifact remains readable
    /// until a later explicit quarantine generation and remains recoverable until a still later
    /// sweep generation.
    ///
    /// Returns whether this call newly released the publication.
    ///
    /// # Errors
    ///
    /// Returns missing-artifact, ownership, reference-state, or catalog errors.
    pub fn release_publication(
        &self,
        digest: ArtifactDigest,
        creating_event: EventId,
    ) -> Result<bool, ArtifactStoreError> {
        self.catalog.release_publication(digest, creating_event)
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
        let (committed_bytes, durable_reserved_bytes) = self.catalog.quota_bytes()?;
        let reserved_bytes = durable_reserved_bytes.checked_add(reserved_bytes).ok_or_else(|| {
            ArtifactStoreError::message(
                ErrorCode::ArithmeticOverflow,
                RecoveryClass::RecoverStore,
                "durable and requested artifact reservations overflowed",
            )
        })?;
        QuotaSnapshot::for_policy(
            committed_bytes,
            reserved_bytes,
            self.config.quota_bytes(),
        )
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
    /// Only publications explicitly released through the ownership APIs are eligible. A finalized
    /// artifact with no reference is retained by default because its journal or evidence owner may
    /// not have committed yet.
    ///
    /// # Errors
    ///
    /// Returns catalog, integrity, or invalid-plan errors.
    pub fn plan_gc(&self, generation: CollectionGeneration) -> Result<GcPlan, ArtifactStoreError> {
        GcPlan::build(generation, self.catalog.inventory()?, &self.catalog.roots()?)
    }

    /// Collects only explicitly released, currently unreferenced artifacts before a caller waits
    /// for physical storage. Two fresh generations preserve the durable quarantine transition
    /// while allowing already released bytes to be swept during this pressure response.
    ///
    /// Finalized publications that have not acquired or released an owner remain protected, and
    /// reference roots are reloaded between generations so shared dependency closure stays live.
    ///
    /// # Errors
    ///
    /// Returns catalog, integrity, generation-overflow, or filesystem errors.
    pub fn collect_released_for_pressure(&self) -> Result<(), ArtifactStoreError> {
        let inventory = self.catalog.inventory()?;
        let latest = inventory
            .iter()
            .filter_map(|entry| match entry.quarantine() {
                QuarantineState::Active => None,
                QuarantineState::Quarantined { since } => Some(since.get()),
            })
            .max()
            .unwrap_or(0);
        let first = CollectionGeneration::new(latest.checked_add(1).ok_or_else(|| {
            ArtifactStoreError::message(
                ErrorCode::ArithmeticOverflow,
                RecoveryClass::RecoverStore,
                "artifact pressure collection generation overflowed",
            )
        })?)?;
        let first_plan = self.plan_gc(first)?;
        self.apply_gc_plan(&first_plan)?;
        let second = CollectionGeneration::new(first.get().checked_add(1).ok_or_else(|| {
            ArtifactStoreError::message(
                ErrorCode::ArithmeticOverflow,
                RecoveryClass::RecoverStore,
                "artifact pressure sweep generation overflowed",
            )
        })?)?;
        let second_plan = self.plan_gc(second)?;
        self.apply_gc_plan(&second_plan)?;
        Ok(())
    }

    /// Applies an explicit collection plan in canonical action order.
    ///
    /// Each action is restart-recoverable. If an error interrupts a plan, reopening the store
    /// reconciles the action at its durable metadata boundary before a fresh plan is computed.
    ///
    /// # Errors
    ///
    /// Returns stale-plan, catalog, I/O, missing-file, or corruption errors.
    pub fn apply_gc_plan(&self, plan: &GcPlan) -> Result<GcApplication, ArtifactStoreError> {
        let mut application = GcApplication::default();
        for &action in plan.actions() {
            self.apply_action(action)?;
            application.observe(action)?;
        }
        Ok(application)
    }

    fn apply_action(&self, action: GcAction) -> Result<(), ArtifactStoreError> {
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
            GcAction::QuarantineRepair { digest, size, generation } => {
                let metadata = self.require_state(digest, size, QuarantineState::Active)?;
                if metadata.integrity() != IntegrityState::Corrupt {
                    return Err(stale_plan());
                }
                self.catalog
                    .set_quarantine(digest, QuarantineState::Quarantined { since: generation })
            }
            GcAction::DeleteRepair { digest, size, since } => {
                let metadata =
                    self.require_state(digest, size, QuarantineState::Quarantined { since })?;
                if metadata.integrity() != IntegrityState::Corrupt {
                    return Err(stale_plan());
                }
                let containments = self.catalog.repair_containments(digest)?;
                self.catalog.delete_record(digest)?;
                self.remove_repair_content(digest, &containments)
            }
        }
    }

    fn remove_repair_content(
        &self,
        digest: ArtifactDigest,
        containments: &[(u64, crate::ContainedLayoutNamespace)],
    ) -> Result<(), ArtifactStoreError> {
        for path in [self.paths.object(digest), self.paths.quarantine(digest)] {
            match fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.file_type().is_file() || metadata.file_type().is_symlink() => {
                    fs::remove_file(&path).map_err(|error| io(StoreOperation::Remove, error))?;
                    sync_directory(path.parent().ok_or_else(|| {
                        ArtifactStoreError::message(
                            ErrorCode::CorruptObject,
                            RecoveryClass::TerminalIntegrity,
                            "repair content has no canonical parent",
                        )
                    })?)?;
                }
                Ok(_) => return Err(ArtifactStoreError::message(
                    ErrorCode::CorruptObject,
                    RecoveryClass::TerminalIntegrity,
                    "repair content is not a regular file",
                )),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(io(StoreOperation::Remove, error)),
            }
        }
        let mut removed_containment = false;
        for &(identity, namespace) in containments {
            let directory = self.paths.recovery().join(format!(
                "{identity:016x}-{}-{}",
                namespace.tag(),
                digest.to_hex(),
            ));
            match fs::symlink_metadata(&directory) {
                Ok(metadata) if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() => {
                    fs::remove_dir_all(&directory)
                        .map_err(|error| io(StoreOperation::Remove, error))?;
                    removed_containment = true;
                }
                Ok(_) => return Err(ArtifactStoreError::message(
                    ErrorCode::CorruptObject,
                    RecoveryClass::TerminalIntegrity,
                    "repair containment is not a safe directory",
                )),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(io(StoreOperation::Remove, error)),
            }
        }
        if removed_containment {
            sync_directory(self.paths.recovery())?;
        }
        Ok(())
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

fn acquire_ownership(paths: &StorePaths) -> Result<fs::File, ArtifactStoreError> {
    let path = paths.root().join("owner.lock");
    match fs::symlink_metadata(&path) {
        Ok(metadata) if !metadata.file_type().is_file() => {
            return Err(ArtifactStoreError::message(
                ErrorCode::InvalidConfiguration,
                RecoveryClass::TerminalIntegrity,
                "artifact ownership lock is not a regular file",
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(io(StoreOperation::AcquireOwnership, error)),
    }
    let ownership = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|error| io(StoreOperation::AcquireOwnership, error))?;
    FileExt::try_lock(&ownership)
        .map_err(|error| ArtifactStoreError::store_owned(error.into()))?;
    Ok(ownership)
}

const fn configured_read_limit(requested: u64, configured: Option<u64>) -> u64 {
    match configured {
        Some(configured) if configured < requested => configured,
        _ => requested,
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
