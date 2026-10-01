//! Reserve a writable, run-bound acknowledgement before restoration changes files.

use super::{
    Completed, ProductDeliverable, ProductRunServiceError, RunRecord, binding, failure, path,
    read_record, sync_directory,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read as _, Seek as _, Write as _},
    path::{Path, PathBuf},
};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Prepared {
    version: u8,
    binding: [u8; 32],
}

pub(in crate::product_run::deliverable) struct Reservation {
    owner: Option<fs::File>,
    file: Option<fs::File>,
    temporary: PathBuf,
    final_path: PathBuf,
    binding: [u8; 32],
}

impl Reservation {
    pub(in crate::product_run::deliverable) fn prepare(
        directory: &Path,
        record: &RunRecord,
        deliverable: &ProductDeliverable,
    ) -> Result<Self, ProductRunServiceError> {
        let binding = binding(record, deliverable)?;
        let final_path = path(directory, record);
        let ownership = final_path.with_extension("discard-reservation");
        if read_record(&ownership)?.is_some_and(|bytes| !bytes.is_empty()) {
            return Err(failure("discard reservation ownership contains foreign data"));
        }
        let owner = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(ownership)
            .map_err(failure)?;
        owner.try_lock().map_err(failure)?;
        let mut reservation = Self {
            owner: Some(owner),
            file: None,
            temporary: final_path.with_extension("discard-result.new"),
            final_path,
            binding,
        };
        let temporary = &reservation.temporary;
        // Always create in the destination directory, including when reopening an
        // earlier reservation. Writing an existing file alone cannot establish that
        // the directory allows the completion rename.
        let mut staging = tempfile::NamedTempFile::new_in(directory).map_err(failure)?;
        let prepared = Prepared { version: 1, binding };
        staging
            .write_all(&serde_json::to_vec(&prepared).map_err(failure)?)
            .and_then(|()| staging.as_file().sync_all())
            .map_err(failure)?;
        let file = match staging.persist_noclobber(temporary) {
            Ok(file) => file,
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                let bytes = read_record(temporary)?
                    .ok_or_else(|| failure("discard reservation disappeared"))?;
                let existing: Prepared = serde_json::from_slice(&bytes).map_err(failure)?;
                if existing.version != prepared.version || existing.binding != binding {
                    return Err(failure("discard reservation belongs to another candidate"));
                }
                fs::OpenOptions::new().read(true).write(true).open(temporary).map_err(failure)?
            }
            Err(error) => return Err(failure(error)),
        };
        file.try_lock().map_err(failure)?;
        reservation.file = Some(file);
        let file =
            reservation.file.as_mut().ok_or_else(|| failure("discard reservation is closed"))?;
        // Validate again through the locked handle: another holder may have
        // completed the reservation between the first read and lock acquisition.
        file.rewind().map_err(failure)?;
        let mut bytes = Vec::new();
        std::io::Read::by_ref(file).take(1025).read_to_end(&mut bytes).map_err(failure)?;
        let locked: Prepared = serde_json::from_slice(&bytes).map_err(failure)?;
        if locked.version != prepared.version || locked.binding != binding {
            return Err(failure("locked discard reservation does not match its candidate"));
        }
        sync_directory(directory)?;
        Ok(reservation)
    }

    pub(in crate::product_run::deliverable) fn complete(
        mut self,
        status: &str,
    ) -> Result<(), ProductRunServiceError> {
        let completed = Completed { version: 1, binding: self.binding, status: status.to_owned() };
        let bytes = serde_json::to_vec(&completed).map_err(failure)?;
        let parent =
            self.final_path.parent().ok_or_else(|| failure("discard result has no parent"))?;
        let mut staging = tempfile::NamedTempFile::new_in(parent).map_err(failure)?;
        staging.write_all(&bytes).and_then(|()| staging.as_file().sync_all()).map_err(failure)?;
        let file = self.file.as_mut().ok_or_else(|| failure("discard reservation is closed"))?;
        // Closing permits replacement on Windows. A complete record is published
        // atomically, so a torn final acknowledgement cannot corrupt the reservation.
        file.unlock().map_err(failure)?;
        drop(self.file.take());
        staging.persist(&self.temporary).map_err(failure)?;
        sync_directory(parent)?;
        if let Some(existing) = read_record(&self.final_path)? {
            let completed: Completed = serde_json::from_slice(&existing).map_err(failure)?;
            if completed.version != 1 || completed.binding != self.binding {
                return Err(failure("foreign completed discard record was preserved"));
            }
        }
        fs::rename(&self.temporary, &self.final_path).map_err(failure)?;
        sync_directory(parent)
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        // Explicit unlock also releases a lock briefly inherited by a subprocess
        // during fork. Closing only this handle can leave that inherited lock held.
        if let Some(file) = self.file.take()
            && let Err(error) = file.unlock()
        {
            let _ = std::io::stderr()
                .lock()
                .write_all(format!("discard reservation unlock failed: {error}\n").as_bytes());
        }
        if let Some(owner) = self.owner.take()
            && let Err(error) = owner.unlock()
        {
            let _ = std::io::stderr().lock().write_all(
                format!("discard reservation ownership unlock failed: {error}\n").as_bytes(),
            );
        }
    }
}
