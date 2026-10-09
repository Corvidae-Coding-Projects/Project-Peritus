//! Preflighted, resumable, and atomic portable bundle assembly.

use super::format::invalid;
use super::{BundleLimits, BundlePlan};
use crate::{EvidenceCancellation, EvidenceError};
use peritus_artifact_store::ArtifactStore;
use peritus_types::Sha256Digest;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{ErrorKind, Read, Seek, SeekFrom, Write};
use std::path::Path;
use tempfile::NamedTempFile;

const STREAM_CHUNK_BYTES: usize = 64 * 1024;

/// Stable restart position for one exact deterministic bundle export.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BundleExportCursor {
    export_id: Sha256Digest,
    byte_offset: u64,
}

impl BundleExportCursor {
    /// Restores a retained cursor. Its identity and offset are checked before output is touched.
    #[must_use]
    pub const fn new(export_id: Sha256Digest, byte_offset: u64) -> Self {
        Self { export_id, byte_offset }
    }

    /// Starts the exact export described by a plan.
    #[must_use]
    pub const fn start(plan: &BundlePlan) -> Self {
        Self { export_id: plan.export_id(), byte_offset: 0 }
    }

    /// Returns the stable export identity.
    #[must_use]
    pub const fn export_id(self) -> Sha256Digest {
        self.export_id
    }

    /// Returns the next byte offset not yet accepted by the caller's output.
    #[must_use]
    pub const fn byte_offset(self) -> u64 {
        self.byte_offset
    }
}

/// Durable meaning of an owned bundle export operation's retained cursor.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BundleExportPhase {
    /// Exact bytes are privately staged and no caller output has been accepted yet.
    Prepared,
    /// Caller output has accepted a prefix and can resume at the retained cursor.
    Writing,
    /// Every planned byte was accepted by caller output.
    Complete,
}

/// Owned authenticated stage and output cursor for one exact bundle export.
pub struct BundleExportOperation {
    staged: StagedBundle,
    cursor: BundleExportCursor,
    phase: BundleExportPhase,
}

impl BundleExportOperation {
    pub(super) const fn from_staged(staged: StagedBundle) -> Self {
        let cursor = BundleExportCursor::new(staged.receipt.export_id, 0);
        Self { staged, cursor, phase: BundleExportPhase::Prepared }
    }

    /// Authenticates and stages a new exact export under explicit cancellation ownership.
    ///
    /// # Errors
    ///
    /// Returns planning, artifact, staging, capacity, or cancellation failures.
    pub fn prepare(
        plan: &BundlePlan,
        artifacts: &ArtifactStore,
        limits: BundleLimits,
        cancellation: &EvidenceCancellation,
    ) -> Result<Self, EvidenceError> {
        Self::restore(plan, artifacts, limits, BundleExportCursor::start(plan), cancellation)
    }

    /// Restages exact bytes for a retained restart cursor, then owns subsequent output progress.
    ///
    /// # Errors
    ///
    /// Rejects a cursor for different bytes or an invalid offset, plus preparation failures.
    pub fn restore(
        plan: &BundlePlan,
        artifacts: &ArtifactStore,
        limits: BundleLimits,
        cursor: BundleExportCursor,
        cancellation: &EvidenceCancellation,
    ) -> Result<Self, EvidenceError> {
        if cursor.export_id != plan.export_id() || cursor.byte_offset > plan.byte_count() {
            return Err(invalid("bundle export cursor does not belong to this exact plan"));
        }
        ensure_live(Some(cancellation), "prepare evidence bundle export")?;
        let staged =
            StagedBundle::create_in(artifacts.root(), plan, artifacts, limits, Some(cancellation))?;
        let phase = if cursor.byte_offset == staged.receipt.byte_count {
            BundleExportPhase::Complete
        } else if cursor.byte_offset == 0 {
            BundleExportPhase::Prepared
        } else {
            BundleExportPhase::Writing
        };
        Ok(Self { staged, cursor, phase })
    }

    /// Returns the retained exact export cursor.
    #[must_use]
    pub const fn cursor(&self) -> BundleExportCursor {
        self.cursor
    }

    /// Returns the truthful current export phase.
    #[must_use]
    pub const fn phase(&self) -> BundleExportPhase {
        self.phase
    }

    /// Returns the complete receipt only after every output byte was accepted.
    #[must_use]
    pub const fn receipt(&self) -> Option<BundleReceipt> {
        if matches!(self.phase, BundleExportPhase::Complete) {
            Some(self.staged.receipt)
        } else {
            None
        }
    }

    /// Copies at most the caller-selected bytes from the authenticated stage.
    ///
    /// Returns a receipt only when every byte has been accepted by output. The output must be
    /// positioned at [`Self::cursor`]; partial I/O failures retain every accepted byte.
    ///
    /// # Errors
    /// Returns cancellation, stage, or output I/O failures without losing its cursor.
    pub fn advance<W: Write>(
        &mut self,
        output: &mut W,
        bytes: std::num::NonZeroUsize,
        cancellation: &EvidenceCancellation,
    ) -> Result<Option<BundleReceipt>, EvidenceError> {
        if matches!(self.phase, BundleExportPhase::Complete) {
            return Ok(Some(self.staged.receipt));
        }
        self.phase = BundleExportPhase::Writing;
        copy_staged(
            &mut self.staged,
            output,
            &mut self.cursor,
            Some(cancellation),
            bytes.get() as u64,
        )?;
        if self.cursor.byte_offset == self.staged.receipt.byte_count {
            self.phase = BundleExportPhase::Complete;
        }
        Ok(self.receipt())
    }

    /// Resumes output from the retained cursor without rebuilding or rehashing the stage.
    ///
    /// # Errors
    ///
    /// Returns output, staged-content, or cancellation failures while retaining exact progress.
    pub fn resume<W: Write>(
        &mut self,
        output: &mut W,
        cancellation: &EvidenceCancellation,
    ) -> Result<BundleReceipt, EvidenceError> {
        if matches!(self.phase, BundleExportPhase::Complete) {
            return Ok(self.staged.receipt);
        }
        self.phase = BundleExportPhase::Writing;
        copy_staged(&mut self.staged, output, &mut self.cursor, Some(cancellation), u64::MAX)?;
        self.phase = BundleExportPhase::Complete;
        Ok(self.staged.receipt)
    }
}

/// Successful streaming bundle observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BundleReceipt {
    pub(super) export_id: Sha256Digest,
    pub(super) root_digest: Sha256Digest,
    pub(super) bundle_digest: Sha256Digest,
    pub(super) byte_count: u64,
}

impl BundleReceipt {
    /// Returns the stable identity used by resumable cursors.
    #[must_use]
    pub const fn export_id(self) -> Sha256Digest {
        self.export_id
    }
    /// Returns the manifest-bound portable root digest.
    #[must_use]
    pub const fn root_digest(self) -> Sha256Digest {
        self.root_digest
    }
    /// Returns SHA-256 over every assembled bundle byte.
    #[must_use]
    pub const fn bundle_digest(self) -> Sha256Digest {
        self.bundle_digest
    }
    /// Returns the exact assembled byte count.
    #[must_use]
    pub const fn byte_count(self) -> u64 {
        self.byte_count
    }
}

/// Preflights a complete deterministic bundle before copying it to the supplied output.
///
/// Artifact, digest, size, capacity, and caller-budget failures occur in private staging before
/// caller output is touched. Use [`resume_bundle`] when output I/O itself may need continuation.
///
/// # Errors
///
/// Returns stale/corrupt artifact, staging, output I/O, representation, or selected-limit failures.
pub fn assemble_bundle<W: Write>(
    plan: &BundlePlan,
    artifacts: &ArtifactStore,
    mut output: W,
    limits: BundleLimits,
) -> Result<BundleReceipt, EvidenceError> {
    let mut cursor = BundleExportCursor::start(plan);
    resume_bundle(plan, artifacts, &mut output, limits, &mut cursor)
}

/// Resumes one stable export from the cursor's next byte.
///
/// The output must append at the cursor's retained offset. The cursor advances after every write
/// accepted by the output and remains usable after an output error.
///
/// # Errors
///
/// Rejects a cursor for another export or an offset beyond the exact completed bundle, in addition
/// to the failures documented by [`assemble_bundle`].
pub fn resume_bundle<W: Write>(
    plan: &BundlePlan,
    artifacts: &ArtifactStore,
    output: &mut W,
    limits: BundleLimits,
    cursor: &mut BundleExportCursor,
) -> Result<BundleReceipt, EvidenceError> {
    if cursor.export_id != plan.export_id() || cursor.byte_offset > plan.byte_count() {
        return Err(invalid("bundle export cursor does not belong to this exact plan"));
    }
    let mut staged = StagedBundle::create_in(artifacts.root(), plan, artifacts, limits, None)?;
    if staged.receipt.export_id != cursor.export_id
        || staged.receipt.byte_count != plan.byte_count()
    {
        return Err(invalid("staged bundle disagrees with its serialized plan"));
    }
    copy_staged(&mut staged, output, cursor, None, u64::MAX)?;
    Ok(staged.receipt)
}

fn copy_staged<W: Write>(
    staged: &mut StagedBundle,
    output: &mut W,
    cursor: &mut BundleExportCursor,
    cancellation: Option<&EvidenceCancellation>,
    maximum_bytes: u64,
) -> Result<(), EvidenceError> {
    ensure_live(cancellation, "copy staged evidence bundle")?;
    staged
        .file
        .as_file_mut()
        .seek(SeekFrom::Start(cursor.byte_offset))
        .map_err(|error| EvidenceError::io("seek staged evidence bundle", error))?;
    ensure_live(cancellation, "copy staged evidence bundle")?;
    let mut buffer = vec![0_u8; STREAM_CHUNK_BYTES].into_boxed_slice();
    let end = cursor.byte_offset.saturating_add(maximum_bytes).min(staged.receipt.byte_count);
    while cursor.byte_offset < end {
        let remaining = end - cursor.byte_offset;
        let wanted = usize::try_from(remaining.min(STREAM_CHUNK_BYTES as u64))
            .map_err(|_| invalid("bundle continuation size exceeds usize"))?;
        ensure_live(cancellation, "read staged evidence bundle")?;
        let read = staged
            .file
            .as_file_mut()
            .read(&mut buffer[..wanted])
            .map_err(|error| EvidenceError::io("read staged evidence bundle", error))?;
        ensure_live(cancellation, "read staged evidence bundle")?;
        if read == 0 {
            return Err(invalid("staged evidence bundle is truncated"));
        }
        write_progress(output, &buffer[..read], cursor, cancellation)?;
    }
    Ok(())
}

/// Publishes one complete bundle through a synchronized same-directory temporary file.
///
/// Publication is no-clobber and idempotent for an already-published exact bundle.
///
/// # Errors
///
/// Returns planning, artifact, capacity, staging, synchronization, conflict, or publication I/O
/// failures without exposing a partial destination.
pub fn publish_bundle(
    plan: &BundlePlan,
    artifacts: &ArtifactStore,
    destination: impl AsRef<Path>,
    limits: BundleLimits,
) -> Result<BundleReceipt, EvidenceError> {
    let destination = destination.as_ref();
    let parent = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let staged = StagedBundle::create_in(parent, plan, artifacts, limits, None)?;
    let receipt = staged.receipt;
    match staged.file.persist_noclobber(destination) {
        Ok(file) => {
            file.sync_all().map_err(|error| {
                EvidenceError::io("synchronize published evidence bundle", error)
            })?;
            #[cfg(unix)]
            sync_directory(parent)?;
            Ok(receipt)
        }
        Err(error) if error.error.kind() == ErrorKind::AlreadyExists => {
            exact_existing(destination, receipt)?;
            Ok(receipt)
        }
        Err(error) => Err(EvidenceError::io("publish evidence bundle", error.error)),
    }
}

pub(super) struct StagedBundle {
    pub(super) file: NamedTempFile,
    pub(super) receipt: BundleReceipt,
}

impl StagedBundle {
    fn create_in(
        directory: &Path,
        plan: &BundlePlan,
        artifacts: &ArtifactStore,
        limits: BundleLimits,
        cancellation: Option<&EvidenceCancellation>,
    ) -> Result<Self, EvidenceError> {
        let mut preparation =
            super::preparation::BundlePreparation::new_in(directory, plan, artifacts, limits)?;
        let live = EvidenceCancellation::new();
        let cancellation = cancellation.unwrap_or(&live);
        while preparation
            .advance(
                const { std::num::NonZeroUsize::new(STREAM_CHUNK_BYTES).unwrap() },
                cancellation,
            )?
            .is_none()
        {}
        preparation.into_staged()
    }
}

pub(super) fn ensure_capacity(directory: &Path, required: u64) -> Result<(), EvidenceError> {
    let stats = fs4::statvfs(directory)
        .map_err(|error| EvidenceError::io("observe evidence bundle staging capacity", error))?;
    if stats.available_space() < required {
        return Err(EvidenceError::io(
            "reserve evidence bundle staging capacity",
            std::io::Error::other(format!(
                "bundle needs {required} bytes but only {} are available",
                stats.available_space()
            )),
        ));
    }
    Ok(())
}

fn write_progress<W: Write>(
    output: &mut W,
    mut bytes: &[u8],
    cursor: &mut BundleExportCursor,
    cancellation: Option<&EvidenceCancellation>,
) -> Result<(), EvidenceError> {
    while !bytes.is_empty() {
        ensure_live(cancellation, "write evidence bundle")?;
        match output.write(bytes) {
            Ok(0) => {
                return Err(EvidenceError::io(
                    "write evidence bundle",
                    std::io::Error::from(ErrorKind::WriteZero),
                ));
            }
            Ok(written) => {
                cursor.byte_offset = cursor
                    .byte_offset
                    .checked_add(
                        u64::try_from(written)
                            .map_err(|_| invalid("written bundle length exceeds u64"))?,
                    )
                    .ok_or_else(|| invalid("bundle cursor offset overflowed"))?;
                bytes = &bytes[written..];
                ensure_live(cancellation, "write evidence bundle")?;
            }
            Err(error) => return Err(EvidenceError::io("write evidence bundle", error)),
        }
    }
    Ok(())
}

fn exact_existing(path: &Path, expected: BundleReceipt) -> Result<(), EvidenceError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| EvidenceError::io("inspect published evidence bundle", error))?;
    if !metadata.file_type().is_file() || metadata.len() != expected.byte_count {
        return Err(invalid("published bundle path owns different content"));
    }
    let mut file = File::open(path)
        .map_err(|error| EvidenceError::io("open published evidence bundle", error))?;
    let mut digest = Sha256::new();
    let mut count = 0_u64;
    let mut buffer = vec![0_u8; STREAM_CHUNK_BYTES].into_boxed_slice();
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| EvidenceError::io("read published evidence bundle", error))?;
        if read == 0 {
            break;
        }
        count = count
            .checked_add(u64::try_from(read).map_err(|_| invalid("bundle read exceeds u64"))?)
            .ok_or_else(|| invalid("published bundle byte count overflowed"))?;
        digest.update(&buffer[..read]);
    }
    if count != expected.byte_count
        || Sha256Digest::new(digest.finalize().into()) != expected.bundle_digest
    {
        return Err(invalid("published bundle path owns different content"));
    }
    file.sync_all()
        .map_err(|error| EvidenceError::io("synchronize published evidence bundle", error))
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), EvidenceError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| EvidenceError::io("synchronize evidence bundle directory", error))
}

fn ensure_live(
    cancellation: Option<&EvidenceCancellation>,
    operation: &'static str,
) -> Result<(), EvidenceError> {
    if cancellation.is_some_and(EvidenceCancellation::is_cancelled) {
        Err(EvidenceError::cancelled(operation))
    } else {
        Ok(())
    }
}
