//! Owned streaming handles for long-lived daemon transfers.

use std::{
    fs::{self, File},
    io::Write,
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

mod reader;
pub use reader::ArtifactReadHandle;

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
    quota_limit: Option<u64>,
    verified: bool,
    publication: Option<Publication>,
    operation_identity: u64,
    publication_reserved: bool,
}

impl ArtifactWriteHandle {
    pub(crate) fn create(
        paths: &StorePaths,
        catalog: &Catalog,
        request: WriteRequest,
        configured_limit: Option<u64>,
        quota_limit: Option<u64>,
        minimum_free_bytes: u64,
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
        let writer_identity = catalog.allocate_operation_identity()?;
        let (file, temporary_path) = crate::writer::create_temporary(
            paths,
            request.expected_digest(),
            writer_identity,
            request.expected_size(),
            minimum_free_bytes,
        )?;
        Ok(Self {
            store_root: paths.root().to_path_buf(),
            request,
            temporary_path: Some(temporary_path),
            file: Some(file),
            hasher: Sha256::new(),
            written: 0,
            failed: false,
            quota_limit,
            verified: false,
            publication: None,
            operation_identity: writer_identity,
            publication_reserved: false,
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
        &mut self,
        paths: &StorePaths,
        catalog: &Catalog,
    ) -> Result<FinalizedArtifact, ArtifactStoreError> {
        if self.store_root != paths.root() {
            return Err(invalid_request("writer belongs to another artifact store"));
        }
        if self.failed {
            return Err(invalid_request("cannot complete a writer after a write failure"));
        }
        if !self.verified {
            if self.written != self.request.expected_size() {
                return Err(ArtifactStoreError::mismatch(
                    ErrorCode::SizeMismatch,
                    self.request.expected_size(),
                    self.written,
                ));
            }
            let actual_digest = ArtifactDigest::new(self.hasher.clone().finalize().into());
            if actual_digest != self.request.expected_digest() {
                return Err(ArtifactStoreError::message(
                    ErrorCode::DigestMismatch,
                    RecoveryClass::CorrectRequest,
                    "streamed artifact digest does not match the declared digest",
                ));
            }
            let file = self.file.as_mut().ok_or_else(|| invalid_request("writer is closed"))?;
            synchronize_temporary(file)?;
            sync_directory(paths.temporary())?;
            self.file.take();
            self.hasher = Sha256::new();
            self.verified = true;
        }
        let partial = ArtifactMetadata::new(
            self.request.expected_digest(),
            self.request.expected_size(),
            self.request.media_type().clone(),
            self.request.encryption().clone(),
            FinalizationState::Partial,
            self.request.creating_event(),
            QuarantineState::Active,
        );
        catalog.reserve_publication(
            &partial,
            self.operation_identity,
            self.quota_limit,
        )?;
        self.publication_reserved = true;
        let destination = paths.object(self.request.expected_digest());
        let destination_parent = paths.ensure_object_parent(self.request.expected_digest())?;
        if self.temporary_path.is_some() {
            let temporary = self
                .temporary_path
                .as_ref()
                .ok_or_else(|| invalid_request("temporary path is unavailable"))?;
            publish(
                temporary,
                &destination,
                &destination_parent,
                paths.temporary(),
                self.request.expected_digest(),
                self.request.expected_size(),
                &mut self.publication,
            )?;
            self.temporary_path.take();
        }
        let publication = self
            .publication
            .ok_or_else(|| invalid_request("publication receipt is unavailable"))?;
        let metadata = ArtifactMetadata::new(
            self.request.expected_digest(),
            self.request.expected_size(),
            self.request.media_type().clone(),
            self.request.encryption().clone(),
            FinalizationState::Finalized,
            self.request.creating_event(),
            QuarantineState::Active,
        );
        let restored = catalog.record_finalized(&metadata, self.quota_limit)?;
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
        if !self.publication_reserved
            && let Some(path) = self.temporary_path.take()
        {
            let _ = fs::remove_file(path);
        }
    }
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
