//! Bounded streaming artifact writer.

use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use peritus_types::EventId;
use sha2::{Digest, Sha256};

use crate::{
    ArtifactDigest, ArtifactMetadata, ArtifactStoreError, EncryptionMetadata, ErrorCode,
    FinalizationState, MediaType, QuarantineState, RecoveryClass, StoreOperation,
    catalog::Catalog,
    finalize::{publish, synchronize_temporary, verify_finalized},
    path::{StorePaths, io, sync_directory},
    verified::write_bounds_valid,
};

static TEMPORARY_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Validated-at-writer-creation artifact declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WriteRequest {
    expected_digest: ArtifactDigest,
    expected_size: u64,
    declared_limit: u64,
    media_type: MediaType,
    encryption: EncryptionMetadata,
    creating_event: EventId,
}

impl WriteRequest {
    /// Declares exact expected content and durable metadata.
    #[must_use]
    pub const fn new(
        expected_digest: ArtifactDigest,
        expected_size: u64,
        declared_limit: u64,
        media_type: MediaType,
        encryption: EncryptionMetadata,
        creating_event: EventId,
    ) -> Self {
        Self {
            expected_digest,
            expected_size,
            declared_limit,
            media_type,
            encryption,
            creating_event,
        }
    }

    /// Returns the exact expected digest.
    #[must_use]
    pub const fn expected_digest(&self) -> ArtifactDigest {
        self.expected_digest
    }

    /// Returns the exact expected size.
    #[must_use]
    pub const fn expected_size(&self) -> u64 {
        self.expected_size
    }

    /// Returns the caller-declared hard streaming bound.
    #[must_use]
    pub const fn declared_limit(&self) -> u64 {
        self.declared_limit
    }

    /// Borrows the validated media type recorded at finalization.
    #[must_use]
    pub const fn media_type(&self) -> &MediaType {
        &self.media_type
    }

    /// Borrows the external encryption binding metadata.
    #[must_use]
    pub const fn encryption(&self) -> &EncryptionMetadata {
        &self.encryption
    }

    /// Returns the journal event that will own the finalized object.
    #[must_use]
    pub const fn creating_event(&self) -> EventId {
        self.creating_event
    }
}

/// Whether finalization published new bytes or observed identical existing bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Publication {
    /// This writer atomically published the object.
    New,
    /// Identical content was already finalized.
    Existing,
}

/// Exact successful finalization observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FinalizedArtifact {
    digest: ArtifactDigest,
    size: u64,
    publication: Publication,
}

impl FinalizedArtifact {
    pub(crate) const fn new(digest: ArtifactDigest, size: u64, publication: Publication) -> Self {
        Self { digest, size, publication }
    }

    /// Returns the verified content digest.
    #[must_use]
    pub const fn digest(self) -> ArtifactDigest {
        self.digest
    }

    /// Returns the verified byte size.
    #[must_use]
    pub const fn size(self) -> u64 {
        self.size
    }

    /// Returns the publication outcome.
    #[must_use]
    pub const fn publication(self) -> Publication {
        self.publication
    }
}

/// Exclusive temporary writer with incremental SHA-256 and hard byte bounds.
#[must_use = "a writer must be finalized or explicitly dropped"]
pub struct ArtifactWriter<'store> {
    paths: &'store StorePaths,
    catalog: &'store Catalog,
    request: WriteRequest,
    temporary_path: Option<PathBuf>,
    file: Option<File>,
    hasher: Sha256,
    written: u64,
    failed: bool,
    quota_limit: u64,
}

impl<'store> ArtifactWriter<'store> {
    pub(crate) fn create(
        paths: &'store StorePaths,
        catalog: &'store Catalog,
        request: WriteRequest,
        configured_limit: u64,
        quota_limit: u64,
    ) -> Result<Self, ArtifactStoreError> {
        if !write_bounds_valid(request.expected_size, request.declared_limit, configured_limit) {
            return Err(invalid_request(
                "expected size, declared limit, and configured limit are inconsistent",
            ));
        }
        let (file, temporary_path) = create_temporary(paths, request.expected_digest)?;
        Ok(Self {
            paths,
            catalog,
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
    /// Returns a byte-limit, overflow, writer-state, or I/O error. A writer becomes unusable after
    /// the first write failure.
    pub fn write_chunk(&mut self, chunk: &[u8]) -> Result<(), ArtifactStoreError> {
        if self.failed {
            return Err(invalid_request("writer is unusable after a prior write failure"));
        }
        let chunk_size = u64::try_from(chunk.len()).map_err(|_| overflow())?;
        let attempted = self.written.checked_add(chunk_size).ok_or_else(overflow)?;
        if attempted > self.request.declared_limit {
            self.failed = true;
            return Err(ArtifactStoreError::limit(
                ErrorCode::ByteLimitExceeded,
                attempted,
                self.request.declared_limit,
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

    /// Flushes, synchronizes, verifies, atomically publishes, synchronizes the parent directory,
    /// and durably records metadata.
    ///
    /// # Errors
    ///
    /// Returns a typed mismatch, corruption, state, or I/O error. No partial temporary file becomes
    /// authoritative content.
    pub fn finalize(mut self) -> Result<FinalizedArtifact, ArtifactStoreError> {
        if self.failed {
            return Err(invalid_request("cannot finalize a writer after a write failure"));
        }
        if self.written != self.request.expected_size {
            return Err(ArtifactStoreError::mismatch(
                ErrorCode::SizeMismatch,
                self.request.expected_size,
                self.written,
            ));
        }
        let actual_digest = ArtifactDigest::new(std::mem::take(&mut self.hasher).finalize().into());
        if actual_digest != self.request.expected_digest {
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
        let destination = self.paths.object(self.request.expected_digest);
        let destination_parent = self.paths.ensure_object_parent(self.request.expected_digest)?;
        let publication = publish(
            temporary,
            &destination,
            &destination_parent,
            self.paths.temporary(),
            self.request.expected_digest,
            self.request.expected_size,
        )?;
        self.temporary_path.take();
        let metadata = ArtifactMetadata::new(
            self.request.expected_digest,
            self.request.expected_size,
            self.request.media_type.clone(),
            self.request.encryption.clone(),
            FinalizationState::Finalized,
            self.request.creating_event,
            QuarantineState::Active,
        );
        let restored = match self.catalog.record_finalized(&metadata, self.quota_limit) {
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
            let quarantine = self.paths.quarantine(self.request.expected_digest);
            match fs::symlink_metadata(&quarantine) {
                Ok(file_metadata) if file_metadata.file_type().is_file() => {
                    verify_finalized(
                        &quarantine,
                        self.request.expected_digest,
                        self.request.expected_size,
                    )?;
                    fs::remove_file(quarantine)
                        .map_err(|error| io(StoreOperation::Remove, error))?;
                    sync_directory(
                        &self.paths.ensure_quarantine_parent(self.request.expected_digest)?,
                    )?;
                }
                Ok(_) => {
                    return Err(ArtifactStoreError::message(
                        ErrorCode::CorruptObject,
                        RecoveryClass::TerminalIntegrity,
                        "quarantine path is not a regular file",
                    ));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(io(StoreOperation::InspectObject, error)),
            }
        }
        Ok(FinalizedArtifact {
            digest: self.request.expected_digest,
            size: self.request.expected_size,
            publication,
        })
    }
}

impl Drop for ArtifactWriter<'_> {
    fn drop(&mut self) {
        self.file.take();
        if let Some(path) = self.temporary_path.take() {
            let _ = fs::remove_file(path);
        }
    }
}

pub(super) fn create_temporary(
    paths: &StorePaths,
    digest: ArtifactDigest,
) -> Result<(File, PathBuf), ArtifactStoreError> {
    for _ in 0..1_024 {
        let counter = TEMPORARY_COUNTER.fetch_add(1, Ordering::Relaxed);
        let name =
            format!(".artifact-{:08x}-{counter:016x}-{}.tmp", std::process::id(), digest.to_hex());
        let path = paths.temporary().join(name);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((file, path)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(io(StoreOperation::CreateTemporary, error)),
        }
    }
    Err(ArtifactStoreError::message(
        ErrorCode::Io,
        RecoveryClass::Retry,
        "could not allocate a unique temporary artifact name",
    ))
}

const fn invalid_request(message: &'static str) -> ArtifactStoreError {
    ArtifactStoreError::message(
        ErrorCode::InvalidWriteRequest,
        RecoveryClass::CorrectRequest,
        message,
    )
}

const fn overflow() -> ArtifactStoreError {
    ArtifactStoreError::message(
        ErrorCode::ArithmeticOverflow,
        RecoveryClass::CorrectRequest,
        "streamed artifact byte count overflowed",
    )
}
