//! Preflighted, resumable, and atomic portable bundle assembly.

use super::format::{MAGIC, invalid};
use super::{BundleLimits, BundlePlan};
use crate::{EvidenceError, EvidenceErrorKind, RecoveryAction};
use peritus_artifact_store::{ArtifactDigest, ArtifactStore};
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
    pub fn start(plan: &BundlePlan) -> Self {
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

/// Successful streaming bundle observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BundleReceipt {
    export_id: Sha256Digest,
    root_digest: Sha256Digest,
    bundle_digest: Sha256Digest,
    byte_count: u64,
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
    let mut staged = StagedBundle::create_in(artifacts.root(), plan, artifacts, limits)?;
    if staged.receipt.export_id != cursor.export_id
        || staged.receipt.byte_count != plan.byte_count()
    {
        return Err(invalid("staged bundle disagrees with its serialized plan"));
    }
    staged
        .file
        .as_file_mut()
        .seek(SeekFrom::Start(cursor.byte_offset))
        .map_err(|error| EvidenceError::io("seek staged evidence bundle", error))?;
    let mut buffer = vec![0_u8; STREAM_CHUNK_BYTES].into_boxed_slice();
    while cursor.byte_offset < staged.receipt.byte_count {
        let remaining = staged.receipt.byte_count - cursor.byte_offset;
        let wanted = usize::try_from(remaining.min(STREAM_CHUNK_BYTES as u64))
            .map_err(|_| invalid("bundle continuation size exceeds usize"))?;
        let read = staged
            .file
            .as_file_mut()
            .read(&mut buffer[..wanted])
            .map_err(|error| EvidenceError::io("read staged evidence bundle", error))?;
        if read == 0 {
            return Err(invalid("staged evidence bundle is truncated"));
        }
        write_progress(output, &buffer[..read], cursor)?;
    }
    Ok(staged.receipt)
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
    let staged = StagedBundle::create_in(parent, plan, artifacts, limits)?;
    let receipt = staged.receipt;
    match staged.file.persist_noclobber(destination) {
        Ok(file) => {
            file.sync_all()
                .map_err(|error| EvidenceError::io("synchronize published evidence bundle", error))?;
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

struct StagedBundle {
    file: NamedTempFile,
    receipt: BundleReceipt,
}

impl StagedBundle {
    fn create_in(
        directory: &Path,
        plan: &BundlePlan,
        artifacts: &ArtifactStore,
        limits: BundleLimits,
    ) -> Result<Self, EvidenceError> {
        plan.validate_limits(limits)?;
        ensure_capacity(directory, plan.byte_count())?;
        let mut file = NamedTempFile::new_in(directory)
            .map_err(|error| EvidenceError::io("create staged evidence bundle", error))?;
        let mut writer = HashingWriter::new(file.as_file_mut(), limits.max_bundle_bytes());
        write_bundle(plan, artifacts, &mut writer, limits)?;
        let (bundle_digest, byte_count) = writer.finish();
        if byte_count != plan.byte_count() {
            return Err(invalid("assembled bundle byte count disagrees with plan"));
        }
        file.as_file_mut()
            .flush()
            .and_then(|()| file.as_file().sync_all())
            .map_err(|error| EvidenceError::io("synchronize staged evidence bundle", error))?;
        Ok(Self {
            file,
            receipt: BundleReceipt {
                export_id: plan.export_id(),
                root_digest: plan.manifest().root_digest(),
                bundle_digest,
                byte_count,
            },
        })
    }
}

fn write_bundle<W: Write>(
    plan: &BundlePlan,
    artifacts: &ArtifactStore,
    writer: &mut HashingWriter<W>,
    limits: BundleLimits,
) -> Result<(), EvidenceError> {
    writer.write(MAGIC)?;
    let manifest_size = plan.manifest().canonical_size()?;
    write_sized_header(writer, manifest_size, limits.max_entry_bytes())?;
    plan.manifest().write_canonical(&mut |bytes| writer.write(bytes))?;
    write_count(writer, plan.records().len(), limits.max_entries())?;
    for record in plan.records() {
        let size = record.canonical_size()?;
        write_sized_header(writer, size, limits.max_entry_bytes())?;
        record.write_canonical(&mut |bytes| writer.write(bytes))?;
    }
    write_count(writer, plan.frames().len(), limits.max_entries())?;
    for frame in plan.frames() {
        write_u64(writer, frame.entry.global_position())?;
        write_sized(writer, &frame.bytes, limits.max_entry_bytes())?;
    }
    write_count(writer, plan.manifest().artifacts().len(), limits.max_entries())?;
    for entry in plan.manifest().artifacts() {
        writer.write(entry.digest().as_bytes())?;
        write_u64(writer, entry.size())?;
        stream_artifact(writer, artifacts, entry.digest(), entry.size(), limits)?;
    }
    writer.write(plan.manifest().root_digest().as_bytes())
}

fn stream_artifact<W: Write>(
    writer: &mut HashingWriter<W>,
    store: &ArtifactStore,
    digest: ArtifactDigest,
    expected_size: u64,
    limits: BundleLimits,
) -> Result<(), EvidenceError> {
    limits.check_entry_bytes(expected_size)?;
    let mut reader = store
        .open_read(digest)
        .map_err(|error| EvidenceError::artifact("open authenticated bundle artifact", error))?;
    if reader.metadata().size() != expected_size {
        return Err(invalid("artifact size changed after planning"));
    }
    let chunk_bytes = usize::try_from(expected_size.clamp(1, STREAM_CHUNK_BYTES as u64))
        .map_err(|_| invalid("artifact chunk size exceeds usize"))?;
    let mut count = 0_u64;
    while let Some(chunk) = reader
        .read_chunk(chunk_bytes)
        .map_err(|error| EvidenceError::artifact("read authenticated bundle artifact", error))?
    {
        let length = u64::try_from(chunk.bytes().len())
            .map_err(|_| invalid("artifact chunk length exceeds u64"))?;
        count = count.checked_add(length).ok_or_else(|| {
            EvidenceError::new(
                EvidenceErrorKind::ArithmeticOverflow,
                RecoveryAction::CorrectInput,
                "assemble evidence bundle",
                "artifact byte count overflowed",
            )
        })?;
        writer.write(chunk.bytes())?;
    }
    if count != expected_size {
        return Err(invalid("authenticated artifact size disagrees with manifest"));
    }
    Ok(())
}

fn ensure_capacity(directory: &Path, required: u64) -> Result<(), EvidenceError> {
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
) -> Result<(), EvidenceError> {
    while !bytes.is_empty() {
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

fn sync_directory(path: &Path) -> Result<(), EvidenceError> {
    #[cfg(unix)]
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| EvidenceError::io("synchronize evidence bundle directory", error))?;
    let _ = path;
    Ok(())
}

struct HashingWriter<W> {
    inner: W,
    hasher: Sha256,
    count: u64,
    limit: Option<u64>,
}

impl<W: Write> HashingWriter<W> {
    fn new(inner: W, limit: Option<u64>) -> Self {
        Self { inner, hasher: Sha256::new(), count: 0, limit }
    }
    fn write(&mut self, bytes: &[u8]) -> Result<(), EvidenceError> {
        let length =
            u64::try_from(bytes.len()).map_err(|_| invalid("bundle byte count exceeds u64"))?;
        let next = self
            .count
            .checked_add(length)
            .ok_or_else(|| invalid("bundle byte count overflowed"))?;
        if self.limit.is_some_and(|limit| next > limit) {
            return Err(invalid("bundle exceeds complete byte limit"));
        }
        self.inner
            .write_all(bytes)
            .map_err(|error| EvidenceError::io("write staged evidence bundle", error))?;
        self.hasher.update(bytes);
        self.count = next;
        Ok(())
    }
    fn finish(self) -> (Sha256Digest, u64) {
        (Sha256Digest::new(self.hasher.finalize().into()), self.count)
    }
}

fn write_count<W: Write>(
    writer: &mut HashingWriter<W>,
    count: usize,
    limit: Option<u64>,
) -> Result<(), EvidenceError> {
    let count = u64::try_from(count).map_err(|_| invalid("entry count exceeds u64"))?;
    if limit.is_some_and(|limit| count > limit) {
        return Err(invalid("entry count exceeds selected limit"));
    }
    write_u64(writer, count)
}

fn write_sized<W: Write>(
    writer: &mut HashingWriter<W>,
    bytes: &[u8],
    limit: Option<u64>,
) -> Result<(), EvidenceError> {
    let size = u64::try_from(bytes.len()).map_err(|_| invalid("entry size exceeds u64"))?;
    write_sized_header(writer, size, limit)?;
    writer.write(bytes)
}

fn write_sized_header<W: Write>(
    writer: &mut HashingWriter<W>,
    size: u64,
    limit: Option<u64>,
) -> Result<(), EvidenceError> {
    if limit.is_some_and(|limit| size > limit) {
        return Err(invalid("entry exceeds selected byte limit"));
    }
    write_u64(writer, size)
}

fn write_u64<W: Write>(writer: &mut HashingWriter<W>, value: u64) -> Result<(), EvidenceError> {
    writer.write(&value.to_be_bytes())
}
