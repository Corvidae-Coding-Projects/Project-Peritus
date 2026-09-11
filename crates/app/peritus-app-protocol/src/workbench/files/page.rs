//! Revision-fenced file selection pages; all historical bytes remain in immutable archives.

use super::{WorkbenchFileMetadata, WorkbenchFileMode, invalid};
use crate::{AppProtocolError, ControlOperationId, WorkbenchQuery};

/// Bounded selected-workspace file reference inspection request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchFileQuery {
    query: WorkbenchQuery,
    revision: u64,
    offset: u32,
}
impl WorkbenchFileQuery {
    /// Creates a revision-fenced page request; zero revision is invalid.
    ///
    /// # Errors
    /// Rejects impossible revision or offset.
    pub const fn new(
        query: WorkbenchQuery,
        revision: u64,
        offset: u32,
    ) -> Result<Self, AppProtocolError> {
        if revision == 0 || offset > 256 {
            return Err(invalid());
        }
        Ok(Self { query, revision, offset })
    }
    /// Returns exact owning scope.
    #[must_use]
    pub const fn query(self) -> WorkbenchQuery {
        self.query
    }
    /// Returns inspected aggregate revision.
    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision
    }
    /// Returns zero-based retained-reference offset.
    #[must_use]
    pub const fn offset(self) -> u32 {
        self.offset
    }
}
/// Exact current version and explicit inclusion status of a retained file reference.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchFileRow {
    attachment: ControlOperationId,
    version: ControlOperationId,
    label: String,
    mode: WorkbenchFileMode,
    file: WorkbenchFileMetadata,
    selected: bool,
    eligible: bool,
}
impl WorkbenchFileRow {
    /// Bounds inert labels; eligibility must imply explicit selection.
    ///
    /// # Errors
    /// Rejects invalid label or inconsistent selection status.
    pub fn new(
        attachment: ControlOperationId,
        version: ControlOperationId,
        label: String,
        mode: WorkbenchFileMode,
        file: WorkbenchFileMetadata,
        selected: bool,
        eligible: bool,
    ) -> Result<Self, AppProtocolError> {
        if label.is_empty()
            || label.len() > 4096
            || label.chars().any(char::is_control)
            || (eligible && !selected)
        {
            return Err(invalid());
        }
        Ok(Self { attachment, version, label, mode, file, selected, eligible })
    }
    /// Returns original reference operation.
    #[must_use]
    pub const fn attachment(&self) -> ControlOperationId {
        self.attachment
    }
    /// Returns current immutable version operation.
    #[must_use]
    pub const fn version(&self) -> ControlOperationId {
        self.version
    }
    /// Borrows explicit source label.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }
    /// Returns user-confirmed source refresh mode.
    #[must_use]
    pub const fn mode(&self) -> WorkbenchFileMode {
        self.mode
    }
    /// Returns selected immutable observation.
    #[must_use]
    pub const fn file(&self) -> WorkbenchFileMetadata {
        self.file
    }
    /// Reports explicit selection preference.
    #[must_use]
    pub const fn selected(&self) -> bool {
        self.selected
    }
    /// Reports current queue eligibility, not confirmed provider delivery.
    #[must_use]
    pub const fn eligible(&self) -> bool {
        self.eligible
    }
}
/// At most 32 retained reference rows, scoped to one exact aggregate revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchFilePage {
    query: WorkbenchFileQuery,
    rows: Vec<WorkbenchFileRow>,
    total: u32,
}
impl WorkbenchFilePage {
    /// Validates complete page shape without accepting omitted rows or hidden truncation.
    ///
    /// # Errors
    /// Rejects inconsistent total, offset or row count.
    pub fn new(
        query: WorkbenchFileQuery,
        rows: Vec<WorkbenchFileRow>,
        total: u32,
    ) -> Result<Self, AppProtocolError> {
        if total > 256
            || query.offset() > total
            || rows.len() != (total - query.offset()).min(32) as usize
        {
            return Err(invalid());
        }
        Ok(Self { query, rows, total })
    }
    /// Returns exact page and revision binding.
    #[must_use]
    pub const fn query(&self) -> WorkbenchFileQuery {
        self.query
    }
    /// Borrows retained references in publication order.
    #[must_use]
    pub fn rows(&self) -> &[WorkbenchFileRow] {
        &self.rows
    }
    /// Returns complete retained-reference count.
    #[must_use]
    pub const fn total(&self) -> u32 {
        self.total
    }
}
