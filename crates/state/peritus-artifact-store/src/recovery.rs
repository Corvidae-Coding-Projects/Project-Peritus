//! Idempotent restart recovery for temporary, object, quarantine, and catalog state.

use std::{collections::BTreeMap, fs, path::Path};

use crate::{
    ArtifactDigest, ArtifactStore, ArtifactStoreError, ErrorCode, IntegrityState, QuarantineState,
    RecoveryClass, StoreOperation,
    finalize::{inspect_file, verify_finalized},
    path::{io, sync_directory},
};

/// Orphaned finalized bytes moved to quarantine during recovery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QuarantinedArtifact {
    digest: ArtifactDigest,
    size: u64,
}

/// Cataloged artifact whose divergent bytes were durably contained during recovery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContainedCorruption {
    digest: ArtifactDigest,
    expected_size: u64,
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
    quarantined_orphans: Vec<QuarantinedArtifact>,
    contained_corruptions: Vec<ContainedCorruption>,
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

    /// Returns finalized objects with no durable record that were conservatively quarantined.
    #[must_use]
    pub fn quarantined_orphans(&self) -> &[QuarantinedArtifact] {
        &self.quarantined_orphans
    }

    /// Returns cataloged artifacts whose divergent bytes were made non-referenceable and moved
    /// out of the active object namespace.
    #[must_use]
    pub fn contained_corruptions(&self) -> &[ContainedCorruption] {
        &self.contained_corruptions
    }
}

impl ArtifactStore {
    /// Reconciles crash windows and removes abandoned temporary files.
    ///
    /// Durable catalog state directs interrupted quarantine moves. Untracked active objects are
    /// first moved to quarantine; untracked files already in quarantine are deleted on this later
    /// recovery pass. Every digest-named file is re-hashed before it is trusted or moved.
    ///
    /// # Errors
    ///
    /// Returns an I/O or terminal integrity error for malformed layouts, missing recorded bytes, or
    /// content that disagrees with its digest path.
    pub fn recover(&mut self) -> Result<RecoveryReport, ArtifactStoreError> {
        let mut report = RecoveryReport {
            removed_temporary_files: remove_temporary_files(self.paths.temporary())?,
            ..RecoveryReport::default()
        };

        let inventory: BTreeMap<_, _> = self
            .catalog
            .recovery_inventory()?
            .into_iter()
            .map(|entry| (entry.digest(), entry))
            .collect();
        for (&digest, entry) in &inventory {
            let object = self.paths.object(digest);
            let quarantine = self.paths.quarantine(digest);
            let object_exists = regular_file_exists(&object)?;
            let quarantine_exists = regular_file_exists(&quarantine)?;
            let metadata = self.catalog.metadata(digest)?.ok_or_else(missing_recorded_file)?;
            if metadata.integrity() == IntegrityState::Corrupt {
                self.recover_contained_corruption(digest, object_exists, quarantine_exists)?;
                continue;
            }
            match entry.quarantine() {
                QuarantineState::Active => match (object_exists, quarantine_exists) {
                    (true, false) => {
                        if let Err(error) = verify_finalized(&object, digest, entry.size()) {
                            if error.code() != ErrorCode::CorruptObject {
                                return Err(error);
                            }
                            self.catalog.set_integrity(digest, IntegrityState::Corrupt)?;
                            self.move_corrupt_to_quarantine(digest)?;
                            report
                                .contained_corruptions
                                .push(ContainedCorruption { digest, expected_size: entry.size() });
                        }
                    }
                    (false, true) => {
                        verify_finalized(&quarantine, digest, entry.size())?;
                        self.move_to_objects(digest, entry.size())?;
                        report.completed_state_moves = increment(report.completed_state_moves)?;
                    }
                    (true, true) => {
                        verify_finalized(&object, digest, entry.size())?;
                        verify_finalized(&quarantine, digest, entry.size())?;
                        fs::remove_file(&quarantine)
                            .map_err(|error| io(StoreOperation::Remove, error))?;
                        sync_directory(&self.paths.ensure_quarantine_parent(digest)?)?;
                        report.completed_state_moves = increment(report.completed_state_moves)?;
                    }
                    (false, false) => return Err(missing_recorded_file()),
                },
                QuarantineState::Quarantined { .. } => match (object_exists, quarantine_exists) {
                    (false, true) => verify_finalized(&quarantine, digest, entry.size())?,
                    (true, false) => {
                        verify_finalized(&object, digest, entry.size())?;
                        self.move_to_quarantine(digest, entry.size())?;
                        report.completed_state_moves = increment(report.completed_state_moves)?;
                    }
                    (true, true) => {
                        verify_finalized(&object, digest, entry.size())?;
                        verify_finalized(&quarantine, digest, entry.size())?;
                        fs::remove_file(&object)
                            .map_err(|error| io(StoreOperation::Remove, error))?;
                        sync_directory(&self.paths.ensure_object_parent(digest)?)?;
                        report.completed_state_moves = increment(report.completed_state_moves)?;
                    }
                    (false, false) => return Err(missing_recorded_file()),
                },
            }
        }

        // A sweep removes its durable row before its quarantine file. A crash in between is
        // therefore completed on the next recovery pass.
        for digest in scan_digest_tree(self.paths.quarantine_root())? {
            if !inventory.contains_key(&digest) {
                let path = self.paths.quarantine(digest);
                inspect_file(&path, digest)?;
                fs::remove_file(path).map_err(|error| io(StoreOperation::Remove, error))?;
                sync_directory(&self.paths.ensure_quarantine_parent(digest)?)?;
                report.removed_swept_files = increment(report.removed_swept_files)?;
            }
        }

        // Publication precedes catalog insertion. If the process dies in that narrow window, keep
        // the verified bytes for one quarantine cycle instead of deleting them immediately.
        for digest in scan_digest_tree(self.paths.objects_root())? {
            if !inventory.contains_key(&digest) {
                let size = inspect_file(&self.paths.object(digest), digest)?;
                self.move_to_quarantine(digest, size)?;
                report.quarantined_orphans.push(QuarantinedArtifact { digest, size });
            }
        }
        Ok(report)
    }

    fn recover_contained_corruption(
        &self,
        digest: ArtifactDigest,
        object_exists: bool,
        quarantine_exists: bool,
    ) -> Result<(), ArtifactStoreError> {
        match (object_exists, quarantine_exists) {
            (true, false) => self.move_corrupt_to_quarantine(digest),
            (false, true) => Ok(()),
            (true, true) => Err(corrupt_layout(
                "contained corruption exists in active and quarantine namespaces",
            )),
            (false, false) => Err(missing_recorded_file()),
        }
    }

    fn move_corrupt_to_quarantine(&self, digest: ArtifactDigest) -> Result<(), ArtifactStoreError> {
        let source = self.paths.object(digest);
        let destination = self.paths.quarantine(digest);
        let source_parent = self.paths.ensure_object_parent(digest)?;
        let destination_parent = self.paths.ensure_quarantine_parent(digest)?;
        fs::rename(source, destination)
            .map_err(|error| io(StoreOperation::MoveQuarantine, error))?;
        sync_directory(&destination_parent)?;
        sync_directory(&source_parent)
    }
}

fn remove_temporary_files(directory: &Path) -> Result<u64, ArtifactStoreError> {
    let mut removed = 0_u64;
    for entry in fs::read_dir(directory).map_err(|error| io(StoreOperation::Recover, error))? {
        let entry = entry.map_err(|error| io(StoreOperation::Recover, error))?;
        let file_type = entry.file_type().map_err(|error| io(StoreOperation::Recover, error))?;
        if !file_type.is_file() && !file_type.is_symlink() {
            return Err(corrupt_layout("temporary directory contains a non-file entry"));
        }
        fs::remove_file(entry.path()).map_err(|error| io(StoreOperation::Remove, error))?;
        removed = increment(removed)?;
    }
    if removed != 0 {
        sync_directory(directory)?;
    }
    Ok(removed)
}

fn scan_digest_tree(root: &Path) -> Result<Vec<ArtifactDigest>, ArtifactStoreError> {
    let mut digests = Vec::new();
    for prefix_entry in fs::read_dir(root).map_err(|error| io(StoreOperation::Recover, error))? {
        let prefix_entry = prefix_entry.map_err(|error| io(StoreOperation::Recover, error))?;
        let prefix_type =
            prefix_entry.file_type().map_err(|error| io(StoreOperation::Recover, error))?;
        let prefix = prefix_entry.file_name();
        let prefix = prefix.to_str().ok_or_else(|| corrupt_layout("non-UTF-8 digest prefix"))?;
        if !prefix_type.is_dir()
            || prefix.len() != 2
            || !prefix.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(corrupt_layout("noncanonical digest prefix directory"));
        }
        for object_entry in
            fs::read_dir(prefix_entry.path()).map_err(|error| io(StoreOperation::Recover, error))?
        {
            let object_entry = object_entry.map_err(|error| io(StoreOperation::Recover, error))?;
            if !object_entry
                .file_type()
                .map_err(|error| io(StoreOperation::Recover, error))?
                .is_file()
            {
                return Err(corrupt_layout("digest directory contains a non-regular file"));
            }
            let name = object_entry.file_name();
            let name = name.to_str().ok_or_else(|| corrupt_layout("non-UTF-8 digest filename"))?;
            let digest = ArtifactDigest::parse_internal_hex(name)?;
            if &name[..2] != prefix {
                return Err(corrupt_layout("digest filename is under the wrong prefix"));
            }
            digests.push(digest);
        }
    }
    digests.sort_unstable();
    Ok(digests)
}

fn regular_file_exists(path: &Path) -> Result<bool, ArtifactStoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(true),
        Ok(_) => Err(corrupt_layout("artifact path is not a regular file")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(io(StoreOperation::Recover, error)),
    }
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

const fn missing_recorded_file() -> ArtifactStoreError {
    ArtifactStoreError::message(
        ErrorCode::MissingArtifact,
        RecoveryClass::TerminalIntegrity,
        "durable artifact metadata has no object or quarantine file",
    )
}
