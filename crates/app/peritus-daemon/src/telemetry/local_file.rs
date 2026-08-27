//! Synchronized sequence-named local telemetry batch exporter.

use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use peritus_telemetry::{ExportAck, ExportBatch, Exporter, ExporterError, ExporterErrorCode};

static TEMPORARY_NONCE: AtomicU64 = AtomicU64::new(1);
const MAX_SPOOL_ENTRIES: usize = 65_536;

pub(super) struct LocalFileExporter {
    directory: PathBuf,
    quota_bytes: u64,
}

impl LocalFileExporter {
    pub(super) fn open(directory: &Path, quota_bytes: u64) -> Result<Self, ExporterError> {
        fs::create_dir_all(directory).map_err(|_| unavailable())?;
        let metadata = fs::symlink_metadata(directory).map_err(|_| unavailable())?;
        if !metadata.file_type().is_dir() {
            return Err(rejected());
        }
        protect(directory)?;
        let exporter = Self { directory: directory.to_path_buf(), quota_bytes };
        exporter.quarantine_temporaries()?;
        Ok(exporter)
    }

    fn quarantine_temporaries(&self) -> Result<(), ExporterError> {
        for entry in bounded_entries(&self.directory)? {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(".batch-") && name.ends_with(".tmp") {
                let quarantine = self.directory.join(format!("{name}.incomplete"));
                fs::rename(entry.path(), quarantine).map_err(|_| unavailable())?;
            }
        }
        sync_directory(&self.directory)
    }

    fn ensure_quota(&self, incoming: u64, final_path: &Path) -> Result<(), ExporterError> {
        if incoming > self.quota_bytes {
            return Err(rejected());
        }
        let mut retained = Vec::new();
        let mut used = 0_u64;
        for entry in bounded_entries(&self.directory)? {
            let metadata = entry.metadata().map_err(|_| unavailable())?;
            if !metadata.file_type().is_file() || entry.path() == final_path {
                continue;
            }
            let name = entry.file_name();
            if !name.to_string_lossy().starts_with("batch-") {
                continue;
            }
            used = used.checked_add(metadata.len()).ok_or_else(rejected)?;
            retained.push((name, entry.path(), metadata.len()));
        }
        retained.sort_unstable_by(|left, right| left.0.cmp(&right.0));
        for (_, path, size) in retained {
            if used.checked_add(incoming).is_some_and(|total| total <= self.quota_bytes) {
                break;
            }
            fs::remove_file(path).map_err(|_| unavailable())?;
            used = used.saturating_sub(size);
        }
        if used.checked_add(incoming).is_none_or(|total| total > self.quota_bytes) {
            return Err(rejected());
        }
        sync_directory(&self.directory)
    }
}

impl Exporter for LocalFileExporter {
    fn export(&mut self, batch: &ExportBatch) -> Result<ExportAck, ExporterError> {
        let bytes = batch.canonical_bytes().map_err(|_| protocol())?;
        let name = format!(
            "batch-{:020}-{:020}-{}.bin",
            batch.first_sequence(),
            batch.last_sequence(),
            digest_hex(batch.batch_id().as_bytes()),
        );
        let final_path = self.directory.join(name);
        if final_path.exists() {
            if read_bounded(&final_path, bytes.len())? == bytes {
                return Ok(ExportAck::accept(batch));
            }
            return Err(protocol());
        }
        self.ensure_quota(u64::try_from(bytes.len()).map_err(|_| rejected())?, &final_path)?;
        let temporary = self.directory.join(format!(
            ".batch-{}-{}.tmp",
            std::process::id(),
            TEMPORARY_NONCE.fetch_add(1, Ordering::Relaxed),
        ));
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|_| unavailable())?;
        file.write_all(&bytes).map_err(|_| unavailable())?;
        file.sync_all().map_err(|_| unavailable())?;
        drop(file);
        fs::rename(&temporary, &final_path).map_err(|_| unavailable())?;
        sync_directory(&self.directory)?;
        Ok(ExportAck::accept(batch))
    }

    fn shutdown(&mut self) -> Result<(), ExporterError> {
        sync_directory(&self.directory)
    }
}

fn bounded_entries(directory: &Path) -> Result<Vec<fs::DirEntry>, ExporterError> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(directory).map_err(|_| unavailable())? {
        if entries.len() == MAX_SPOOL_ENTRIES {
            return Err(rejected());
        }
        entries.push(entry.map_err(|_| unavailable())?);
    }
    Ok(entries)
}

fn read_bounded(path: &Path, expected: usize) -> Result<Vec<u8>, ExporterError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| unavailable())?;
    if !metadata.file_type().is_file() || usize::try_from(metadata.len()).ok() != Some(expected) {
        return Err(protocol());
    }
    let mut bytes = Vec::with_capacity(expected);
    File::open(path)
        .and_then(|mut file| file.read_to_end(&mut bytes))
        .map_err(|_| unavailable())?;
    Ok(bytes)
}

fn digest_hex(bytes: &[u8; 32]) -> String {
    let mut text = String::with_capacity(64);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut text, "{byte:02x}").expect("writing to String cannot fail");
    }
    text
}

#[cfg(unix)]
fn protect(path: &Path) -> Result<(), ExporterError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|_| unavailable())
}

#[cfg(windows)]
const fn protect(_path: &Path) -> Result<(), ExporterError> {
    Ok(())
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), ExporterError> {
    File::open(path).and_then(|file| file.sync_all()).map_err(|_| unavailable())
}

#[cfg(windows)]
const fn sync_directory(_path: &Path) -> Result<(), ExporterError> {
    Ok(())
}

const fn unavailable() -> ExporterError {
    ExporterError::new(ExporterErrorCode::Unavailable, true)
}

const fn rejected() -> ExporterError {
    ExporterError::new(ExporterErrorCode::Rejected, false)
}

const fn protocol() -> ExporterError {
    ExporterError::new(ExporterErrorCode::Protocol, false)
}
