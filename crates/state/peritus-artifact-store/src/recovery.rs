//! Idempotent restart recovery for temporary, object, quarantine, and catalog state.

use std::{fs, path::Path};

use crate::{
    ArtifactDigest, ArtifactMetadata, ArtifactStore, ArtifactStoreError, ErrorCode,
    FinalizationState, IntegrityState, QuarantineState, RecoveryClass, ReferenceOwner,
    StoreOperation,
    finalize::{inspect_file, publish, verify_finalized},
    path::{io, sync_directory},
};

/// Orphaned finalized bytes moved to quarantine during recovery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QuarantinedArtifact {
    digest: ArtifactDigest,
    size: u64,
}

/// Cataloged artifact whose missing or divergent bytes require exact repair.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContainedCorruption {
    digest: ArtifactDigest,
    expected_size: u64,
    reason: ArtifactRepairReason,
    referenced: bool,
}

/// Why exact durable artifact content requires owner-directed repair.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactRepairReason {
    /// No canonical object or quarantine bytes remain for the durable identity.
    Missing,
    /// Bytes remain contained, but they do not match the durable identity.
    Corrupt,
}

/// One paged exact artifact repair obligation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArtifactRepairObligation {
    digest: ArtifactDigest,
    expected_size: u64,
    reason: ArtifactRepairReason,
}

/// Fixed store namespace from which a malformed layout entry was contained.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContainedLayoutNamespace {
    /// The temporary-writer directory.
    Temporary,
    /// The active object digest tree.
    Objects,
    /// The quarantine digest tree.
    Quarantine,
}

impl ContainedLayoutNamespace {
    pub(crate) const fn tag(self) -> &'static str {
        match self {
            Self::Temporary => "temporary",
            Self::Objects => "objects",
            Self::Quarantine => "quarantine",
        }
    }

    pub(crate) const fn database_tag(self) -> i64 {
        match self {
            Self::Objects => 1,
            Self::Quarantine => 2,
            Self::Temporary => 3,
        }
    }
}

/// Durable containment inventory entry for malformed unrelated layout content.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContainedLayoutEntry {
    identity: u64,
    namespace: ContainedLayoutNamespace,
    repair_digest: Option<ArtifactDigest>,
}

impl ContainedCorruption {
    /// Returns the durable content identity whose bytes diverged.
    #[must_use]
    pub const fn digest(self) -> ArtifactDigest {
        self.digest
    }

    /// Returns the logical size recorded before corruption was detected.
    #[must_use]
    pub const fn expected_size(self) -> u64 {
        self.expected_size
    }

    /// Returns why exact content repair is required.
    #[must_use]
    pub const fn reason(self) -> ArtifactRepairReason {
        self.reason
    }

    /// Returns whether at least one durable owner still requires the content.
    #[must_use]
    pub const fn is_referenced(self) -> bool {
        self.referenced
    }
}

impl ArtifactRepairObligation {
    /// Returns the exact content digest requiring repair.
    #[must_use]
    pub const fn digest(self) -> ArtifactDigest {
        self.digest
    }

    /// Returns the durable expected byte size.
    #[must_use]
    pub const fn expected_size(self) -> u64 {
        self.expected_size
    }

    /// Returns whether bytes are missing or contained as corrupt.
    #[must_use]
    pub const fn reason(self) -> ArtifactRepairReason {
        self.reason
    }
}

impl ContainedLayoutEntry {
    /// Returns the durable operation identity assigned to this containment directory.
    #[must_use]
    pub const fn identity(self) -> u64 {
        self.identity
    }

    /// Returns the fixed namespace from which the entry was removed.
    #[must_use]
    pub const fn namespace(self) -> ContainedLayoutNamespace {
        self.namespace
    }

    /// Returns the exact repair identity when this containment belongs to an active obligation.
    #[must_use]
    pub const fn repair_digest(self) -> Option<ArtifactDigest> {
        self.repair_digest
    }
}

impl QuarantinedArtifact {
    /// Returns the verified content digest.
    #[must_use]
    pub const fn digest(self) -> ArtifactDigest {
        self.digest
    }

    /// Returns verified logical bytes.
    #[must_use]
    pub const fn size(self) -> u64 {
        self.size
    }
}

/// Restart-recovery observations.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RecoveryReport {
    removed_temporary_files: u64,
    completed_state_moves: u64,
    removed_swept_files: u64,
    reconciled_publications: u64,
    quarantined_orphans: Vec<QuarantinedArtifact>,
    contained_corruptions: Vec<ContainedCorruption>,
    contained_layout_entries: Vec<ContainedLayoutEntry>,
}

/// Bounded counters from one restart-recovery pass.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RecoverySummary {
    removed_temporary_files: u64,
    completed_state_moves: u64,
    removed_swept_files: u64,
    reconciled_publications: u64,
}

/// One exact recovery observation delivered without accumulating the complete anomaly history.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryObservation {
    /// One uncataloged verified object was moved to quarantine.
    QuarantinedOrphan(QuarantinedArtifact),
    /// One exact missing or divergent artifact requires repair.
    ContainedCorruption(ContainedCorruption),
    /// One malformed layout entry was moved into durable containment.
    ContainedLayout(ContainedLayoutEntry),
}

struct RecoveryRun<'a> {
    report: RecoveryReport,
    collect_details: bool,
    observer: &'a mut dyn FnMut(RecoveryObservation),
}

impl RecoveryRun<'_> {
    fn observe(&mut self, observation: RecoveryObservation) {
        if self.collect_details {
            match observation {
                RecoveryObservation::QuarantinedOrphan(value) => {
                    self.report.quarantined_orphans.push(value);
                }
                RecoveryObservation::ContainedCorruption(value) => {
                    self.report.contained_corruptions.push(value);
                }
                RecoveryObservation::ContainedLayout(value) => {
                    self.report.contained_layout_entries.push(value);
                }
            }
        }
        (self.observer)(observation);
    }
}

impl RecoveryReport {
    /// Returns abandoned temporary files removed.
    #[must_use]
    pub const fn removed_temporary_files(&self) -> u64 {
        self.removed_temporary_files
    }

    /// Returns interrupted quarantine/restore moves completed.
    #[must_use]
    pub const fn completed_state_moves(&self) -> u64 {
        self.completed_state_moves
    }

    /// Returns untracked quarantine files left by an interrupted sweep and removed now.
    #[must_use]
    pub const fn removed_swept_files(&self) -> u64 {
        self.removed_swept_files
    }

    /// Returns durable publication reservations reconciled after an interrupted finalization.
    #[must_use]
    pub const fn reconciled_publications(&self) -> u64 {
        self.reconciled_publications
    }

    /// Returns finalized objects with no durable record that were conservatively quarantined.
    #[must_use]
    pub fn quarantined_orphans(&self) -> &[QuarantinedArtifact] {
        &self.quarantined_orphans
    }

    /// Returns cataloged artifacts made non-referenceable because exact bytes are missing or were
    /// moved out of a canonical namespace.
    #[must_use]
    pub fn contained_corruptions(&self) -> &[ContainedCorruption] {
        &self.contained_corruptions
    }

    /// Returns malformed unrelated layout entries moved into durable containment.
    #[must_use]
    pub fn contained_layout_entries(&self) -> &[ContainedLayoutEntry] {
        &self.contained_layout_entries
    }

    /// Returns bounded counters independent of collected detailed observations.
    #[must_use]
    pub const fn summary(&self) -> RecoverySummary {
        RecoverySummary {
            removed_temporary_files: self.removed_temporary_files,
            completed_state_moves: self.completed_state_moves,
            removed_swept_files: self.removed_swept_files,
            reconciled_publications: self.reconciled_publications,
        }
    }
}

impl RecoverySummary {
    /// Returns abandoned temporary files removed.
    #[must_use]
    pub const fn removed_temporary_files(self) -> u64 {
        self.removed_temporary_files
    }

    /// Returns interrupted quarantine/restore moves completed.
    #[must_use]
    pub const fn completed_state_moves(self) -> u64 {
        self.completed_state_moves
    }

    /// Returns untracked quarantine files left by an interrupted sweep and removed now.
    #[must_use]
    pub const fn removed_swept_files(self) -> u64 {
        self.removed_swept_files
    }

    /// Returns durable publication reservations reconciled after interrupted finalization.
    #[must_use]
    pub const fn reconciled_publications(self) -> u64 {
        self.reconciled_publications
    }
}

impl ArtifactStore {
    /// Returns one bounded digest-ordered page of exact content repair obligations.
    ///
    /// The page includes finalized records already made non-referenceable by recovery. Owners may
    /// be loaded in bounded pages through [`Self::repair_owners_after`].
    ///
    /// # Errors
    ///
    /// Returns a catalog, page-bound, or filesystem-observation error.
    pub fn repair_obligations_after(
        &self,
        after: Option<ArtifactDigest>,
        maximum: usize,
    ) -> Result<Vec<ArtifactRepairObligation>, ArtifactStoreError> {
        let metadata = self.catalog.repair_obligations_after(after, maximum)?;
        let mut obligations = Vec::with_capacity(metadata.len());
        for (record, reason) in metadata {
            obligations.push(ArtifactRepairObligation {
                digest: record.digest(),
                expected_size: record.size(),
                reason,
            });
        }
        Ok(obligations)
    }

    /// Returns one bounded canonical page of durable owners for an exact repair obligation.
    ///
    /// # Errors
    ///
    /// Returns a catalog, page-bound, or owner-representation error.
    pub fn repair_owners_after(
        &self,
        digest: ArtifactDigest,
        after: Option<ReferenceOwner>,
        maximum: usize,
    ) -> Result<Vec<ReferenceOwner>, ArtifactStoreError> {
        self.catalog.reference_owners_after(digest, after, maximum)
    }

    /// Reconciles crash windows and removes abandoned temporary files.
    ///
    /// Durable catalog state directs interrupted quarantine moves. Untracked active objects are
    /// first moved to quarantine; untracked files already in quarantine are deleted on this later
    /// recovery pass. Every digest-named file is re-hashed before it is trusted or moved.
    ///
    /// # Errors
    ///
    /// Returns an I/O or catalog error when recovery itself cannot make durable progress. Exact
    /// missing or corrupt artifacts are made non-referenceable and surfaced as repair obligations
    /// without fencing unrelated healthy content.
    pub fn recover(&mut self) -> Result<RecoveryReport, ArtifactStoreError> {
        let mut ignore = |_| {};
        self.recover_with_observer(&mut ignore, true)
    }

    /// Reconciles crash windows while delivering each detailed observation to `observer`.
    ///
    /// This form retains only bounded counters, so production startup memory does not grow with the
    /// number of malformed or damaged entries. [`Self::recover`] remains available to callers that
    /// explicitly need the legacy collected report.
    ///
    /// # Errors
    ///
    /// Returns an I/O or catalog error when recovery itself cannot make durable progress.
    pub fn recover_streaming(
        &self,
        mut observer: impl FnMut(RecoveryObservation),
    ) -> Result<RecoverySummary, ArtifactStoreError> {
        let report = self.recover_with_observer(&mut observer, false)?;
        Ok(report.summary())
    }

    fn recover_with_observer(
        &self,
        observer: &mut dyn FnMut(RecoveryObservation),
        collect_details: bool,
    ) -> Result<RecoveryReport, ArtifactStoreError> {
        let mut run = RecoveryRun {
            report: RecoveryReport::default(),
            collect_details,
            observer,
        };
        self.reconcile_repair_containments(&mut run)?;
        self.contain_malformed_digest_entries(
            self.paths.objects_root(),
            ContainedLayoutNamespace::Objects,
            &mut run,
        )?;
        self.contain_malformed_digest_entries(
            self.paths.quarantine_root(),
            ContainedLayoutNamespace::Quarantine,
            &mut run,
        )?;
        self.reconcile_partial_publications(&mut run)?;
        self.remove_temporary_files(&mut run)?;
        self.reconcile_finalized_artifacts(&mut run)?;
        self.reconcile_untracked_quarantine(&mut run)?;
        self.reconcile_untracked_objects(&mut run)?;
        Ok(run.report)
    }

    fn remove_temporary_files(
        &self,
        run: &mut RecoveryRun<'_>,
    ) -> Result<(), ArtifactStoreError> {
        let directory = self.paths.temporary();
        let mut removed = 0_u64;
        for entry in fs::read_dir(directory).map_err(|error| io(StoreOperation::Recover, error))? {
            let entry = entry.map_err(|error| io(StoreOperation::Recover, error))?;
            let file_type =
                entry.file_type().map_err(|error| io(StoreOperation::Recover, error))?;
            if file_type.is_file() || file_type.is_symlink() {
                fs::remove_file(entry.path()).map_err(|error| io(StoreOperation::Remove, error))?;
                removed = increment(removed)?;
            } else {
                self.contain_path(
                    &entry.path(),
                    ContainedLayoutNamespace::Temporary,
                    None,
                    run,
                )?;
            }
        }
        if removed != 0 {
            sync_directory(directory)?;
        }
        run.report.removed_temporary_files = removed;
        Ok(())
    }

    fn contain_malformed_digest_entries(
        &self,
        root: &Path,
        namespace: ContainedLayoutNamespace,
        run: &mut RecoveryRun<'_>,
    ) -> Result<(), ArtifactStoreError> {
        for prefix_entry in
            fs::read_dir(root).map_err(|error| io(StoreOperation::Recover, error))?
        {
            let prefix_entry = prefix_entry.map_err(|error| io(StoreOperation::Recover, error))?;
            let file_type =
                prefix_entry.file_type().map_err(|error| io(StoreOperation::Recover, error))?;
            let prefix_name = prefix_entry.file_name();
            let valid_prefix = file_type.is_dir()
                && prefix_name.to_str().is_some_and(|name| is_lower_hex(name, 2));
            if !valid_prefix {
                self.contain_path(&prefix_entry.path(), namespace, None, run)?;
                continue;
            }
            let prefix = prefix_name
                .to_str()
                .ok_or_else(|| corrupt_layout("digest prefix is not UTF-8"))?;
            for object_entry in fs::read_dir(prefix_entry.path())
                .map_err(|error| io(StoreOperation::Recover, error))?
            {
                let object_entry =
                    object_entry.map_err(|error| io(StoreOperation::Recover, error))?;
                let object_type =
                    object_entry.file_type().map_err(|error| io(StoreOperation::Recover, error))?;
                let name = object_entry.file_name();
                let valid_object = object_type.is_file()
                    && name
                        .to_str()
                        .is_some_and(|name| is_lower_hex(name, 64) && &name[..2] == prefix);
                if !valid_object {
                    self.contain_path(&object_entry.path(), namespace, None, run)?;
                }
            }
        }
        Ok(())
    }

    fn contain_path(
        &self,
        source: &Path,
        namespace: ContainedLayoutNamespace,
        repair_digest: Option<ArtifactDigest>,
        run: &mut RecoveryRun<'_>,
    ) -> Result<(), ArtifactStoreError> {
        let identity = self.catalog.allocate_operation_identity()?;
        let container_name = repair_digest.map_or_else(
            || format!("{identity:016x}-{}", namespace.tag()),
            |digest| format!("{identity:016x}-{}-{}", namespace.tag(), digest.to_hex()),
        );
        let container = self.paths.recovery().join(container_name);
        fs::create_dir(&container).map_err(|error| io(StoreOperation::Recover, error))?;
        sync_directory(self.paths.recovery())?;
        let source_parent = source
            .parent()
            .ok_or_else(|| corrupt_layout("contained entry has no store parent"))?;
        fs::rename(source, container.join("payload"))
            .map_err(|error| io(StoreOperation::MoveQuarantine, error))?;
        sync_directory(&container)?;
        sync_directory(source_parent)?;
        if let Some(digest) = repair_digest
            && self
                .catalog
                .record_repair_containment(digest, identity, namespace)?
                .is_none()
        {
            return Err(corrupt_layout(
                "exact containment lost its durable repair obligation",
            ));
        }
        run.observe(RecoveryObservation::ContainedLayout(ContainedLayoutEntry {
            identity,
            namespace,
            repair_digest,
        }));
        Ok(())
    }

    fn reconcile_repair_containments(
        &self,
        run: &mut RecoveryRun<'_>,
    ) -> Result<(), ArtifactStoreError> {
        for entry in fs::read_dir(self.paths.recovery())
            .map_err(|error| io(StoreOperation::Recover, error))?
        {
            let entry = entry.map_err(|error| io(StoreOperation::Recover, error))?;
            if !entry
                .file_type()
                .map_err(|error| io(StoreOperation::Recover, error))?
                .is_dir()
            {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let Some((identity, namespace, digest)) = parse_repair_container_name(&name)? else {
                continue;
            };
            match fs::symlink_metadata(entry.path().join("payload")) {
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(io(StoreOperation::Recover, error)),
            }
            if self
                .catalog
                .record_repair_containment(digest, identity, namespace)?
                .is_some()
            {
                run.observe(RecoveryObservation::ContainedLayout(ContainedLayoutEntry {
                    identity,
                    namespace,
                    repair_digest: Some(digest),
                }));
            } else {
                fs::remove_dir_all(entry.path())
                    .map_err(|error| io(StoreOperation::Remove, error))?;
                sync_directory(self.paths.recovery())?;
            }
        }
        Ok(())
    }

    fn reconcile_partial_publications(
        &self,
        run: &mut RecoveryRun<'_>,
    ) -> Result<(), ArtifactStoreError> {
        let mut cursor = None;
        loop {
            let partials = self.catalog.partial_publications_after(cursor, 256)?;
            if partials.is_empty() {
                return Ok(());
            }
            for publication in &partials {
                let partial = publication.metadata();
                let digest = partial.digest();
                let object = self.paths.object(digest);
                let quarantine = self.paths.quarantine(digest);
                let temporary = crate::writer::temporary_path(
                    &self.paths,
                    digest,
                    publication.operation_identity(),
                );
                let object_state = verify_path(&object, digest, partial.size())?;
                let quarantine_state = verify_path(&quarantine, digest, partial.size())?;
                let temporary_state = verify_path(&temporary, digest, partial.size())?;
                let observed_reason = if object_state == FileVerification::Corrupt
                    || quarantine_state == FileVerification::Corrupt
                    || temporary_state == FileVerification::Corrupt
                {
                    ArtifactRepairReason::Corrupt
                } else {
                    ArtifactRepairReason::Missing
                };
                let repair_reason = if observed_reason == ArtifactRepairReason::Corrupt {
                    Some(
                        self.catalog
                            .record_repair_obligation(digest, observed_reason)?,
                    )
                } else {
                    None
                };
                let valid_object = match object_state {
                    FileVerification::Valid => true,
                    FileVerification::Corrupt => {
                        self.contain_path(
                            &object,
                            ContainedLayoutNamespace::Objects,
                            Some(digest),
                            run,
                        )?;
                        false
                    }
                    FileVerification::Missing => false,
                };
                let valid_quarantine = match quarantine_state {
                    FileVerification::Valid => true,
                    FileVerification::Corrupt => {
                        self.contain_path(
                            &quarantine,
                            ContainedLayoutNamespace::Quarantine,
                            Some(digest),
                            run,
                        )?;
                        false
                    }
                    FileVerification::Missing => false,
                };
                let valid_temporary = match temporary_state {
                    FileVerification::Valid => true,
                    FileVerification::Corrupt => {
                        self.contain_path(
                            &temporary,
                            ContainedLayoutNamespace::Temporary,
                            Some(digest),
                            run,
                        )?;
                        false
                    }
                    FileVerification::Missing => false,
                };
                if valid_object {
                    if valid_quarantine {
                        remove_file_durable(
                            &quarantine,
                            &self.paths.ensure_quarantine_parent(digest)?,
                        )?;
                    }
                    self.complete_partial_publication(partial, run)?;
                } else if valid_quarantine {
                    self.move_to_objects(digest, partial.size())?;
                    run.report.completed_state_moves =
                        increment(run.report.completed_state_moves)?;
                    self.complete_partial_publication(partial, run)?;
                } else if valid_temporary {
                    let destination_parent = self.paths.ensure_object_parent(digest)?;
                    let mut publication = None;
                    publish(
                        &temporary,
                        &object,
                        &destination_parent,
                        self.paths.temporary(),
                        digest,
                        partial.size(),
                        &mut publication,
                    )?;
                    self.complete_partial_publication(partial, run)?;
                } else {
                    let reason = self
                        .catalog
                        .fence_partial_repair(digest, repair_reason.unwrap_or(observed_reason))?;
                    self.observe_corruption(partial, reason, run)?;
                }
                cursor = Some(digest);
            }
            if partials.len() < 256 {
                return Ok(());
            }
        }
    }

    fn complete_partial_publication(
        &self,
        partial: &ArtifactMetadata,
        run: &mut RecoveryRun<'_>,
    ) -> Result<(), ArtifactStoreError> {
        let finalized = ArtifactMetadata::new(
            partial.digest(),
            partial.size(),
            partial.media_type().clone(),
            partial.encryption().clone(),
            FinalizationState::Finalized,
            partial.creating_event(),
            QuarantineState::Active,
        );
        self.catalog.record_finalized(&finalized, self.config.quota_bytes())?;
        run.report.reconciled_publications =
            increment(run.report.reconciled_publications)?;
        Ok(())
    }

    fn reconcile_finalized_artifacts(
        &self,
        run: &mut RecoveryRun<'_>,
    ) -> Result<(), ArtifactStoreError> {
        let mut cursor = self.catalog.finalized_recovery_cursor()?;
        loop {
            let records = self.catalog.finalized_metadata_after(cursor, 256)?;
            if records.is_empty() {
                if cursor.is_some() {
                    self.catalog.replace_finalized_recovery_cursor(cursor, None)?;
                }
                return Ok(());
            }
            for metadata in &records {
                self.reconcile_finalized(metadata, run)?;
            }
            let next = records.last().map(ArtifactMetadata::digest).ok_or_else(|| {
                corrupt_layout("nonempty artifact recovery page has no final digest")
            })?;
            self.catalog
                .replace_finalized_recovery_cursor(cursor, Some(next))?;
            cursor = Some(next);
            if records.len() < 256 {
                self.catalog.replace_finalized_recovery_cursor(cursor, None)?;
                return Ok(());
            }
        }
    }

    fn reconcile_finalized(
        &self,
        metadata: &ArtifactMetadata,
        run: &mut RecoveryRun<'_>,
    ) -> Result<(), ArtifactStoreError> {
        let digest = metadata.digest();
        let object = self.paths.object(digest);
        let quarantine = self.paths.quarantine(digest);
        let mut object_state = verify_path(&object, digest, metadata.size())?;
        let mut quarantine_state = verify_path(&quarantine, digest, metadata.size())?;
        let observed_reason = if object_state == FileVerification::Corrupt
            || quarantine_state == FileVerification::Corrupt
        {
            ArtifactRepairReason::Corrupt
        } else {
            ArtifactRepairReason::Missing
        };
        let persisted_reason = if metadata.integrity() == IntegrityState::Corrupt {
            Some(self.catalog.fence_finalized_repair(digest, observed_reason)?)
        } else {
            None
        };
        let referenced = self.catalog.has_references(digest)?;
        let restore_referenced = metadata.integrity() == IntegrityState::Healthy
            && referenced
            && matches!(metadata.quarantine(), QuarantineState::Quarantined { .. });
        let expected_quarantine = if metadata.integrity() == IntegrityState::Corrupt {
            true
        } else {
            matches!(metadata.quarantine(), QuarantineState::Quarantined { .. }) && !referenced
        };

        if expected_quarantine {
            if quarantine_state != FileVerification::Valid
                && object_state == FileVerification::Valid
            {
                if quarantine_state == FileVerification::Corrupt {
                    self.contain_path(
                        &quarantine,
                        ContainedLayoutNamespace::Quarantine,
                        persisted_reason.map(|_| digest),
                        run,
                    )?;
                }
                self.move_to_quarantine(digest, metadata.size())?;
                run.report.completed_state_moves =
                    increment(run.report.completed_state_moves)?;
                object_state = FileVerification::Missing;
                quarantine_state = FileVerification::Valid;
            }
            if quarantine_state == FileVerification::Valid {
                match object_state {
                    FileVerification::Valid => remove_file_durable(
                        &object,
                        &self.paths.ensure_object_parent(digest)?,
                    )?,
                    FileVerification::Corrupt => self.contain_path(
                        &object,
                        ContainedLayoutNamespace::Objects,
                        persisted_reason.map(|_| digest),
                        run,
                    )?,
                    FileVerification::Missing => {}
                }
                if metadata.integrity() == IntegrityState::Corrupt {
                    self.observe_corruption(
                        metadata,
                        persisted_reason.unwrap_or(ArtifactRepairReason::Corrupt),
                        run,
                    )?;
                }
                return Ok(());
            }
        } else {
            if object_state != FileVerification::Valid
                && quarantine_state == FileVerification::Valid
            {
                if object_state == FileVerification::Corrupt {
                    self.contain_path(
                        &object,
                        ContainedLayoutNamespace::Objects,
                        persisted_reason.map(|_| digest),
                        run,
                    )?;
                }
                self.move_to_objects(digest, metadata.size())?;
                run.report.completed_state_moves =
                    increment(run.report.completed_state_moves)?;
                object_state = FileVerification::Valid;
                quarantine_state = FileVerification::Missing;
            }
            if object_state == FileVerification::Valid {
                match quarantine_state {
                    FileVerification::Valid => remove_file_durable(
                        &quarantine,
                        &self.paths.ensure_quarantine_parent(digest)?,
                    )?,
                    FileVerification::Corrupt => self.contain_path(
                        &quarantine,
                        ContainedLayoutNamespace::Quarantine,
                        persisted_reason.map(|_| digest),
                        run,
                    )?,
                    FileVerification::Missing => {}
                }
                if restore_referenced {
                    self.catalog.set_quarantine(digest, QuarantineState::Active)?;
                }
                return Ok(());
            }
        }

        let reason = if object_state == FileVerification::Corrupt
            || quarantine_state == FileVerification::Corrupt
        {
            ArtifactRepairReason::Corrupt
        } else {
            ArtifactRepairReason::Missing
        };
        let reason = persisted_reason
            .map_or_else(|| self.catalog.fence_finalized_repair(digest, reason), Ok)?;
        if object_state == FileVerification::Corrupt {
            self.contain_path(
                &object,
                ContainedLayoutNamespace::Objects,
                Some(digest),
                run,
            )?;
        }
        if quarantine_state == FileVerification::Corrupt {
            self.contain_path(
                &quarantine,
                ContainedLayoutNamespace::Quarantine,
                Some(digest),
                run,
            )?;
        }
        self.observe_corruption(metadata, reason, run)
    }

    fn observe_corruption(
        &self,
        metadata: &ArtifactMetadata,
        reason: ArtifactRepairReason,
        run: &mut RecoveryRun<'_>,
    ) -> Result<(), ArtifactStoreError> {
        run.observe(RecoveryObservation::ContainedCorruption(ContainedCorruption {
            digest: metadata.digest(),
            expected_size: metadata.size(),
            reason,
            referenced: self.catalog.has_references(metadata.digest())?,
        }));
        Ok(())
    }

    fn reconcile_untracked_quarantine(
        &self,
        run: &mut RecoveryRun<'_>,
    ) -> Result<(), ArtifactStoreError> {
        self.visit_digest_tree(
            self.paths.quarantine_root(),
            |store, digest, path, run| {
                if store.catalog.metadata(digest)?.is_some() {
                    return Ok(());
                }
                match inspect_file(path, digest) {
                    Ok(_) => {
                        let parent = path
                            .parent()
                            .ok_or_else(|| corrupt_layout("quarantine object has no parent"))?;
                        remove_file_durable(path, parent)?;
                        run.report.removed_swept_files =
                            increment(run.report.removed_swept_files)?;
                        Ok(())
                    }
                    Err(error) if error.code() == ErrorCode::CorruptObject => store.contain_path(
                        path,
                        ContainedLayoutNamespace::Quarantine,
                        None,
                        run,
                    ),
                    Err(error) if error.code() == ErrorCode::MissingArtifact => Ok(()),
                    Err(error) => Err(error),
                }
            },
            run,
        )
    }

    fn reconcile_untracked_objects(
        &self,
        run: &mut RecoveryRun<'_>,
    ) -> Result<(), ArtifactStoreError> {
        self.visit_digest_tree(
            self.paths.objects_root(),
            |store, digest, path, run| {
                if store.catalog.metadata(digest)?.is_some() {
                    return Ok(());
                }
                match inspect_file(path, digest) {
                    Ok(size) => {
                        store.move_to_quarantine(digest, size)?;
                        run.observe(RecoveryObservation::QuarantinedOrphan(
                            QuarantinedArtifact { digest, size },
                        ));
                        Ok(())
                    }
                    Err(error) if error.code() == ErrorCode::CorruptObject => {
                        store.contain_path(
                            path,
                            ContainedLayoutNamespace::Objects,
                            None,
                            run,
                        )
                    }
                    Err(error) if error.code() == ErrorCode::MissingArtifact => Ok(()),
                    Err(error) => Err(error),
                }
            },
            run,
        )
    }

    fn visit_digest_tree<F>(
        &self,
        root: &Path,
        mut visit: F,
        run: &mut RecoveryRun<'_>,
    ) -> Result<(), ArtifactStoreError>
    where
        F: FnMut(
            &Self,
            ArtifactDigest,
            &Path,
            &mut RecoveryRun<'_>,
        ) -> Result<(), ArtifactStoreError>,
    {
        for prefix_entry in
            fs::read_dir(root).map_err(|error| io(StoreOperation::Recover, error))?
        {
            let prefix_entry = prefix_entry.map_err(|error| io(StoreOperation::Recover, error))?;
            let prefix = prefix_entry.file_name();
            let prefix = prefix
                .to_str()
                .ok_or_else(|| corrupt_layout("digest prefix changed during recovery"))?;
            for object_entry in fs::read_dir(prefix_entry.path())
                .map_err(|error| io(StoreOperation::Recover, error))?
            {
                let object_entry =
                    object_entry.map_err(|error| io(StoreOperation::Recover, error))?;
                let name = object_entry.file_name();
                let name = name
                    .to_str()
                    .ok_or_else(|| corrupt_layout("digest filename changed during recovery"))?;
                if !is_lower_hex(name, 64) || &name[..2] != prefix {
                    return Err(corrupt_layout("digest layout changed during recovery"));
                }
                let digest = ArtifactDigest::parse_internal_hex(name)?;
                visit(self, digest, &object_entry.path(), run)?;
            }
        }
        Ok(())
    }
}

fn remove_file_durable(path: &Path, parent: &Path) -> Result<(), ArtifactStoreError> {
    fs::remove_file(path).map_err(|error| io(StoreOperation::Remove, error))?;
    sync_directory(parent)
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum FileVerification {
    Missing,
    Valid,
    Corrupt,
}

fn verify_path(
    path: &Path,
    digest: ArtifactDigest,
    size: u64,
) -> Result<FileVerification, ArtifactStoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.file_type().is_file() => Ok(FileVerification::Corrupt),
        Ok(_) => match verify_finalized(path, digest, size) {
            Ok(()) => Ok(FileVerification::Valid),
            Err(error) if error.code() == ErrorCode::CorruptObject => {
                Ok(FileVerification::Corrupt)
            }
            Err(error) if error.code() == ErrorCode::MissingArtifact => {
                Ok(FileVerification::Missing)
            }
            Err(error) => Err(error),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(FileVerification::Missing)
        }
        Err(error) => Err(io(StoreOperation::Recover, error)),
    }
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn parse_repair_container_name(
    name: &str,
) -> Result<Option<(u64, ContainedLayoutNamespace, ArtifactDigest)>, ArtifactStoreError> {
    let mut parts = name.split('-');
    let (Some(identity), Some(namespace), Some(digest), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Ok(None);
    };
    if !is_lower_hex(identity, 16) || !is_lower_hex(digest, 64) {
        return Ok(None);
    }
    let namespace = match namespace {
        "objects" => ContainedLayoutNamespace::Objects,
        "quarantine" => ContainedLayoutNamespace::Quarantine,
        "temporary" => ContainedLayoutNamespace::Temporary,
        _ => return Ok(None),
    };
    let identity = u64::from_str_radix(identity, 16)
        .map_err(|_| corrupt_layout("repair-containment identity is invalid"))?;
    let digest = ArtifactDigest::parse_internal_hex(digest)?;
    Ok(Some((identity, namespace, digest)))
}

fn increment(value: u64) -> Result<u64, ArtifactStoreError> {
    value.checked_add(1).ok_or_else(|| {
        ArtifactStoreError::message(
            ErrorCode::ArithmeticOverflow,
            RecoveryClass::RecoverStore,
            "recovery observation count overflowed",
        )
    })
}

const fn corrupt_layout(message: &'static str) -> ArtifactStoreError {
    ArtifactStoreError::message(ErrorCode::CorruptObject, RecoveryClass::TerminalIntegrity, message)
}
