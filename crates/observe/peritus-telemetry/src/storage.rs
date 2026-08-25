//! Durable, atomic export-checkpoint storage.

#[cfg(test)]
mod tests;

use std::{
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use peritus_types::Sha256Digest;

use crate::{BufferCounters, ExportStreamId, TelemetryError, TelemetryErrorKind, TelemetryPump};

const CHECKPOINT_PREFIX: &[u8] = b"PERITUS-C7-EXPORT-CHECKPOINT-V2\0";
const CHECKPOINT_BYTES: usize = CHECKPOINT_PREFIX.len() + 16 + 8 + 32 + (4 * 8) + 32;
const CHECKPOINT_BYTES_U64: u64 = 152;
static TEMPORARY_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Exact contiguous final-disposition boundary persisted for restart recovery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExportCheckpoint {
    stream_id: ExportStreamId,
    disposed_through_sequence: u64,
    prefix_digest: Sha256Digest,
    counters: BufferCounters,
}

impl ExportCheckpoint {
    /// Captures the latest exact final-disposition boundary from a pump.
    #[must_use]
    pub const fn from_pump(pump: &TelemetryPump) -> Self {
        Self {
            stream_id: pump.stream_id(),
            disposed_through_sequence: pump.disposed_through_sequence(),
            prefix_digest: pump.disposed_prefix(),
            counters: pump.disposed_counters(),
        }
    }

    /// Returns the export-stream identity.
    #[must_use]
    pub const fn stream_id(self) -> ExportStreamId {
        self.stream_id
    }
    /// Returns the highest stable sequence known to be exported or dropped.
    #[must_use]
    pub const fn disposed_through_sequence(self) -> u64 {
        self.disposed_through_sequence
    }
    /// Returns the projection-prefix digest through that sequence.
    #[must_use]
    pub const fn prefix_digest(self) -> Sha256Digest {
        self.prefix_digest
    }
    /// Returns counters captured exactly at the final-disposition boundary.
    #[must_use]
    pub const fn counters(self) -> BufferCounters {
        self.counters
    }

    fn validate(self) -> Result<Self, TelemetryError> {
        if self.counters.submitted() != self.disposed_through_sequence {
            return Err(checkpoint_error("checkpoint sequence and counters disagree"));
        }
        let disposed = self
            .counters
            .exported()
            .checked_add(self.counters.dropped())
            .ok_or_else(|| checkpoint_error("checkpoint disposition accounting overflows"))?;
        if disposed != self.disposed_through_sequence {
            return Err(checkpoint_error(
                "checkpoint exported and dropped totals do not cover its exact prefix",
            ));
        }
        if self.disposed_through_sequence == 0 && self.prefix_digest != Sha256Digest::new([0; 32]) {
            return Err(checkpoint_error("genesis checkpoint has a nonzero prefix digest"));
        }
        Ok(self)
    }

    fn encode(self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(CHECKPOINT_BYTES);
        bytes.extend_from_slice(CHECKPOINT_PREFIX);
        bytes.extend_from_slice(self.stream_id.as_bytes());
        bytes.extend_from_slice(&self.disposed_through_sequence.to_be_bytes());
        bytes.extend_from_slice(self.prefix_digest.as_bytes());
        for value in [
            self.counters.submitted(),
            self.counters.accepted(),
            self.counters.dropped(),
            self.counters.exported(),
        ] {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        let checksum = peritus_codec::sha256(&bytes);
        bytes.extend_from_slice(checksum.as_bytes());
        bytes
    }

    fn decode(bytes: &[u8]) -> Result<Self, TelemetryError> {
        if bytes.len() != CHECKPOINT_BYTES || !bytes.starts_with(CHECKPOINT_PREFIX) {
            return Err(checkpoint_error(
                "checkpoint length or V2 domain marker is invalid or unsupported",
            ));
        }
        let checksum_start = CHECKPOINT_BYTES - Sha256Digest::LENGTH;
        let stored_checksum = Sha256Digest::new(
            bytes[checksum_start..]
                .try_into()
                .map_err(|_| checkpoint_error("checkpoint checksum length is invalid"))?,
        );
        if peritus_codec::sha256(&bytes[..checksum_start]) != stored_checksum {
            return Err(checkpoint_error("checkpoint checksum does not match"));
        }
        let mut offset = CHECKPOINT_PREFIX.len();
        let stream_id = ExportStreamId::new(take::<16>(bytes, &mut offset)?)?;
        let disposed_through_sequence = u64::from_be_bytes(take::<8>(bytes, &mut offset)?);
        let prefix_digest = Sha256Digest::new(take::<32>(bytes, &mut offset)?);
        let submitted = u64::from_be_bytes(take::<8>(bytes, &mut offset)?);
        let accepted = u64::from_be_bytes(take::<8>(bytes, &mut offset)?);
        let dropped = u64::from_be_bytes(take::<8>(bytes, &mut offset)?);
        let exported = u64::from_be_bytes(take::<8>(bytes, &mut offset)?);
        Self {
            stream_id,
            disposed_through_sequence,
            prefix_digest,
            counters: BufferCounters::from_parts(submitted, accepted, dropped, exported)?,
        }
        .validate()
    }
}

/// Single-owner append-only checkpoint directory with bounded generation retention.
pub struct CheckpointStore {
    directory: PathBuf,
    stream_id: ExportStreamId,
    retain: NonZeroUsize,
}

impl CheckpointStore {
    /// Opens or creates a checkpoint directory.
    ///
    /// # Errors
    ///
    /// Returns a redaction-safe storage error when directory initialization fails.
    pub fn open(
        directory: impl AsRef<Path>,
        stream_id: ExportStreamId,
        retain: NonZeroUsize,
    ) -> Result<Self, TelemetryError> {
        fs::create_dir_all(directory.as_ref())
            .map_err(|_| storage_error("create checkpoint directory"))?;
        let store = Self { directory: directory.as_ref().to_path_buf(), stream_id, retain };
        store.cleanup_temporaries()?;
        Ok(store)
    }

    /// Atomically publishes one exact final-disposition boundary and prunes older generations.
    ///
    /// A temporary file is synchronized and renamed to a sequence-unique final name. Recovery
    /// ignores temporary files. The final name is never overwritten by normal single-owner use.
    ///
    /// # Errors
    ///
    /// Returns identity, serialization, I/O, or conflicting-generation failures.
    pub fn persist(&self, checkpoint: ExportCheckpoint) -> Result<(), TelemetryError> {
        self.persist_with_finalize(checkpoint, Self::finalize_published_generation)
    }

    fn persist_with_finalize<F>(
        &self,
        checkpoint: ExportCheckpoint,
        finalize: F,
    ) -> Result<(), TelemetryError>
    where
        F: FnOnce(&Self) -> Result<(), TelemetryError>,
    {
        let checkpoint = checkpoint.validate()?;
        if checkpoint.stream_id != self.stream_id {
            return Err(checkpoint_error("checkpoint belongs to another export stream"));
        }
        let bytes = checkpoint.encode();
        let final_path = self.final_path(checkpoint.disposed_through_sequence);
        if final_path.exists() {
            let existing = read_bounded(&final_path)?;
            if existing == bytes {
                return finalize(self);
            }
            return Err(checkpoint_error("checkpoint generation already contains different bytes"));
        }
        let temporary = self.temporary_path(checkpoint.disposed_through_sequence);
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|_| storage_error("create checkpoint temporary"))?;
        file.write_all(&bytes).map_err(|_| storage_error("write checkpoint temporary"))?;
        file.sync_all().map_err(|_| storage_error("synchronize checkpoint temporary"))?;
        drop(file);
        fs::rename(&temporary, &final_path)
            .map_err(|_| storage_error("publish checkpoint generation"))?;
        finalize(self)
    }

    /// Loads the highest published checkpoint, or genesis when none exists.
    ///
    /// # Errors
    ///
    /// Returns a storage or terminal checkpoint-integrity failure.
    pub fn load_latest(&self) -> Result<Option<ExportCheckpoint>, TelemetryError> {
        let Some((_, path)) = self.generations()?.into_iter().max_by_key(|(sequence, _)| *sequence)
        else {
            return Ok(None);
        };
        let checkpoint = ExportCheckpoint::decode(&read_bounded(&path)?)?;
        if checkpoint.stream_id != self.stream_id {
            return Err(checkpoint_error("stored checkpoint belongs to another stream"));
        }
        Ok(Some(checkpoint))
    }

    fn prune(&self) -> Result<(), TelemetryError> {
        let mut generations = self.generations()?;
        generations.sort_unstable_by_key(|(sequence, _)| *sequence);
        let remove = generations.len().saturating_sub(self.retain.get());
        for (_, path) in generations.into_iter().take(remove) {
            fs::remove_file(path).map_err(|_| storage_error("prune checkpoint generation"))?;
        }
        if remove > 0 {
            sync_directory(&self.directory)?;
        }
        Ok(())
    }

    fn finalize_published_generation(&self) -> Result<(), TelemetryError> {
        sync_directory(&self.directory)?;
        self.prune()
    }

    fn generations(&self) -> Result<Vec<(u64, PathBuf)>, TelemetryError> {
        let prefix = stream_hex(self.stream_id);
        let mut generations = Vec::new();
        for entry in fs::read_dir(&self.directory)
            .map_err(|_| storage_error("enumerate checkpoint generations"))?
        {
            let entry = entry.map_err(|_| storage_error("read checkpoint directory entry"))?;
            if !entry.file_type().map_err(|_| storage_error("inspect checkpoint entry"))?.is_file()
            {
                continue;
            }
            let Some(sequence) = parse_generation(&entry.file_name(), &prefix) else { continue };
            generations.push((sequence, entry.path()));
        }
        Ok(generations)
    }

    fn cleanup_temporaries(&self) -> Result<(), TelemetryError> {
        let prefix = format!(".{}-", stream_hex(self.stream_id));
        let mut removed = false;
        for entry in fs::read_dir(&self.directory)
            .map_err(|_| storage_error("enumerate checkpoint temporaries"))?
        {
            let entry = entry.map_err(|_| storage_error("read checkpoint temporary entry"))?;
            if !entry
                .file_type()
                .map_err(|_| storage_error("inspect checkpoint temporary"))?
                .is_file()
            {
                continue;
            }
            let matches = entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with(&prefix) && name.ends_with(".temporary"));
            if matches {
                fs::remove_file(entry.path())
                    .map_err(|_| storage_error("remove abandoned checkpoint temporary"))?;
                removed = true;
            }
        }
        if removed {
            sync_directory(&self.directory)?;
        }
        Ok(())
    }

    fn final_path(&self, sequence: u64) -> PathBuf {
        self.directory.join(format!("{}-{sequence:020}.checkpoint", stream_hex(self.stream_id)))
    }

    fn temporary_path(&self, sequence: u64) -> PathBuf {
        let counter = TEMPORARY_COUNTER.fetch_add(1, Ordering::Relaxed);
        self.directory.join(format!(
            ".{}-{sequence:020}-{}-{counter}.temporary",
            stream_hex(self.stream_id),
            std::process::id(),
        ))
    }
}

fn take<const N: usize>(bytes: &[u8], offset: &mut usize) -> Result<[u8; N], TelemetryError> {
    let end =
        offset.checked_add(N).ok_or_else(|| checkpoint_error("checkpoint offset overflow"))?;
    let value = bytes
        .get(*offset..end)
        .ok_or_else(|| checkpoint_error("checkpoint is truncated"))?
        .try_into()
        .map_err(|_| checkpoint_error("checkpoint field length is invalid"))?;
    *offset = end;
    Ok(value)
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, TelemetryError> {
    let mut file = File::open(path).map_err(|_| storage_error("open checkpoint generation"))?;
    let length = file.metadata().map_err(|_| storage_error("inspect checkpoint generation"))?.len();
    if length != CHECKPOINT_BYTES_U64 {
        return Err(checkpoint_error("checkpoint file length is invalid"));
    }
    let mut bytes = Vec::with_capacity(CHECKPOINT_BYTES);
    file.read_to_end(&mut bytes).map_err(|_| storage_error("read checkpoint generation"))?;
    Ok(bytes)
}

fn parse_generation(name: &OsStr, prefix: &str) -> Option<u64> {
    let name = name.to_str()?;
    let sequence = name.strip_prefix(prefix)?.strip_prefix('-')?.strip_suffix(".checkpoint")?;
    if sequence.len() != 20 || !sequence.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    sequence.parse().ok()
}

fn stream_hex(stream: ExportStreamId) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(32);
    for byte in stream.as_bytes() {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), TelemetryError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| storage_error("synchronize checkpoint directory"))
}

#[cfg(windows)]
#[allow(
    clippy::unnecessary_wraps,
    reason = "the cross-platform checkpoint contract reports Unix directory-sync failures"
)]
const fn sync_directory(_path: &Path) -> Result<(), TelemetryError> {
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn sync_directory(path: &Path) -> Result<(), TelemetryError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| storage_error("synchronize checkpoint directory"))
}

const fn checkpoint_error(detail: &'static str) -> TelemetryError {
    TelemetryError::new(TelemetryErrorKind::InvalidCheckpoint, "validate export checkpoint", detail)
}

const fn storage_error(operation: &'static str) -> TelemetryError {
    TelemetryError::sourced(
        TelemetryErrorKind::Storage,
        operation,
        "checkpoint filesystem operation failed",
        "io",
    )
}
