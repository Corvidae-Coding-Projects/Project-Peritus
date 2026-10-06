//! Owned streaming before-images, independent of inline command payload admission.

use crate::{FileMode, PatchError, PatchOperationContext, Preimage, RollbackStatus};
use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};
use std::{
    fs::File,
    io::{Read, Seek as _, SeekFrom, Write},
    sync::{Arc, Mutex},
};

/// An owned streaming source and its exact immutable snapshot declaration.
///
/// Clones share an owned source. Staging hashes the complete
/// stream and rejects changed bytes before the first workspace mutation. Size is an identity
/// assertion rather than a command payload ceiling.
#[derive(Clone, Debug)]
pub struct SnapshotFile {
    source: Arc<dyn SnapshotSource>,
    identity: Preimage,
}

impl SnapshotFile {
    /// Binds an owned snapshot handle to its expected bytes and portable mode.
    /// The transaction verifies this declaration while staging, before installing any target.
    #[must_use]
    pub fn new(file: File, digest: Sha256Digest, size: u64, mode: FileMode) -> Self {
        Self::from_source(Arc::new(OwnedFile(Arc::new(Mutex::new(file)))), digest, size, mode)
    }

    /// Binds a lazy, owned snapshot source without holding a descriptor for every saved path.
    #[must_use]
    pub fn from_source(
        source: Arc<dyn SnapshotSource>,
        digest: Sha256Digest,
        size: u64,
        mode: FileMode,
    ) -> Self {
        Self { source, identity: Preimage::present(digest, size, mode) }
    }

    /// Returns the exact declared content identity.
    #[must_use]
    pub const fn identity(&self) -> Preimage {
        self.identity
    }

    /// Streams and verifies the complete immutable snapshot into caller-owned storage.
    ///
    /// # Errors
    /// Rejects changed, truncated or excess content and storage/read failures.
    pub fn write_to(&self, output: &mut dyn Write) -> Result<(), PatchError> {
        let mut copy = || -> std::io::Result<()> {
            let mut file = self.source.open()?;
            let Preimage::Present { digest: expected, size, .. } = self.identity else {
                return Err(std::io::Error::other("snapshot has no content identity"));
            };
            let mut hasher = Sha256::new();
            let mut remaining = size;
            let mut chunk = vec![0_u8; 64 * 1024];
            while remaining != 0 {
                let requested = chunk.len().min(usize::try_from(remaining).unwrap_or(usize::MAX));
                let count = file.read(&mut chunk[..requested])?;
                if count == 0 {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "snapshot truncated",
                    ));
                }
                output.write_all(&chunk[..count])?;
                hasher.update(&chunk[..count]);
                remaining -= count as u64;
            }
            if file.read(&mut chunk[..1])? != 0
                || Sha256Digest::new(hasher.finalize().into()) != expected
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "snapshot content changed",
                ));
            }
            drop(file);
            Ok(())
        };
        copy().map_err(|error| {
            if error.kind() == std::io::ErrorKind::InvalidData {
                PatchError::message(
                    crate::ErrorCode::InvalidContent,
                    crate::RecoveryClass::CorrectPatch,
                    PatchOperationContext::StageFinal,
                    RollbackStatus::NotRequired,
                    "retained snapshot differs from its declared content identity",
                )
                .with_io_source(error)
            } else {
                PatchError::io(
                    PatchOperationContext::StageFinal,
                    RollbackStatus::NotRequired,
                    error,
                )
            }
        })
    }
}

/// Owns the lifetime of retained snapshot bytes and opens an independent sequential reader.
/// Sources may be backed by protected immutable artifacts or owned temporary storage.
pub trait SnapshotSource: std::fmt::Debug + Send + Sync {
    /// Opens a reader at byte zero. Staging independently verifies the complete returned bytes.
    ///
    /// # Errors
    /// Returns the source's original storage or integrity error.
    fn open(&self) -> std::io::Result<Box<dyn Read + Send>>;
}

#[derive(Debug)]
struct OwnedFile(Arc<Mutex<File>>);
impl SnapshotSource for OwnedFile {
    fn open(&self) -> std::io::Result<Box<dyn Read + Send>> {
        Ok(Box::new(OwnedReader { file: Arc::clone(&self.0), offset: 0 }))
    }
}
struct OwnedReader {
    file: Arc<Mutex<File>>,
    offset: u64,
}
impl Read for OwnedReader {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        let mut file =
            self.file.lock().map_err(|_| std::io::Error::other("snapshot reader poisoned"))?;
        file.seek(SeekFrom::Start(self.offset))?;
        let count = file.read(bytes)?;
        drop(file);
        self.offset = self
            .offset
            .checked_add(count as u64)
            .ok_or_else(|| std::io::Error::other("snapshot offset overflow"))?;
        Ok(count)
    }
}

impl PartialEq for SnapshotFile {
    fn eq(&self, other: &Self) -> bool {
        self.identity == other.identity
    }
}
impl Eq for SnapshotFile {}
