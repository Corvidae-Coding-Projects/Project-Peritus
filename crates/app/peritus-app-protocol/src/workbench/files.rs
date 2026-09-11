//! Explicit workspace-file preview and selection. DTOs never confer read authority.

use crate::{AppErrorCode, AppProtocolError, ProductModelChoice, WorkbenchQuery};
use peritus_types::ProviderProfileId;

mod import;
mod preview;
pub use import::{WorkbenchFileImportPreview, WorkbenchFileImportRequest, WorkbenchFileUpload};
#[cfg(test)]
mod tests;
pub use preview::{WorkbenchFileMetadata, WorkbenchFilePreview};
mod page;
pub use page::{WorkbenchFilePage, WorkbenchFileQuery, WorkbenchFileRow};

/// Maximum selected UTF-8 file bytes; larger sources require an explicit range.
pub const MAX_WORKBENCH_FILE_BYTES: u64 = 256 * 1024;

pub(super) const fn invalid() -> AppProtocolError {
    AppProtocolError::new(AppErrorCode::MalformedFrame, None)
}

/// Exact source selection, resolved to bytes only by the authorized host.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkbenchFileRange {
    /// Complete source under the inclusion ceiling.
    All,
    /// Nonempty half-open byte range.
    Bytes {
        /// First included byte.
        start: u64,
        /// First excluded byte.
        end: u64,
    },
    /// One-based inclusive complete line range.
    Lines {
        /// First included line.
        first: u32,
        /// Last included line.
        last: u32,
    },
}
impl WorkbenchFileRange {
    /// Validates structural bounds, not source existence or authorization.
    ///
    /// # Errors
    /// Rejects empty/reversed or out-of-ceiling ranges.
    pub const fn validate(self) -> Result<(), AppProtocolError> {
        match self {
            Self::All => Ok(()),
            Self::Bytes { start, end } if start < end && end <= 64 * 1024 * 1024 => Ok(()),
            Self::Lines { first, last }
                if first > 0 && first <= last && last <= 64 * 1024 * 1024 =>
            {
                Ok(())
            }
            _ => Err(invalid()),
        }
    }
}

/// Explicit snapshot versus authorized refresh on the next request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkbenchFileMode {
    /// Retain confirmed immutable bytes.
    Snapshot,
    /// Reauthorize the exact workspace source at each request boundary.
    RefreshOnRequest,
}

/// Exact selected source and provider preview binding, never an ambient path capability.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchFileRequest {
    query: WorkbenchQuery,
    revision: u64,
    path: String,
    range: WorkbenchFileRange,
    mode: WorkbenchFileMode,
    provider: ProviderProfileId,
    model: ProductModelChoice,
}
impl WorkbenchFileRequest {
    /// Bounds an explicit source descriptor. The host applies canonical-path and policy checks.
    ///
    /// # Errors
    /// Rejects absent revisions, empty/oversized paths, controls or invalid ranges.
    pub fn new(
        query: WorkbenchQuery,
        revision: u64,
        path: String,
        range: WorkbenchFileRange,
        mode: WorkbenchFileMode,
        provider: ProviderProfileId,
        model: ProductModelChoice,
    ) -> Result<Self, AppProtocolError> {
        range.validate()?;
        if revision == 0
            || path.is_empty()
            || path.len() > 4096
            || path.chars().any(char::is_control)
        {
            return Err(invalid());
        }
        Ok(Self { query, revision, path, range, mode, provider, model })
    }
    /// Returns exact conversation/workspace scope.
    #[must_use]
    pub const fn query(&self) -> WorkbenchQuery {
        self.query
    }
    /// Returns the inspected aggregate revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
    /// Borrows selected relative source text; no path is opened by this DTO.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }
    /// Returns explicit selected range semantics.
    #[must_use]
    pub const fn range(&self) -> WorkbenchFileRange {
        self.range
    }
    /// Returns explicit future-read mode.
    #[must_use]
    pub const fn mode(&self) -> WorkbenchFileMode {
        self.mode
    }
    /// Returns the configured preview provider.
    #[must_use]
    pub const fn provider(&self) -> ProviderProfileId {
        self.provider
    }
    /// Borrows the exact selected model.
    #[must_use]
    pub const fn model(&self) -> &ProductModelChoice {
        &self.model
    }
}
