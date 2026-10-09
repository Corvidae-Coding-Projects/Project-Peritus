//! Owned streaming handles for long-lived daemon transfers.

use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    path::PathBuf,
};

use sha2::{Digest, Sha256};

use crate::{
    ArtifactDigest, ArtifactMetadata, ArtifactStoreError, ErrorCode, FinalizationState,
    FinalizedArtifact, Publication, QuarantineState, RecoveryClass, StoreOperation, WriteRequest,
    catalog::Catalog,
    finalize::{publish, synchronize_temporary, verify_finalized},
    path::{StorePaths, io, sync_directory},
};

/// One contiguous owned read result with its exact starting byte offset.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactReadChunk {
    offset: u64,
    bytes: Vec<u8>,
}

impl ArtifactReadChunk {
    /// Returns the exact zero-based byte offset.
    #[must_use]
    pub const fn offset(&self) -> u64 {
        self.offset
    }

    /// Borrows the nonempty contiguous bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Owned preverified reader for one immutable finalized artifact.
pub struct ArtifactReadHandle {
    file: File,
    metadata: ArtifactMetadata,
    next_offset: u64,
    maximum_chunk_bytes: u64,
}

impl ArtifactReadHandle {
    pub(crate) fn open(
        paths: &StorePaths,
        metadata: ArtifactMetadata,
        maximum_chunk_bytes: u64,
    ) -> Result<Self, ArtifactStoreError> {
        let path = paths.object(metadata.digest());
        let path_metadata = fs::symlink_metadata(&path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                missing_artifact()
            } else {
                io(StoreOperation::InspectObject, error)
            }
        })?;
        if !path_metadata.file_type().is_file() {
            return Err(corrupt("artifact object is not a regular file"));
        }
        let mut file =
            File::open(path).map_err(|error| io(StoreOperation::InspectObject, error))?;
        verify_open_file(&mut file, metadata.digest(), metadata.size())?;
        Ok(Self { file, metadata, next_offset: 0, maximum_chunk_bytes })
    }

    /// Borrows the exact durable metadata verified before streaming.
    #[must_use]
    pub const fn metadata(&self) -> &ArtifactMetadata {
        &self.metadata
    }

    /// Returns the next unread byte offset.
    #[must_use]
    pub const fn next_offset(&self) -> u64 {
        self.next_offset
    }

    /// Returns the exact unread byte count.
    #[must_use]
    pub const fn remaining_bytes(&self) -> u64 {
        self.metadata.size() - self.next_offset
    }

    /// Reads the next nonempty bounded contiguous chunk, or `None` at exact completion.
    ///
    /// # Errors
    ///
    /// Rejects a zero/oversized bound, allocation overflow, I/O failure, or content mutation.
    pub fn read_chunk(
        &mut self,
        maximum_bytes: usize,
    ) -> Result<Option<ArtifactReadChunk>, ArtifactStoreError> {
        let maximum_u64 = u64::try_from(maximum_bytes).map_err(|_| overflow())?;
        if maximum_bytes == 0 || maximum_u64 > self.maximum_chunk_bytes {
            return Err(ArtifactStoreError::limit(
                ErrorCode::ByteLimitExceeded,
                maximum_u64,
                self.maximum_chunk_bytes,
            ));
        }
        let remaining = self.remaining_bytes();
        if remaining == 0 {
            return Ok(None);
        }
        let count = usize::try_from(remaining.min(maximum_u64)).map_err(|_| overflow())?;
        let mut bytes = vec![0_u8; count];
        if let Err(error) = self.file.read_exact(&mut bytes) {
            return Err(if error.kind() == std::io::ErrorKind::UnexpectedEof {
                corrupt("artifact bytes ended before the verified durable size")
            } else {
                io(StoreOperation::InspectObject, error)
            });
        }
        let offset = self.next_offset;
        self.next_offset = self
            .next_offset
            .checked_add(u64::try_from(count).map_err(|_| overflow())?)
            .ok_or_else(overflow)?;
        if self.next_offset == self.metadata.size() {
            let mut trailing = [0_u8; 1];
            if self
                .file
                .read(&mut trailing)
                .map_err(|error| io(StoreOperation::InspectObject, error))?
                != 0
            {
                return Err(corrupt("artifact bytes grew after stream verification"));
            }
        }
        Ok(Some(ArtifactReadChunk { offset, bytes }))
    }
}

/// Owned exclusive temporary writer that can live in a daemon transfer registry.
#[must_use = "an owned writer must be completed through ArtifactStore or explicitly dropped"]
pub struct ArtifactWriteHandle {
    store_root: PathBuf,
    request: WriteRequest,
    temporary_path: Option<PathBuf>,
    file: Option<File>,
    hasher: Sha256,
    written: u64,
    failed: bool,
    quota_limit: u64,
}

impl ArtifactWriteHandle {
    pub(crate) fn create(
        paths: &StorePaths,
        request: WriteRequest,
        configured_limit: u64,
        quota_limit: u64,
    ) -> Result<Self, ArtifactStoreError> {
        if !crate::verified::write_bounds_valid(
            request.expected_size(),
            request.declared_limit(),
            configured_limit,
        ) {
            return Err(invalid_request(
                "expected size, declared limit, and configured limit are inconsistent",
            ));
        }
        let (file, temporary_path) =
            crate::writer::create_temporary(paths, request.expected_digest())?;
        Ok(Self {
            store_root: paths.root().to_path_buf(),
            request,
            temporary_path: Some(temporary_path),
            file: Some(file),
            hasher: Sha256::new(),
            written: 0,
            failed: false,
            quota_limit,
        })
    }

    /// Streams one complete chunk or rejects it without a partial chunk write.
    ///
    /// # Errors
    ///
    /// Returns a byte-limit, overflow, state, or I/O error and poisons the handle after a write
    /// failure.
    pub fn write_chunk(&mut self, chunk: &[u8]) -> Result<(), ArtifactStoreError> {
        if self.failed {
            return Err(invalid_request("writer is unusable after a prior write failure"));
        }
        let chunk_size = u64::try_from(chunk.len()).map_err(|_| overflow())?;
        let attempted = self.written.checked_add(chunk_size).ok_or_else(overflow)?;
        if attempted > self.request.declared_limit() {
            self.failed = true;
            return Err(ArtifactStoreError::limit(
                ErrorCode::ByteLimitExceeded,
                attempted,
                self.request.declared_limit(),
            ));
        }
        let file = self.file.as_mut().ok_or_else(|| invalid_request("writer is closed"))?;
        if let Err(error) = file.write_all(chunk) {
            self.failed = true;
            return Err(io(StoreOperation::WriteTemporary, error));
        }
        self.hasher.update(chunk);
        self.written = attempted;
        Ok(())
    }

    /// Returns bytes successfully accepted so far.
    #[must_use]
    pub const fn bytes_written(&self) -> u64 {
        self.written
    }

    pub(crate) fn complete(
        mut self,
        paths: &StorePaths,
        catalog: &Catalog,
    ) -> Result<FinalizedArtifact, ArtifactStoreError> {
        if self.store_root != paths.root() {
            return Err(invalid_request("writer belongs to another artifact store"));
        }
        if self.failed {
            return Err(invalid_request("cannot complete a writer after a write failure"));
        }
        if self.written != self.request.expected_size() {
            return Err(ArtifactStoreError::mismatch(
                ErrorCode::SizeMismatch,
                self.request.expected_size(),
                self.written,
            ));
        }
        let actual_digest = ArtifactDigest::new(std::mem::take(&mut self.hasher).finalize().into());
        if actual_digest != self.request.expected_digest() {
            return Err(ArtifactStoreError::message(
                ErrorCode::DigestMismatch,
                RecoveryClass::CorrectRequest,
                "streamed artifact digest does not match the declared digest",
            ));
        }
        let file = self.file.as_mut().ok_or_else(|| invalid_request("writer is closed"))?;
        synchronize_temporary(file)?;
        self.file.take();
        let temporary = self
            .temporary_path
            .as_ref()
            .ok_or_else(|| invalid_request("temporary path is unavailable"))?;
        let destination = paths.object(self.request.expected_digest());
        let destination_parent = paths.ensure_object_parent(self.request.expected_digest())?;
        let publication = publish(
            temporary,
            &destination,
            &destination_parent,
            paths.temporary(),
            self.request.expected_digest(),
            self.request.expected_size(),
        )?;
        self.temporary_path.take();
        let metadata = ArtifactMetadata::new(
            self.request.expected_digest(),
            self.request.expected_size(),
            self.request.media_type().clone(),
            self.request.encryption().clone(),
            FinalizationState::Finalized,
            self.request.creating_event(),
            QuarantineState::Active,
        );
        let restored = match catalog.record_finalized(&metadata, self.quota_limit) {
            Ok(restored) => restored,
            Err(error)
                if publication == Publication::New
                    && matches!(
                        error.code(),
                        ErrorCode::QuotaExceeded | ErrorCode::ArithmeticOverflow
                    ) =>
            {
                fs::remove_file(&destination)
                    .map_err(|remove_error| io(StoreOperation::Remove, remove_error))?;
                sync_directory(&destination_parent)?;
                return Err(error);
            }
            Err(error) => return Err(error),
        };
        if restored {
            let quarantine = paths.quarantine(self.request.expected_digest());
            match fs::symlink_metadata(&quarantine) {
                Ok(file_metadata) if file_metadata.file_type().is_file() => {
                    verify_finalized(
                        &quarantine,
                        self.request.expected_digest(),
                        self.request.expected_size(),
                    )?;
                    fs::remove_file(quarantine)
                        .map_err(|error| io(StoreOperation::Remove, error))?;
                    sync_directory(
                        &paths.ensure_quarantine_parent(self.request.expected_digest())?,
                    )?;
                }
                Ok(_) => return Err(corrupt("quarantine path is not a regular file")),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(io(StoreOperation::InspectObject, error)),
            }
        }
        Ok(FinalizedArtifact::new(
            self.request.expected_digest(),
            self.request.expected_size(),
            publication,
        ))
    }
}

impl Drop for ArtifactWriteHandle {
    fn drop(&mut self) {
        self.file.take();
        if let Some(path) = self.temporary_path.take() {
            let _ = fs::remove_file(path);
        }
    }
}

fn verify_open_file(
    file: &mut File,
    expected_digest: ArtifactDigest,
    expected_size: u64,
) -> Result<(), ArtifactStoreError> {
    let mut hasher = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read =
            file.read(&mut buffer).map_err(|error| io(StoreOperation::InspectObject, error))?;
        if read == 0 {
            break;
        }
        size =
            size.checked_add(u64::try_from(read).map_err(|_| overflow())?).ok_or_else(overflow)?;
        hasher.update(&buffer[..read]);
    }
    if size != expected_size || ArtifactDigest::new(hasher.finalize().into()) != expected_digest {
        return Err(corrupt("artifact bytes disagree with durable digest or size"));
    }
    file.seek(SeekFrom::Start(0)).map_err(|error| io(StoreOperation::InspectObject, error))?;
    Ok(())
}

const fn invalid_request(message: &'static str) -> ArtifactStoreError {
    ArtifactStoreError::message(
        ErrorCode::InvalidWriteRequest,
        RecoveryClass::CorrectRequest,
        message,
    )
}

const fn corrupt(message: &'static str) -> ArtifactStoreError {
    ArtifactStoreError::message(ErrorCode::CorruptObject, RecoveryClass::TerminalIntegrity, message)
}

const fn missing_artifact() -> ArtifactStoreError {
    ArtifactStoreError::message(
        ErrorCode::MissingArtifact,
        RecoveryClass::RecoverStore,
        "artifact file is missing",
    )
}

const fn overflow() -> ArtifactStoreError {
    ArtifactStoreError::message(
        ErrorCode::ArithmeticOverflow,
        RecoveryClass::TerminalIntegrity,
        "artifact byte count overflowed",
    )
}
