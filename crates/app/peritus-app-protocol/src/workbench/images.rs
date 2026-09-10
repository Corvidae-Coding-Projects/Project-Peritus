//! Explicit image upload and preview contracts; none imply provider delivery or path authority.

use crate::{AppErrorCode, AppProtocolError, ArtifactMetadata, ProductModelChoice, WorkbenchQuery};
use peritus_types::{ArtifactId, ProviderProfileId};

mod metadata;
pub use metadata::{WorkbenchImageFormat, WorkbenchImageMetadata};
mod preview;
pub use preview::WorkbenchImagePreview;
mod page;
pub use page::{
    MAX_WORKBENCH_IMAGE_PAGE, WorkbenchImagePage, WorkbenchImageQuery, WorkbenchImageRow,
};
#[cfg(test)]
mod tests;

/// Maximum original encoded image size accepted by this protocol surface.
pub const MAX_WORKBENCH_IMAGE_BYTES: u64 = 4 * 1024 * 1024;
/// Maximum UTF-8 source label bytes. The label is not a filesystem capability.
pub const MAX_WORKBENCH_IMAGE_LABEL_BYTES: usize = 1024;

const fn invalid() -> AppProtocolError {
    AppProtocolError::new(AppErrorCode::MalformedFrame, None)
}

/// Explicit inert source label, redacted in debug output.
#[derive(Clone, Eq, PartialEq)]
pub struct WorkbenchImageLabel(String);
impl WorkbenchImageLabel {
    /// Validates exact user-selected source text.
    ///
    /// # Errors
    /// Rejects empty, oversized, or terminal-control-containing labels.
    pub fn new(value: String) -> Result<Self, AppProtocolError> {
        if value.trim().is_empty()
            || value.len() > MAX_WORKBENCH_IMAGE_LABEL_BYTES
            || value.chars().any(char::is_control)
        {
            return Err(invalid());
        }
        Ok(Self(value))
    }
    /// Borrows exact inert source text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Debug for WorkbenchImageLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkbenchImageLabel").field("bytes", &self.0.len()).finish_non_exhaustive()
    }
}

/// Declares an upload owned by an existing, revision-fenced conversation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchImageUpload {
    query: WorkbenchQuery,
    revision: u64,
    metadata: ArtifactMetadata,
}
impl WorkbenchImageUpload {
    /// Bounds original bytes before transfer. MIME and byte decoding are checked by preview.
    ///
    /// # Errors
    /// Rejects an absent conversation revision or zero/excessive image size.
    pub fn new(
        query: WorkbenchQuery,
        revision: u64,
        metadata: ArtifactMetadata,
    ) -> Result<Self, AppProtocolError> {
        if revision == 0
            || metadata.byte_size() == 0
            || metadata.byte_size() > MAX_WORKBENCH_IMAGE_BYTES
        {
            return Err(invalid());
        }
        Ok(Self { query, revision, metadata })
    }
    /// Returns exact owning scope.
    #[must_use]
    pub const fn query(&self) -> WorkbenchQuery {
        self.query
    }
    /// Returns inspected conversation revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
    /// Borrows immutable original transfer metadata.
    #[must_use]
    pub const fn metadata(&self) -> &ArtifactMetadata {
        &self.metadata
    }
}

/// Selects exact uploaded bytes and a configured provider for local validation and preview.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchImageRequest {
    query: WorkbenchQuery,
    revision: u64,
    artifact: ArtifactId,
    provider: ProviderProfileId,
    model: ProductModelChoice,
    label: WorkbenchImageLabel,
}
impl WorkbenchImageRequest {
    /// Binds explicit scope and selection without granting file access or running inference.
    ///
    /// # Errors
    /// Rejects an absent conversation revision.
    pub fn new(
        query: WorkbenchQuery,
        revision: u64,
        artifact: ArtifactId,
        provider: ProviderProfileId,
        model: ProductModelChoice,
        label: WorkbenchImageLabel,
    ) -> Result<Self, AppProtocolError> {
        if revision == 0 {
            return Err(invalid());
        }
        Ok(Self { query, revision, artifact, provider, model, label })
    }
    /// Returns exact owning scope.
    #[must_use]
    pub const fn query(&self) -> WorkbenchQuery {
        self.query
    }
    /// Returns inspected conversation revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
    /// Returns original uploaded artifact identity.
    #[must_use]
    pub const fn artifact(&self) -> ArtifactId {
        self.artifact
    }
    /// Returns explicit configured provider route, never an automatic fallback.
    #[must_use]
    pub const fn provider(&self) -> ProviderProfileId {
        self.provider
    }
    /// Borrows exact model selection used for capability validation.
    #[must_use]
    pub const fn model(&self) -> &ProductModelChoice {
        &self.model
    }
    /// Borrows the source label, not a path grant.
    #[must_use]
    pub const fn label(&self) -> &WorkbenchImageLabel {
        &self.label
    }
}
