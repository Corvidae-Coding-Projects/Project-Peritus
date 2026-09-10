//! Content-free exact observation and provider-bound confirmation preview.

use super::{MAX_WORKBENCH_FILE_BYTES, WorkbenchFileRequest, invalid};
use crate::AppProtocolError;
use peritus_types::Sha256Digest;

/// Source and selected-byte identities observed by the host, not supplied as a read grant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchFileMetadata {
    source_digest: Sha256Digest,
    source_bytes: u64,
    range: (u64, u64),
    digest: Sha256Digest,
}
impl WorkbenchFileMetadata {
    /// Validates complete-source and selected-byte bounds.
    ///
    /// # Errors
    /// Rejects impossible intervals and inconsistent whole-source identities.
    pub const fn new(
        source_digest: Sha256Digest,
        source_bytes: u64,
        range: (u64, u64),
        digest: Sha256Digest,
    ) -> Result<Self, AppProtocolError> {
        if source_bytes > 64 * 1024 * 1024
            || range.0 > range.1
            || range.1 > source_bytes
            || range.1 - range.0 > MAX_WORKBENCH_FILE_BYTES
        {
            return Err(invalid());
        }
        Ok(Self { source_digest, source_bytes, range, digest })
    }
    /// Returns complete-source SHA-256.
    #[must_use]
    pub const fn source_digest(self) -> Sha256Digest {
        self.source_digest
    }
    /// Returns complete observed source size.
    #[must_use]
    pub const fn source_bytes(self) -> u64 {
        self.source_bytes
    }
    /// Returns exact resolved half-open byte range.
    #[must_use]
    pub const fn range(self) -> (u64, u64) {
        self.range
    }
    /// Returns selected-byte SHA-256.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.digest
    }
    /// Returns exact selected size, including original line terminators.
    #[must_use]
    pub const fn bytes(self) -> u64 {
        self.range.1 - self.range.0
    }
    pub(super) fn matches(self, range: super::WorkbenchFileRange) -> bool {
        match range {
            super::WorkbenchFileRange::All => {
                self.range() == (0, self.source_bytes()) && self.digest() == self.source_digest()
            }
            super::WorkbenchFileRange::Bytes { start, end } => self.range() == (start, end),
            super::WorkbenchFileRange::Lines { .. } => self.bytes() > 0,
        }
    }
}

/// Exact confirmation binding. Previewing does not attach bytes or start inference.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchFilePreview {
    request: WorkbenchFileRequest,
    folder: Sha256Digest,
    file: WorkbenchFileMetadata,
    provider_revision: u64,
    resolved_model: String,
}
impl WorkbenchFilePreview {
    /// Binds source identity and selected provider state for explicit user confirmation.
    ///
    /// # Errors
    /// Rejects invalid provider revision/model or a resolved range inconsistent with the request.
    pub fn new(
        request: WorkbenchFileRequest,
        folder: Sha256Digest,
        file: WorkbenchFileMetadata,
        provider_revision: u64,
        resolved_model: String,
    ) -> Result<Self, AppProtocolError> {
        if !file.matches(request.range())
            || provider_revision == 0
            || resolved_model.trim().is_empty()
            || resolved_model.len() > 512
            || resolved_model.chars().any(char::is_control)
        {
            return Err(invalid());
        }
        Ok(Self { request, folder, file, provider_revision, resolved_model })
    }
    /// Borrows the exact confirmed source selection.
    #[must_use]
    pub const fn request(&self) -> &WorkbenchFileRequest {
        &self.request
    }
    /// Returns the observed selected-folder identity.
    #[must_use]
    pub const fn folder(&self) -> Sha256Digest {
        self.folder
    }
    /// Returns exact source and selected-byte identities.
    #[must_use]
    pub const fn file(&self) -> WorkbenchFileMetadata {
        self.file
    }
    /// Returns the validated provider configuration revision.
    #[must_use]
    pub const fn provider_revision(&self) -> u64 {
        self.provider_revision
    }
    /// Borrows the exact resolved model identifier.
    #[must_use]
    pub fn resolved_model(&self) -> &str {
        &self.resolved_model
    }
}
