//! Private remove-on-drop files for bounded-memory proxy state.

use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::PathBuf,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

static NEXT_FILE_ID: AtomicU64 = AtomicU64::new(1);

pub(super) struct TemporaryFile {
    file: Mutex<Option<File>>,
    path: PathBuf,
}

impl TemporaryFile {
    pub(super) fn create(purpose: &str) -> io::Result<Self> {
        loop {
            let identifier = NEXT_FILE_ID
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                    current.checked_add(1)
                })
                .map_err(|_| io::Error::other("temporary proxy file identifier overflowed"))?;
            let path = std::env::temp_dir().join(format!(
                "peritus-network-{purpose}-{}-{identifier}.tmp",
                std::process::id(),
            ));
            let mut options = OpenOptions::new();
            options.read(true).write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            match options.open(&path) {
                Ok(file) => return Ok(Self { file: Mutex::new(Some(file)), path }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
    }

    pub(super) fn append(&self, bytes: &[u8]) -> io::Result<()> {
        let mut guard = self.file.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let file = guard
            .as_mut()
            .ok_or_else(|| io::Error::other("temporary proxy file is closed"))?;
        file.seek(SeekFrom::End(0))?;
        file.write_all(bytes)
    }

    pub(super) fn write_all_at(&self, offset: u64, bytes: &[u8]) -> io::Result<()> {
        let mut guard = self.file.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let file = guard
            .as_mut()
            .ok_or_else(|| io::Error::other("temporary proxy file is closed"))?;
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(bytes)
    }

    pub(super) fn read_exact_at(&self, offset: u64, bytes: &mut [u8]) -> io::Result<()> {
        let mut guard = self.file.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let file = guard
            .as_mut()
            .ok_or_else(|| io::Error::other("temporary proxy file is closed"))?;
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(bytes)
    }

    pub(super) fn replace(&self, bytes: &[u8]) -> io::Result<()> {
        let mut guard = self.file.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let file = guard
            .as_mut()
            .ok_or_else(|| io::Error::other("temporary proxy file is closed"))?;
        file.set_len(0)?;
        file.seek(SeekFrom::Start(0))?;
        file.write_all(bytes)
    }

    pub(super) fn begin_replace(&self) -> io::Result<()> {
        let mut guard = self.file.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let file = guard
            .as_mut()
            .ok_or_else(|| io::Error::other("temporary proxy file is closed"))?;
        file.set_len(0)?;
        file.seek(SeekFrom::Start(0)).map(|_| ())
    }

    pub(super) fn flush(&self) -> io::Result<()> {
        let mut guard = self.file.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        guard
            .as_mut()
            .ok_or_else(|| io::Error::other("temporary proxy file is closed"))?
            .flush()
    }

    pub(super) fn sync_data(&self) -> io::Result<()> {
        let guard = self.file.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        guard
            .as_ref()
            .ok_or_else(|| io::Error::other("temporary proxy file is closed"))?
            .sync_data()
    }
}

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        let file = self
            .file
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        drop(file);
        let _ = std::fs::remove_file(&self.path);
    }
}
