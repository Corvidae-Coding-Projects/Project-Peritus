//! Caller-polled sequence-named local telemetry batch exporter.

use std::{
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Read, Write},
    mem,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use peritus_telemetry::{
    ExportAck, ExportBatch, ExportPhase, ExportPoll, ExportPollControl, ExportProgress, Exporter,
    ExporterError, ExporterErrorCode, ExporterShutdownPoll,
};

static TEMPORARY_NONCE: AtomicU64 = AtomicU64::new(1);

pub(super) struct LocalFileExporter {
    directory: PathBuf,
    quota_bytes: u64,
    operation: Operation,
}

enum Operation {
    Idle,
    Export(PendingExport),
    Shutdown,
}

struct PendingExport {
    ack: ExportAck,
    bytes: Vec<u8>,
    final_path: PathBuf,
    temporary_path: Option<PathBuf>,
    file: Option<File>,
    written: usize,
    stage: ExportStage,
    owns_final: bool,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ExportStage {
    Prepare,
    Writing,
    SyncFile,
    Publish,
    SyncDirectory,
}

impl LocalFileExporter {
    pub(super) fn open(directory: &Path, quota_bytes: u64) -> Result<Self, ExporterError> {
        fs::create_dir_all(directory).map_err(|_| unavailable())?;
        let metadata = fs::symlink_metadata(directory).map_err(|_| unavailable())?;
        if !metadata.file_type().is_dir() {
            return Err(rejected());
        }
        protect(directory)?;
        let exporter = Self {
            directory: directory.to_path_buf(),
            quota_bytes,
            operation: Operation::Idle,
        };
        exporter.quarantine_temporaries()?;
        Ok(exporter)
    }

    fn quarantine_temporaries(&self) -> Result<(), ExporterError> {
        let mut changed = false;
        for entry in fs::read_dir(&self.directory).map_err(|_| unavailable())? {
            let entry = entry.map_err(|_| unavailable())?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if entry.file_type().map_err(|_| unavailable())?.is_file()
                && name.starts_with(".batch-")
                && name.ends_with(".tmp")
            {
                let quarantine = self.directory.join(format!("{name}.incomplete"));
                fs::rename(entry.path(), quarantine).map_err(|_| unavailable())?;
                changed = true;
            }
        }
        if changed {
            sync_directory(&self.directory)?;
        }
        Ok(())
    }

    fn ensure_quota(&self, incoming: u64, final_path: &Path) -> Result<(), ExporterError> {
        if incoming > self.quota_bytes {
            return Err(rejected());
        }
        let mut retained = Vec::new();
        let mut used = 0_u64;
        for entry in fs::read_dir(&self.directory).map_err(|_| unavailable())? {
            let entry = entry.map_err(|_| unavailable())?;
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
        let mut changed = false;
        for (_, path, size) in retained {
            if used.checked_add(incoming).is_some_and(|total| total <= self.quota_bytes) {
                break;
            }
            fs::remove_file(path).map_err(|_| unavailable())?;
            used = used.checked_sub(size).ok_or_else(protocol)?;
            changed = true;
        }
        if used.checked_add(incoming).is_none_or(|total| total > self.quota_bytes) {
            return Err(rejected());
        }
        if changed {
            sync_directory(&self.directory)?;
        }
        Ok(())
    }

    fn advance_export(
        &self,
        pending: &mut PendingExport,
        max_bytes: usize,
    ) -> Result<ExportPoll, ExporterError> {
        match pending.stage {
            ExportStage::Prepare => {
                if pending.final_path.exists() {
                    if read_bounded(&pending.final_path, pending.bytes.len())? != pending.bytes {
                        return Err(protocol());
                    }
                    sync_directory(&self.directory)?;
                    pending.written = pending.bytes.len();
                    return Ok(ExportPoll::Accepted {
                        ack: pending.ack,
                        progress: pending.progress(ExportPhase::Complete),
                    });
                }
                self.ensure_quota(
                    u64::try_from(pending.bytes.len()).map_err(|_| rejected())?,
                    &pending.final_path,
                )?;
                let temporary_path = self.directory.join(format!(
                    ".batch-{}-{}.tmp",
                    std::process::id(),
                    TEMPORARY_NONCE.fetch_add(1, Ordering::Relaxed),
                ));
                let file = OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&temporary_path)
                    .map_err(|_| unavailable())?;
                pending.temporary_path = Some(temporary_path);
                pending.file = Some(file);
                pending.stage = ExportStage::Writing;
                Ok(ExportPoll::Pending(pending.progress(ExportPhase::Writing)))
            }
            ExportStage::Writing => {
                let end = pending
                    .written
                    .checked_add(max_bytes)
                    .unwrap_or(usize::MAX)
                    .min(pending.bytes.len());
                let written = pending
                    .file
                    .as_mut()
                    .ok_or_else(active_protocol)?
                    .write(&pending.bytes[pending.written..end])
                    .map_err(|_| active_unavailable())?;
                if written == 0 && pending.written != pending.bytes.len() {
                    return Err(active_unavailable());
                }
                pending.written = pending
                    .written
                    .checked_add(written)
                    .ok_or_else(active_protocol)?;
                if pending.written == pending.bytes.len() {
                    pending.stage = ExportStage::SyncFile;
                }
                Ok(ExportPoll::Pending(pending.progress(ExportPhase::Writing)))
            }
            ExportStage::SyncFile => {
                pending
                    .file
                    .as_ref()
                    .ok_or_else(active_protocol)?
                    .sync_all()
                    .map_err(|_| active_unavailable())?;
                pending.file = None;
                pending.stage = ExportStage::Publish;
                Ok(ExportPoll::Pending(pending.progress(ExportPhase::Committing)))
            }
            ExportStage::Publish => {
                let temporary_path = pending
                    .temporary_path
                    .as_ref()
                    .ok_or_else(active_protocol)?;
                fs::rename(temporary_path, &pending.final_path)
                    .map_err(|_| active_unavailable())?;
                pending.temporary_path = None;
                pending.owns_final = true;
                pending.stage = ExportStage::SyncDirectory;
                Ok(ExportPoll::Pending(pending.progress(ExportPhase::Committing)))
            }
            ExportStage::SyncDirectory => {
                sync_directory(&self.directory).map_err(|_| active_unavailable())?;
                Ok(ExportPoll::Accepted {
                    ack: pending.ack,
                    progress: pending.progress(ExportPhase::Complete),
                })
            }
        }
    }

    fn cancel_export(
        &self,
        pending: &mut PendingExport,
    ) -> Result<ExportPoll, ExporterError> {
        pending.file = None;
        if let Some(path) = pending.temporary_path.take() {
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(_) => {
                    pending.temporary_path = Some(path);
                    return Err(active_unavailable());
                }
            }
        }
        if pending.owns_final {
            match fs::remove_file(&pending.final_path) {
                Ok(()) => pending.owns_final = false,
                Err(error) if error.kind() == ErrorKind::NotFound => pending.owns_final = false,
                Err(_) => return Err(active_unavailable()),
            }
        }
        sync_directory(&self.directory).map_err(|_| active_unavailable())?;
        Ok(ExportPoll::Cancelled(pending.progress(ExportPhase::Complete)))
    }
}

impl PendingExport {
    fn progress(&self, phase: ExportPhase) -> ExportProgress {
        ExportProgress::new(
            phase,
            u64::try_from(self.written).unwrap_or(u64::MAX),
            u64::try_from(self.bytes.len()).ok(),
        )
    }
}

impl Exporter for LocalFileExporter {
    fn begin_export(&mut self, batch: &ExportBatch) -> Result<ExportProgress, ExporterError> {
        if !matches!(&self.operation, Operation::Idle) {
            return Err(protocol());
        }
        let bytes = batch.canonical_bytes().map_err(|_| protocol())?;
        let total = u64::try_from(bytes.len()).map_err(|_| rejected())?;
        let name = format!(
            "batch-{:020}-{:020}-{}.bin",
            batch.first_sequence(),
            batch.last_sequence(),
            digest_hex(batch.batch_id().as_bytes()),
        );
        self.operation = Operation::Export(PendingExport {
            ack: ExportAck::accept(batch),
            bytes,
            final_path: self.directory.join(name),
            temporary_path: None,
            file: None,
            written: 0,
            stage: ExportStage::Prepare,
            owns_final: false,
        });
        Ok(ExportProgress::new(ExportPhase::Prepared, 0, Some(total)))
    }

    fn poll_export(&mut self, control: ExportPollControl) -> Result<ExportPoll, ExporterError> {
        let operation = mem::replace(&mut self.operation, Operation::Idle);
        let mut pending = match operation {
            Operation::Export(pending) => pending,
            operation => {
                self.operation = operation;
                return Err(protocol());
            }
        };
        let result = match control {
            ExportPollControl::Continue { max_bytes } => {
                self.advance_export(&mut pending, max_bytes.get())
            }
            ExportPollControl::Cancel => self.cancel_export(&mut pending),
        };
        match result {
            Ok(ExportPoll::Pending(progress)) => {
                self.operation = Operation::Export(pending);
                Ok(ExportPoll::Pending(progress))
            }
            Ok(terminal) => Ok(terminal),
            Err(error) => {
                if !error.cleanup_complete() {
                    self.operation = Operation::Export(pending);
                }
                Err(error)
            }
        }
    }

    fn begin_shutdown(&mut self) -> Result<ExportProgress, ExporterError> {
        if !matches!(&self.operation, Operation::Idle) {
            return Err(protocol());
        }
        self.operation = Operation::Shutdown;
        Ok(ExportProgress::new(ExportPhase::Cleaning, 0, None))
    }

    fn poll_shutdown(
        &mut self,
        control: ExportPollControl,
    ) -> Result<ExporterShutdownPoll, ExporterError> {
        if !matches!(&self.operation, Operation::Shutdown) {
            return Err(protocol());
        }
        match control {
            ExportPollControl::Continue { .. } => {
                sync_directory(&self.directory)
                    .map_err(|_| ExporterError::with_cleanup_status(
                        ExporterErrorCode::Shutdown,
                        true,
                        false,
                    ))?;
                self.operation = Operation::Idle;
                Ok(ExporterShutdownPoll::Complete(ExportProgress::new(
                    ExportPhase::Complete,
                    0,
                    None,
                )))
            }
            ExportPollControl::Cancel => {
                self.operation = Operation::Idle;
                Ok(ExporterShutdownPoll::Cancelled(ExportProgress::new(
                    ExportPhase::Complete,
                    0,
                    None,
                )))
            }
        }
    }
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
    if bytes.len() != expected {
        return Err(protocol());
    }
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

#[cfg(not(any(unix, windows)))]
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

#[cfg(not(any(unix, windows)))]
fn sync_directory(path: &Path) -> Result<(), ExporterError> {
    File::open(path).and_then(|file| file.sync_all()).map_err(|_| unavailable())
}

const fn unavailable() -> ExporterError {
    ExporterError::new(ExporterErrorCode::Unavailable, true)
}

const fn active_unavailable() -> ExporterError {
    ExporterError::with_cleanup_status(ExporterErrorCode::Unavailable, true, false)
}

const fn rejected() -> ExporterError {
    ExporterError::new(ExporterErrorCode::Rejected, false)
}

const fn protocol() -> ExporterError {
    ExporterError::new(ExporterErrorCode::Protocol, false)
}

const fn active_protocol() -> ExporterError {
    ExporterError::with_cleanup_status(ExporterErrorCode::Protocol, false, false)
}
