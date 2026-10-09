//! Explicit client-imported snapshots. Source labels are never reopened by the daemon.
use super::{
    MAX_WORKBENCH_FILE_BYTES, WorkbenchFileMetadata, WorkbenchFileMode, WorkbenchFileRequest,
    invalid,
};
use crate::{AppProtocolError, ArtifactMetadata, WorkbenchQuery};
use peritus_types::ArtifactId;

/// Scoped transfer of only the selected text bytes to the local daemon.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchFileUpload {
    query: WorkbenchQuery,
    revision: u64,
    metadata: ArtifactMetadata,
}
impl WorkbenchFileUpload {
    /// Validates ownership revision and the text transfer ceiling, including empty text.
    ///
    /// # Errors
    /// Rejects an absent revision or an oversized selection.
    pub fn new(
        query: WorkbenchQuery,
        revision: u64,
        metadata: ArtifactMetadata,
    ) -> Result<Self, AppProtocolError> {
        if revision == 0 || metadata.byte_size() > MAX_WORKBENCH_FILE_BYTES {
            return Err(invalid());
        }
        Ok(Self { query, revision, metadata })
    }
    /// Returns the owning conversation/workspace.
    #[must_use]
    pub const fn query(&self) -> WorkbenchQuery {
        self.query
    }
    /// Returns the inspected revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
    /// Borrows exact transfer metadata.
    #[must_use]
    pub const fn metadata(&self) -> &ArtifactMetadata {
        &self.metadata
    }
}

/// Client-observed source metadata plus exact transferred bytes. Not a path authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchFileImportRequest {
    selection: WorkbenchFileRequest,
    artifact: ArtifactId,
    file: WorkbenchFileMetadata,
}
impl WorkbenchFileImportRequest {
    /// Binds the explicitly chosen range to an uploaded snapshot.
    ///
    /// # Errors
    /// Rejects refresh mode, invalid labels or inconsistent selected ranges.
    pub fn new(
        selection: WorkbenchFileRequest,
        artifact: ArtifactId,
        file: WorkbenchFileMetadata,
    ) -> Result<Self, AppProtocolError> {
        if selection.mode() != WorkbenchFileMode::Snapshot
            || selection.path().len() > 1024
            || !file.matches(selection.range())
        {
            return Err(invalid());
        }
        Ok(Self { selection, artifact, file })
    }
    /// Borrows the range/provider and inert label (not an external path).
    #[must_use]
    pub const fn selection(&self) -> &WorkbenchFileRequest {
        &self.selection
    }
    /// Returns the scoped immutable transfer artifact.
    #[must_use]
    pub const fn artifact(&self) -> ArtifactId {
        self.artifact
    }
    /// Returns client-observed source metadata; the host independently validates selected bytes.
    #[must_use]
    pub const fn file(&self) -> WorkbenchFileMetadata {
        self.file
    }
}

/// Preview of a validated immutable import, bound to the selected provider revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchFileImportPreview {
    request: WorkbenchFileImportRequest,
    provider_revision: u64,
    resolved_model: String,
}
impl WorkbenchFileImportPreview {
    /// Validates the configured provider/model observation.
    ///
    /// # Errors
    /// Rejects an absent revision or malformed model identifier.
    pub fn new(
        request: WorkbenchFileImportRequest,
        provider_revision: u64,
        resolved_model: String,
    ) -> Result<Self, AppProtocolError> {
        if provider_revision == 0
            || resolved_model.trim().is_empty()
            || resolved_model.len() > 512
            || resolved_model.chars().any(char::is_control)
        {
            return Err(invalid());
        }
        Ok(Self { request, provider_revision, resolved_model })
    }
    /// Borrows the exact import selection and source provenance.
    #[must_use]
    pub const fn request(&self) -> &WorkbenchFileImportRequest {
        &self.request
    }
    /// Returns the observed provider configuration revision.
    #[must_use]
    pub const fn provider_revision(&self) -> u64 {
        self.provider_revision
    }
    /// Borrows the resolved model identifier.
    #[must_use]
    pub fn resolved_model(&self) -> &str {
        &self.resolved_model
    }
}
