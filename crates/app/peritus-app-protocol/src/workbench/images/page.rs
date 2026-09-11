//! Bounded, revision-fenced inspection of retained image references and current eligibility.

use super::{
    AppProtocolError, WorkbenchImageLabel, WorkbenchImageMetadata, WorkbenchQuery, invalid,
};
use crate::{ControlOperationId, WorkbenchInputRow, WorkbenchInputState};
use peritus_types::ArtifactId;

/// Maximum attachment metadata rows in one page.
pub const MAX_WORKBENCH_IMAGE_PAGE: usize = 32;

/// Exact scope and page selection. Revision zero requests the latest first page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchImageQuery {
    query: WorkbenchQuery,
    revision: u64,
    offset: u32,
}
impl WorkbenchImageQuery {
    /// Checks pagination bounds and rejects an unfenced continuation page.
    ///
    /// # Errors
    /// Rejects offsets beyond the retained-image ceiling or a nonzero offset at revision zero.
    pub const fn new(
        query: WorkbenchQuery,
        revision: u64,
        offset: u32,
    ) -> Result<Self, AppProtocolError> {
        if offset > 256 || (revision == 0 && offset != 0) {
            return Err(invalid());
        }
        Ok(Self { query, revision, offset })
    }
    /// Returns owning conversation/workspace.
    #[must_use]
    pub const fn query(self) -> WorkbenchQuery {
        self.query
    }
    /// Returns inspected revision, or zero for the latest first page.
    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision
    }
    /// Returns zero-based retained-image offset.
    #[must_use]
    pub const fn offset(self) -> u32 {
        self.offset
    }
}

/// Immutable image identity/metadata with its latest caption and selection state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchImageRow {
    operation: ControlOperationId,
    artifact: ArtifactId,
    label: WorkbenchImageLabel,
    image: WorkbenchImageMetadata,
    source: WorkbenchInputRow,
    selected: bool,
    eligible: bool,
}
impl WorkbenchImageRow {
    /// Validates one current projection; eligibility never means provider delivery.
    ///
    /// # Errors
    /// Rejects superseded current captions and eligibility for deselected/held/withdrawn sources.
    pub fn new(
        operation: ControlOperationId,
        artifact: ArtifactId,
        label: WorkbenchImageLabel,
        image: WorkbenchImageMetadata,
        source: WorkbenchInputRow,
        selection: (bool, bool),
    ) -> Result<Self, AppProtocolError> {
        if source.state() == WorkbenchInputState::Superseded
            || (selection.1
                && (!selection.0
                    || !matches!(
                        source.state(),
                        WorkbenchInputState::Queued | WorkbenchInputState::Incorporated
                    )))
        {
            return Err(invalid());
        }
        Ok(Self {
            operation,
            artifact,
            label,
            image,
            source,
            selected: selection.0,
            eligible: selection.1,
        })
    }
    /// Returns original import operation identity.
    #[must_use]
    pub const fn operation(&self) -> ControlOperationId {
        self.operation
    }
    /// Returns original immutable artifact identity.
    #[must_use]
    pub const fn artifact(&self) -> ArtifactId {
        self.artifact
    }
    /// Borrows original inert source label.
    #[must_use]
    pub const fn label(&self) -> &WorkbenchImageLabel {
        &self.label
    }
    /// Returns exact decoded original-image metadata.
    #[must_use]
    pub const fn image(&self) -> WorkbenchImageMetadata {
        self.image
    }
    /// Borrows the latest revision of the caption/instruction.
    #[must_use]
    pub const fn source(&self) -> &WorkbenchInputRow {
        &self.source
    }
    /// Returns the user's future-selection preference.
    #[must_use]
    pub const fn selected(&self) -> bool {
        self.selected
    }
    /// Returns whether this image is eligible with the current exact input capture.
    #[must_use]
    pub const fn eligible(&self) -> bool {
        self.eligible
    }
}

/// Complete bounded page at a committed conversation revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchImagePage {
    query: WorkbenchImageQuery,
    total: u32,
    rows: Vec<WorkbenchImageRow>,
}
impl WorkbenchImagePage {
    /// Checks exact page size, revision, and unique attachment identities.
    ///
    /// # Errors
    /// Rejects an unfenced, oversized, duplicate, missing, or surplus page.
    pub fn new(
        query: WorkbenchImageQuery,
        total: u32,
        rows: Vec<WorkbenchImageRow>,
    ) -> Result<Self, AppProtocolError> {
        if query.revision() == 0
            || total > 256
            || query.offset() > total
            || rows.len()
                != usize::try_from(total - query.offset())
                    .map_err(|_| invalid())?
                    .min(MAX_WORKBENCH_IMAGE_PAGE)
            || rows.iter().enumerate().any(|(index, row)| {
                rows[..index].iter().any(|prior| prior.operation() == row.operation())
            })
        {
            return Err(invalid());
        }
        Ok(Self { query, total, rows })
    }
    /// Returns exact page scope and committed revision.
    #[must_use]
    pub const fn query(&self) -> WorkbenchImageQuery {
        self.query
    }
    /// Returns all retained image references, including excluded ones.
    #[must_use]
    pub const fn total(&self) -> u32 {
        self.total
    }
    /// Borrows this exact page's metadata rows.
    #[must_use]
    pub fn rows(&self) -> &[WorkbenchImageRow] {
        &self.rows
    }
}
