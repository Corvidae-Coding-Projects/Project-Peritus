//! Caller-owned durable overflow storage for lossless telemetry custody.

use std::{
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use peritus_types::Sha256Digest;

use crate::{
    ExportStreamId, TelemetryError, TelemetryErrorKind, buffer::BufferedRecord,
};

const SPILL_PREFIX: &[u8] = b"PERITUS-C7-TELEMETRY-SPILL-V1\0";
const FIXED_BYTES: usize = SPILL_PREFIX.len() + 16 + 8 + 32 + 8 + 8 + 32;
static TEMPORARY_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Single-owner durable overflow directory scoped to one caller-selected export stream.
pub struct SpillStore {
    directory: PathBuf,
    stream_id: ExportStreamId,
    first_sequence: Option<u64>,
    last_sequence: Option<u64>,
    removed_through: Option<u64>,
}

impl SpillStore {
    /// Opens or creates a caller-owned spill directory and recovers its published sequence range.
    ///
    /// Temporary files are never accepted as records and are removed during open. Published file
    /// names recover their outer sequence range; projection replay idempotently fills a range left
    /// sparse by an interrupted prior recovery. Record bytes and checksums are validated when read.
    ///
    /// # Errors
    ///
    /// Returns an explicit storage or integrity error for an unusable directory or sequence range.
    pub fn open(
        directory: impl AsRef<Path>,
        stream_id: ExportStreamId,
    ) -> Result<Self, TelemetryError> {
        fs::create_dir_all(directory.as_ref())
            .map_err(|_| spill_storage_error("create telemetry spill directory"))?;
        let metadata = fs::symlink_metadata(directory.as_ref())
            .map_err(|_| spill_storage_error("inspect telemetry spill directory"))?;
        if !metadata.file_type().is_dir() {
            return Err(spill_integrity_error("spill location is not a directory"));
        }
        protect(directory.as_ref())?;
        let mut store = Self {
            directory: directory.as_ref().to_path_buf(),
            stream_id,
            first_sequence: None,
            last_sequence: None,
            removed_through: None,
        };
        store.cleanup_temporaries()?;
        store.recover_range()?;
        Ok(store)
    }

    /// Returns the stream that exclusively owns this spill store.
    #[must_use]
    pub const fn stream_id(&self) -> ExportStreamId {
        self.stream_id
    }

    /// Returns the exact number of durably pending spill records.
    #[must_use]
    pub const fn pending_records(&self) -> u64 {
        match (self.first_sequence, self.last_sequence) {
            (Some(first), Some(last)) => last - first + 1,
            _ => 0,
        }
    }

    /// Returns whether this store has no published spill records.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.first_sequence.is_none()
    }

    pub(crate) fn persist(&mut self, record: &BufferedRecord) -> Result<(), TelemetryError> {
        let bytes = encode_record(self.stream_id, record)?;
        let final_path = self.final_path(record.sequence);
        if final_path.exists() {
            if read_file(&final_path)? != bytes {
                return Err(spill_integrity_error(
                    "published spill sequence contains different canonical bytes",
                ));
            }
            self.include_sequence(record.sequence);
            return Ok(());
        }

        let temporary = self.temporary_path(record.sequence);
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|_| spill_storage_error("create telemetry spill temporary"))?;
        file.write_all(&bytes)
            .map_err(|_| spill_storage_error("write telemetry spill temporary"))?;
        file.sync_all()
            .map_err(|_| spill_storage_error("synchronize telemetry spill temporary"))?;
        drop(file);
        fs::rename(&temporary, &final_path)
            .map_err(|_| spill_storage_error("publish telemetry spill record"))?;
        sync_directory(&self.directory)?;
        self.include_sequence(record.sequence);
        Ok(())
    }

    pub(crate) fn load_prefix(
        &self,
        target_bytes: usize,
    ) -> Result<Vec<BufferedRecord>, TelemetryError> {
        let Some(mut sequence) = self.first_sequence else { return Ok(Vec::new()) };
        let last = self
            .last_sequence
            .ok_or_else(|| spill_integrity_error("spill range has no last sequence"))?;
        let mut records = Vec::new();
        let mut payload_bytes = 0_usize;
        loop {
            let record = decode_record(self.stream_id, sequence, &read_file(&self.final_path(sequence))?)?;
            let next_bytes = payload_bytes.checked_add(record.canonical.len()).ok_or_else(|| {
                TelemetryError::new(
                    TelemetryErrorKind::SequenceOverflow,
                    "load telemetry spill batch",
                    "spill batch payload byte accounting overflow",
                )
            })?;
            if !records.is_empty() && next_bytes > target_bytes {
                break;
            }
            payload_bytes = next_bytes;
            records.push(record);
            if sequence == last {
                break;
            }
            sequence = sequence.checked_add(1).ok_or_else(|| {
                spill_integrity_error("spill sequence range overflows before its last record")
            })?;
        }
        Ok(records)
    }

    pub(crate) fn load_count(
        &self,
        count: usize,
    ) -> Result<Vec<BufferedRecord>, TelemetryError> {
        if count == 0 {
            return Ok(Vec::new());
        }
        let Some(mut sequence) = self.first_sequence else {
            return Err(spill_integrity_error(
                "exact pending batch exceeds durable spill custody",
            ));
        };
        let last = self
            .last_sequence
            .ok_or_else(|| spill_integrity_error("spill range has no last sequence"))?;
        let mut records = Vec::new();
        for _ in 0..count {
            if sequence > last {
                return Err(spill_integrity_error(
                    "exact pending batch exceeds durable spill range",
                ));
            }
            records.push(decode_record(
                self.stream_id,
                sequence,
                &read_file(&self.final_path(sequence))?,
            )?);
            sequence = sequence.checked_add(1).ok_or_else(|| {
                spill_integrity_error("exact pending batch sequence overflows")
            })?;
        }
        Ok(records)
    }

    pub(crate) fn remove_prefix(
        &mut self,
        records: &[BufferedRecord],
    ) -> Result<(), TelemetryError> {
        if records.is_empty() {
            return Ok(());
        }
        let mut previous = None;
        for record in records {
            if previous.is_some_and(|sequence: u64| sequence.checked_add(1) != Some(record.sequence))
            {
                return Err(spill_integrity_error("acknowledged spill prefix is not contiguous"));
            }
            previous = Some(record.sequence);
            let Some(first) = self.first_sequence else {
                if self.removed_through.is_some_and(|removed| record.sequence <= removed) {
                    continue;
                }
                return Err(spill_integrity_error("acknowledged spill prefix is not published"));
            };
            if record.sequence < first {
                if self.removed_through.is_some_and(|removed| record.sequence <= removed) {
                    continue;
                }
                return Err(spill_integrity_error(
                    "acknowledged spill record precedes the durable frontier",
                ));
            }
            if record.sequence != first {
                return Err(spill_integrity_error(
                    "acknowledged spill prefix does not start at the durable frontier",
                ));
            }
            let path = self.final_path(record.sequence);
            let stored = decode_record(self.stream_id, record.sequence, &read_file(&path)?)?;
            if stored.sequence != record.sequence
                || stored.prefix_digest != record.prefix_digest
                || stored.accepted_total != record.accepted_total
                || stored.canonical != record.canonical
            {
                return Err(spill_integrity_error(
                    "acknowledged spill prefix differs from durable canonical bytes",
                ));
            }
            fs::remove_file(path)
                .map_err(|_| spill_storage_error("remove acknowledged telemetry spill record"))?;
            self.removed_through = Some(record.sequence);
            self.first_sequence = match self.last_sequence {
                Some(last) if record.sequence < last => record.sequence.checked_add(1),
                Some(_) => None,
                None => return Err(spill_integrity_error("spill range lost its last sequence")),
            };
            if self.first_sequence.is_none() {
                self.last_sequence = None;
            }
        }
        sync_directory(&self.directory)?;
        self.removed_through = None;
        Ok(())
    }

    fn include_sequence(&mut self, sequence: u64) {
        self.first_sequence = Some(self.first_sequence.map_or(sequence, |first| first.min(sequence)));
        self.last_sequence = Some(self.last_sequence.map_or(sequence, |last| last.max(sequence)));
    }

    fn recover_range(&mut self) -> Result<(), TelemetryError> {
        let prefix = stream_hex(self.stream_id);
        let mut sequences = Vec::new();
        for entry in fs::read_dir(&self.directory)
            .map_err(|_| spill_storage_error("enumerate telemetry spill records"))?
        {
            let entry = entry.map_err(|_| spill_storage_error("read telemetry spill entry"))?;
            if !entry
                .file_type()
                .map_err(|_| spill_storage_error("inspect telemetry spill entry"))?
                .is_file()
            {
                continue;
            }
            if let Some(sequence) = parse_record_name(&entry.file_name(), &prefix) {
                sequences.push(sequence);
            }
        }
        sequences.sort_unstable();
        self.first_sequence = sequences.first().copied();
        self.last_sequence = sequences.last().copied();
        Ok(())
    }

    fn cleanup_temporaries(&self) -> Result<(), TelemetryError> {
        let prefix = format!(".{}-", stream_hex(self.stream_id));
        let mut removed = false;
        for entry in fs::read_dir(&self.directory)
            .map_err(|_| spill_storage_error("enumerate telemetry spill temporaries"))?
        {
            let entry = entry.map_err(|_| spill_storage_error("read telemetry spill temporary"))?;
            let matches = entry
                .file_type()
                .map_err(|_| spill_storage_error("inspect telemetry spill temporary"))?
                .is_file()
                && entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.starts_with(&prefix) && name.ends_with(".temporary"));
            if matches {
                fs::remove_file(entry.path())
                    .map_err(|_| spill_storage_error("remove telemetry spill temporary"))?;
                removed = true;
            }
        }
        if removed {
            sync_directory(&self.directory)?;
        }
        Ok(())
    }

    fn final_path(&self, sequence: u64) -> PathBuf {
        self.directory.join(format!(
            "{}-{sequence:020}.spill",
            stream_hex(self.stream_id),
        ))
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

fn encode_record(
    stream_id: ExportStreamId,
    record: &BufferedRecord,
) -> Result<Vec<u8>, TelemetryError> {
    let payload_length = u64::try_from(record.canonical.len()).map_err(|_| {
        TelemetryError::new(
            TelemetryErrorKind::SequenceOverflow,
            "encode telemetry spill record",
            "canonical spill payload length is not representable",
        )
    })?;
    let capacity = FIXED_BYTES.checked_add(record.canonical.len()).ok_or_else(|| {
        TelemetryError::new(
            TelemetryErrorKind::SequenceOverflow,
            "encode telemetry spill record",
            "spill record allocation length overflows",
        )
    })?;
    let mut bytes = Vec::with_capacity(capacity);
    bytes.extend_from_slice(SPILL_PREFIX);
    bytes.extend_from_slice(stream_id.as_bytes());
    bytes.extend_from_slice(&record.sequence.to_be_bytes());
    bytes.extend_from_slice(record.prefix_digest.as_bytes());
    bytes.extend_from_slice(&record.accepted_total.to_be_bytes());
    bytes.extend_from_slice(&payload_length.to_be_bytes());
    bytes.extend_from_slice(&record.canonical);
    let checksum = peritus_codec::sha256(&bytes);
    bytes.extend_from_slice(checksum.as_bytes());
    Ok(bytes)
}

fn decode_record(
    expected_stream: ExportStreamId,
    expected_sequence: u64,
    bytes: &[u8],
) -> Result<BufferedRecord, TelemetryError> {
    if bytes.len() < FIXED_BYTES || !bytes.starts_with(SPILL_PREFIX) {
        return Err(spill_integrity_error("spill record marker or length is invalid"));
    }
    let checksum_start = bytes.len() - Sha256Digest::LENGTH;
    let stored_checksum = Sha256Digest::new(
        bytes[checksum_start..]
            .try_into()
            .map_err(|_| spill_integrity_error("spill checksum length is invalid"))?,
    );
    if peritus_codec::sha256(&bytes[..checksum_start]) != stored_checksum {
        return Err(spill_integrity_error("spill record checksum does not match"));
    }
    let mut offset = SPILL_PREFIX.len();
    let stream_id = ExportStreamId::new(take::<16>(bytes, &mut offset)?)?;
    let sequence = u64::from_be_bytes(take::<8>(bytes, &mut offset)?);
    let prefix_digest = Sha256Digest::new(take::<32>(bytes, &mut offset)?);
    let accepted_total = u64::from_be_bytes(take::<8>(bytes, &mut offset)?);
    let payload_length = usize::try_from(u64::from_be_bytes(take::<8>(bytes, &mut offset)?))
        .map_err(|_| spill_integrity_error("spill payload length is not representable"))?;
    let payload_end = offset
        .checked_add(payload_length)
        .ok_or_else(|| spill_integrity_error("spill payload offset overflows"))?;
    if payload_end != checksum_start || stream_id != expected_stream || sequence != expected_sequence
    {
        return Err(spill_integrity_error(
            "spill identity, sequence, or payload length does not match",
        ));
    }
    let canonical = bytes[offset..payload_end].to_vec().into_boxed_slice();
    Ok(BufferedRecord {
        sequence,
        canonical,
        prefix_digest,
        accepted_total,
        gap_before: None,
        resident_bytes: 0,
    })
}

fn take<const N: usize>(bytes: &[u8], offset: &mut usize) -> Result<[u8; N], TelemetryError> {
    let end = offset
        .checked_add(N)
        .ok_or_else(|| spill_integrity_error("spill field offset overflows"))?;
    let value = bytes
        .get(*offset..end)
        .ok_or_else(|| spill_integrity_error("spill record is truncated"))?
        .try_into()
        .map_err(|_| spill_integrity_error("spill field length is invalid"))?;
    *offset = end;
    Ok(value)
}

fn read_file(path: &Path) -> Result<Vec<u8>, TelemetryError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| spill_storage_error("inspect telemetry spill record"))?;
    if !metadata.file_type().is_file() {
        return Err(spill_integrity_error("spill record is not a regular file"));
    }
    let length = usize::try_from(metadata.len())
        .map_err(|_| spill_integrity_error("spill record length is not representable"))?;
    let mut bytes = Vec::with_capacity(length);
    File::open(path)
        .and_then(|mut file| file.read_to_end(&mut bytes))
        .map_err(|_| spill_storage_error("read telemetry spill record"))?;
    if bytes.len() != length {
        return Err(spill_integrity_error("spill record changed while it was read"));
    }
    Ok(bytes)
}

fn parse_record_name(name: &OsStr, prefix: &str) -> Option<u64> {
    let name = name.to_str()?;
    let sequence = name.strip_prefix(prefix)?.strip_prefix('-')?.strip_suffix(".spill")?;
    if sequence.len() != 20 || !sequence.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    sequence.parse().ok().filter(|sequence| *sequence != 0)
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
        .map_err(|_| spill_storage_error("protect telemetry spill directory"))
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
        .map_err(|_| spill_storage_error("synchronize telemetry spill directory"))
}

#[cfg(windows)]
const fn sync_directory(_path: &Path) -> Result<(), TelemetryError> {
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn sync_directory(path: &Path) -> Result<(), TelemetryError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| spill_storage_error("synchronize telemetry spill directory"))
}

const fn spill_integrity_error(detail: &'static str) -> TelemetryError {
    TelemetryError::new(TelemetryErrorKind::InvalidCheckpoint, "validate telemetry spill", detail)
}

const fn spill_storage_error(operation: &'static str) -> TelemetryError {
    TelemetryError::sourced(
        TelemetryErrorKind::Storage,
        operation,
        "telemetry spill filesystem operation failed",
        "io",
    )
}
