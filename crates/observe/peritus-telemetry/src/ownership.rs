//! Durable single-owner custody for one exact pending export batch.

use std::{
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use peritus_types::Sha256Digest;

use crate::{ExportBatch, ExportCheckpoint, ExportStreamId, TelemetryError, TelemetryErrorKind};

const ACK_PREFIX: &[u8] = b"PERITUS-C7-PENDING-EXPORT-ACK-V1\0";
const ACK_BYTES: usize = ACK_PREFIX.len() + 16 + 32 + 8 + 8 + 8 + 32;
static TEMPORARY_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct AcknowledgedIdentity {
    stream_id: ExportStreamId,
    batch_id: Sha256Digest,
    first_sequence: u64,
    last_sequence: u64,
    count: u64,
}

impl AcknowledgedIdentity {
    fn from_batch(batch: &ExportBatch) -> Result<Self, TelemetryError> {
        let count = u64::try_from(batch.len())
            .map_err(|_| ownership_error("pending acknowledgement count is not representable"))?;
        Ok(Self {
            stream_id: batch.stream_id(),
            batch_id: batch.batch_id(),
            first_sequence: batch.first_sequence(),
            last_sequence: batch.last_sequence(),
            count,
        })
    }

    fn matches(self, batch: &ExportBatch) -> Result<bool, TelemetryError> {
        Ok(self == Self::from_batch(batch)?)
    }
}

pub(crate) enum RecoveredOwnership {
    None,
    Pending(ExportBatch),
    Acknowledged(ExportBatch),
    OrphanAcknowledgement(AcknowledgedIdentity),
}

impl RecoveredOwnership {
    pub(crate) const fn last_sequence(&self) -> Option<u64> {
        match self {
            Self::None => None,
            Self::Pending(batch) | Self::Acknowledged(batch) => Some(batch.last_sequence()),
            Self::OrphanAcknowledgement(identity) => Some(identity.last_sequence),
        }
    }
}

/// Caller-owned durable storage for one exact pending export batch and acknowledgement marker.
pub struct PendingBatchStore {
    directory: PathBuf,
    stream_id: ExportStreamId,
}

impl PendingBatchStore {
    /// Opens or creates a stream-scoped pending-export directory.
    ///
    /// # Errors
    ///
    /// Returns an explicit storage or integrity failure for an unusable caller-owned directory.
    pub fn open(
        directory: impl AsRef<Path>,
        stream_id: ExportStreamId,
    ) -> Result<Self, TelemetryError> {
        fs::create_dir_all(directory.as_ref())
            .map_err(|_| storage_error("create pending export directory"))?;
        let metadata = fs::symlink_metadata(directory.as_ref())
            .map_err(|_| storage_error("inspect pending export directory"))?;
        if !metadata.file_type().is_dir() {
            return Err(ownership_error("pending export location is not a directory"));
        }
        protect(directory.as_ref())?;
        let store = Self { directory: directory.as_ref().to_path_buf(), stream_id };
        store.cleanup_temporaries()?;
        Ok(store)
    }

    /// Returns the logical export stream that exclusively owns this store.
    #[must_use]
    pub const fn stream_id(&self) -> ExportStreamId {
        self.stream_id
    }

    pub(crate) fn require_empty(&self) -> Result<(), TelemetryError> {
        if self.pending_path().exists() || self.acknowledged_path().exists() {
            return Err(ownership_error(
                "pending export state requires checkpoint-aware recovery",
            ));
        }
        Ok(())
    }

    pub(crate) fn load(&self) -> Result<RecoveredOwnership, TelemetryError> {
        let pending = if self.pending_path().exists() {
            let batch = ExportBatch::from_ownership_bytes(&read_file(&self.pending_path())?)?;
            if batch.stream_id() != self.stream_id {
                return Err(ownership_error("pending batch belongs to another export stream"));
            }
            Some(batch)
        } else {
            None
        };
        let acknowledged = if self.acknowledged_path().exists() {
            let identity = decode_acknowledgement(&read_file(&self.acknowledged_path())?)?;
            if identity.stream_id != self.stream_id {
                return Err(ownership_error(
                    "pending acknowledgement belongs to another export stream",
                ));
            }
            Some(identity)
        } else {
            None
        };
        match (pending, acknowledged) {
            (None, None) => Ok(RecoveredOwnership::None),
            (Some(batch), None) => Ok(RecoveredOwnership::Pending(batch)),
            (Some(batch), Some(identity)) => {
                if !identity.matches(&batch)? {
                    return Err(ownership_error(
                        "pending acknowledgement does not match the exact pending batch",
                    ));
                }
                Ok(RecoveredOwnership::Acknowledged(batch))
            }
            (None, Some(identity)) => {
                Ok(RecoveredOwnership::OrphanAcknowledgement(identity))
            }
        }
    }

    pub(crate) fn persist(&self, batch: &ExportBatch) -> Result<(), TelemetryError> {
        if batch.stream_id() != self.stream_id || batch.is_empty() {
            return Err(ownership_error(
                "pending batch stream or item count is invalid for this store",
            ));
        }
        if self.acknowledged_path().exists() {
            return Err(ownership_error(
                "an acknowledged pending batch must be checkpointed before replacement",
            ));
        }
        self.publish_exact(&self.pending_path(), "pending", &batch.ownership_bytes()?)
    }

    pub(crate) fn mark_acknowledged(&self, batch: &ExportBatch) -> Result<(), TelemetryError> {
        self.verify_pending(batch)?;
        let identity = AcknowledgedIdentity::from_batch(batch)?;
        self.publish_exact(
            &self.acknowledged_path(),
            "acknowledged",
            &encode_acknowledgement(identity),
        )
    }

    pub(crate) fn discard_unaccepted(&self, batch: &ExportBatch) -> Result<(), TelemetryError> {
        if self.acknowledged_path().exists() {
            return Err(ownership_error(
                "acknowledged pending export cannot be discarded as unaccepted",
            ));
        }
        if self.pending_path().exists() {
            self.verify_pending(batch)?;
            fs::remove_file(self.pending_path())
                .map_err(|_| storage_error("remove cancelled pending export"))?;
            sync_directory(&self.directory)?;
        }
        Ok(())
    }

    pub(crate) fn retire_acknowledged(
        &self,
        batch: &ExportBatch,
    ) -> Result<(), TelemetryError> {
        if self.pending_path().exists() {
            self.verify_pending(batch)?;
        }
        if !self.acknowledged_path().exists() {
            return Err(ownership_error(
                "pending export retirement has no durable acknowledgement",
            ));
        }
        let identity = decode_acknowledgement(&read_file(&self.acknowledged_path())?)?;
        if !identity.matches(batch)? {
            return Err(ownership_error(
                "retired acknowledgement does not match the exact pending batch",
            ));
        }
        remove_if_present(&self.pending_path(), "remove checkpointed pending export")?;
        sync_directory(&self.directory)?;
        remove_if_present(
            &self.acknowledged_path(),
            "remove checkpointed pending acknowledgement",
        )?;
        sync_directory(&self.directory)
    }

    pub(crate) fn clear_checkpointed(
        &self,
        checkpoint: ExportCheckpoint,
        ownership: &RecoveredOwnership,
    ) -> Result<(), TelemetryError> {
        let Some(last_sequence) = ownership.last_sequence() else { return Ok(()) };
        if checkpoint.stream_id() != self.stream_id
            || checkpoint.disposed_through_sequence() < last_sequence
        {
            return Err(ownership_error(
                "checkpoint does not cover stale pending export ownership",
            ));
        }
        remove_if_present(&self.pending_path(), "remove stale pending export")?;
        remove_if_present(
            &self.acknowledged_path(),
            "remove stale pending acknowledgement",
        )?;
        sync_directory(&self.directory)
    }

    fn verify_pending(&self, batch: &ExportBatch) -> Result<(), TelemetryError> {
        let stored = ExportBatch::from_ownership_bytes(&read_file(&self.pending_path())?)?;
        if &stored != batch {
            return Err(ownership_error(
                "durable pending batch differs from the in-memory exact batch",
            ));
        }
        Ok(())
    }

    fn publish_exact(
        &self,
        final_path: &Path,
        label: &str,
        bytes: &[u8],
    ) -> Result<(), TelemetryError> {
        if final_path.exists() {
            if read_file(final_path)? == bytes {
                sync_directory(&self.directory)?;
                return Ok(());
            }
            return Err(ownership_error(
                "pending export generation already contains different bytes",
            ));
        }
        let temporary = self.temporary_path(label);
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|_| storage_error("create pending export temporary"))?;
        file.write_all(bytes)
            .map_err(|_| storage_error("write pending export temporary"))?;
        file.sync_all()
            .map_err(|_| storage_error("synchronize pending export temporary"))?;
        drop(file);
        fs::rename(&temporary, final_path)
            .map_err(|_| storage_error("publish pending export state"))?;
        sync_directory(&self.directory)
    }

    fn cleanup_temporaries(&self) -> Result<(), TelemetryError> {
        let prefix = format!(".{}-", stream_hex(self.stream_id));
        let mut removed = false;
        for entry in fs::read_dir(&self.directory)
            .map_err(|_| storage_error("enumerate pending export temporaries"))?
        {
            let entry = entry.map_err(|_| storage_error("read pending export temporary"))?;
            let matches = entry
                .file_type()
                .map_err(|_| storage_error("inspect pending export temporary"))?
                .is_file()
                && entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.starts_with(&prefix) && name.ends_with(".temporary"));
            if matches {
                fs::remove_file(entry.path())
                    .map_err(|_| storage_error("remove pending export temporary"))?;
                removed = true;
            }
        }
        if removed {
            sync_directory(&self.directory)?;
        }
        Ok(())
    }

    fn pending_path(&self) -> PathBuf {
        self.directory.join(format!("{}.pending", stream_hex(self.stream_id)))
    }

    fn acknowledged_path(&self) -> PathBuf {
        self.directory.join(format!("{}.acknowledged", stream_hex(self.stream_id)))
    }

    fn temporary_path(&self, label: &str) -> PathBuf {
        let counter = TEMPORARY_COUNTER.fetch_add(1, Ordering::Relaxed);
        self.directory.join(format!(
            ".{}-{label}-{}-{counter}.temporary",
            stream_hex(self.stream_id),
            std::process::id(),
        ))
    }
}

fn encode_acknowledgement(identity: AcknowledgedIdentity) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(ACK_BYTES);
    bytes.extend_from_slice(ACK_PREFIX);
    bytes.extend_from_slice(identity.stream_id.as_bytes());
    bytes.extend_from_slice(identity.batch_id.as_bytes());
    bytes.extend_from_slice(&identity.first_sequence.to_be_bytes());
    bytes.extend_from_slice(&identity.last_sequence.to_be_bytes());
    bytes.extend_from_slice(&identity.count.to_be_bytes());
    let checksum = peritus_codec::sha256(&bytes);
    bytes.extend_from_slice(checksum.as_bytes());
    bytes
}

fn decode_acknowledgement(bytes: &[u8]) -> Result<AcknowledgedIdentity, TelemetryError> {
    if bytes.len() != ACK_BYTES || !bytes.starts_with(ACK_PREFIX) {
        return Err(ownership_error(
            "pending acknowledgement marker or length is invalid",
        ));
    }
    let checksum_start = ACK_BYTES - Sha256Digest::LENGTH;
    let checksum = Sha256Digest::new(
        bytes[checksum_start..]
            .try_into()
            .map_err(|_| ownership_error("pending acknowledgement checksum is invalid"))?,
    );
    if peritus_codec::sha256(&bytes[..checksum_start]) != checksum {
        return Err(ownership_error(
            "pending acknowledgement checksum does not match",
        ));
    }
    let mut offset = ACK_PREFIX.len();
    let stream_id = ExportStreamId::new(take::<16>(bytes, &mut offset)?)?;
    let batch_id = Sha256Digest::new(take::<32>(bytes, &mut offset)?);
    let first_sequence = u64::from_be_bytes(take::<8>(bytes, &mut offset)?);
    let last_sequence = u64::from_be_bytes(take::<8>(bytes, &mut offset)?);
    let count = u64::from_be_bytes(take::<8>(bytes, &mut offset)?);
    if count == 0 || first_sequence == 0 || last_sequence < first_sequence {
        return Err(ownership_error(
            "pending acknowledgement range or count is invalid",
        ));
    }
    Ok(AcknowledgedIdentity {
        stream_id,
        batch_id,
        first_sequence,
        last_sequence,
        count,
    })
}

fn take<const N: usize>(bytes: &[u8], offset: &mut usize) -> Result<[u8; N], TelemetryError> {
    let end = offset
        .checked_add(N)
        .ok_or_else(|| ownership_error("pending acknowledgement offset overflows"))?;
    let value = bytes
        .get(*offset..end)
        .ok_or_else(|| ownership_error("pending acknowledgement is truncated"))?
        .try_into()
        .map_err(|_| ownership_error("pending acknowledgement field length is invalid"))?;
    *offset = end;
    Ok(value)
}

fn read_file(path: &Path) -> Result<Vec<u8>, TelemetryError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| storage_error("inspect pending export state"))?;
    if !metadata.file_type().is_file() {
        return Err(ownership_error("pending export state is not a regular file"));
    }
    let length = usize::try_from(metadata.len())
        .map_err(|_| ownership_error("pending export state length is not representable"))?;
    let mut bytes = Vec::with_capacity(length);
    File::open(path)
        .and_then(|mut file| file.read_to_end(&mut bytes))
        .map_err(|_| storage_error("read pending export state"))?;
    if bytes.len() != length {
        return Err(ownership_error("pending export state changed while read"));
    }
    Ok(bytes)
}

fn remove_if_present(path: &Path, operation: &'static str) -> Result<(), TelemetryError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(_) => Err(storage_error(operation)),
    }
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
fn protect(path: &Path) -> Result<(), TelemetryError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|_| storage_error("protect pending export directory"))
}

#[cfg(windows)]
const fn protect(_path: &Path) -> Result<(), TelemetryError> {
    Ok(())
}

#[cfg(not(any(unix, windows)))]
const fn protect(_path: &Path) -> Result<(), TelemetryError> {
    Ok(())
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), TelemetryError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| storage_error("synchronize pending export directory"))
}

#[cfg(windows)]
const fn sync_directory(_path: &Path) -> Result<(), TelemetryError> {
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn sync_directory(path: &Path) -> Result<(), TelemetryError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| storage_error("synchronize pending export directory"))
}

const fn ownership_error(detail: &'static str) -> TelemetryError {
    TelemetryError::new(
        TelemetryErrorKind::InvalidCheckpoint,
        "validate pending export ownership",
        detail,
    )
}

const fn storage_error(operation: &'static str) -> TelemetryError {
    TelemetryError::sourced(
        TelemetryErrorKind::Storage,
        operation,
        "pending export filesystem operation failed",
        "io",
    )
}
