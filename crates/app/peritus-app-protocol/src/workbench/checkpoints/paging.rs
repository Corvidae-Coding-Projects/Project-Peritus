//! Explicit checkpoint coverage cursors and bounded pages.

use super::{
    WorkbenchCheckpointName, WorkbenchCheckpointPath, WorkbenchCheckpointReceipt,
    WorkbenchCheckpointReferences, WorkbenchQuery, WorkbenchRewindRequest, invalid,
};
use crate::{AppProtocolError, ControlOperationId};
use peritus_types::Sha256Digest;

#[path = "rewind_page.rs"]
mod rewind_page;
pub use rewind_page::WorkbenchRewindCoveragePage;

/// Ordered section in a complete checkpoint coverage stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkbenchCoverageSection {
    /// Canonically sorted covered paths and their retained preimages.
    Paths,
    /// Explicitly excluded paths and reasons.
    Exclusions,
    /// Explicitly listed non-restorable external effects.
    ExternalEffects,
}

impl WorkbenchCoverageSection {
    const fn order(self) -> u8 {
        match self {
            Self::Paths => 1,
            Self::Exclusions => 2,
            Self::ExternalEffects => 3,
        }
    }
}

/// Stateless continuation bound to one exact full-coverage fingerprint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchCoverageCursor {
    section: WorkbenchCoverageSection,
    offset: u64,
    fingerprint: Sha256Digest,
}
impl WorkbenchCoverageCursor {
    /// Constructs an exact section position for the supplied full-coverage fingerprint.
    #[must_use]
    pub const fn new(
        section: WorkbenchCoverageSection,
        offset: u64,
        fingerprint: Sha256Digest,
    ) -> Self {
        Self { section, offset, fingerprint }
    }
    /// Returns the section to continue.
    #[must_use]
    pub const fn section(self) -> WorkbenchCoverageSection {
        self.section
    }
    /// Returns the zero-based entry offset within the section.
    #[must_use]
    pub const fn offset(self) -> u64 {
        self.offset
    }
    /// Returns the fingerprint that binds this cursor to all checkpoint facts.
    #[must_use]
    pub const fn fingerprint(self) -> Sha256Digest {
        self.fingerprint
    }
}

/// Read-only request for one bounded page of a stored checkpoint manifest.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchCheckpointPageRequest {
    query: WorkbenchQuery,
    revision: u64,
    checkpoint: ControlOperationId,
    cursor: Option<WorkbenchCoverageCursor>,
}
impl WorkbenchCheckpointPageRequest {
    /// Constructs a revision-fenced checkpoint page request.
    ///
    /// # Errors
    /// Rejects revision zero.
    pub const fn new(
        query: WorkbenchQuery,
        revision: u64,
        checkpoint: ControlOperationId,
        cursor: Option<WorkbenchCoverageCursor>,
    ) -> Result<Self, AppProtocolError> {
        if revision == 0 {
            return Err(invalid());
        }
        Ok(Self { query, revision, checkpoint, cursor })
    }
    /// Returns the exact conversation and workspace scope.
    #[must_use]
    pub const fn query(self) -> WorkbenchQuery {
        self.query
    }
    /// Returns the selected current conversation revision.
    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision
    }
    /// Returns the selected durable checkpoint identity.
    #[must_use]
    pub const fn checkpoint(self) -> ControlOperationId {
        self.checkpoint
    }
    /// Returns the continuation when this is not the first page.
    #[must_use]
    pub const fn cursor(self) -> Option<WorkbenchCoverageCursor> {
        self.cursor
    }
}

/// One transfer page from the complete checkpoint manifest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchCheckpointCoveragePage {
    checkpoint: ControlOperationId,
    query: WorkbenchQuery,
    selected_revision: u64,
    accepted_revision: u64,
    name: WorkbenchCheckpointName,
    references: WorkbenchCheckpointReferences,
    fingerprint: Sha256Digest,
    total_paths: u64,
    total_exclusions: u64,
    total_external_effects: u64,
    section: WorkbenchCoverageSection,
    offset: u64,
    paths: Vec<WorkbenchCheckpointPath>,
    exclusions: Vec<String>,
    external_effects: Vec<String>,
    next: Option<WorkbenchCoverageCursor>,
}
impl WorkbenchCheckpointCoveragePage {
    /// Constructs one internally consistent subset of an explicitly counted checkpoint.
    ///
    /// # Errors
    /// Rejects an empty or mixed section, invalid offset, or misbound continuation.
    #[allow(
        clippy::too_many_arguments,
        reason = "page identity and exact continuation remain explicit"
    )]
    pub fn new(
        receipt: &WorkbenchCheckpointReceipt,
        selected_revision: u64,
        fingerprint: Sha256Digest,
        total_paths: u64,
        total_exclusions: u64,
        total_external_effects: u64,
        section: WorkbenchCoverageSection,
        offset: u64,
        paths: Vec<WorkbenchCheckpointPath>,
        exclusions: Vec<String>,
        external_effects: Vec<String>,
        next: Option<WorkbenchCoverageCursor>,
    ) -> Result<Self, AppProtocolError> {
        let totals = [total_paths, total_exclusions, total_external_effects];
        let empty_terminal = totals == [0; 3]
            && section == WorkbenchCoverageSection::Paths
            && offset == 0
            && paths.is_empty()
            && exclusions.is_empty()
            && external_effects.is_empty()
            && next.is_none();
        let page_len = match section {
            WorkbenchCoverageSection::Paths => {
                if (!empty_terminal && paths.is_empty())
                    || !exclusions.is_empty()
                    || !external_effects.is_empty()
                {
                    return Err(invalid());
                }
                paths.len()
            }
            WorkbenchCoverageSection::Exclusions => {
                if !paths.is_empty() || exclusions.is_empty() || !external_effects.is_empty() {
                    return Err(invalid());
                }
                exclusions.len()
            }
            WorkbenchCoverageSection::ExternalEffects => {
                if !paths.is_empty() || !exclusions.is_empty() || external_effects.is_empty() {
                    return Err(invalid());
                }
                external_effects.len()
            }
        };
        let total = section_total(section, total_paths, total_exclusions, total_external_effects);
        let end = offset
            .checked_add(u64::try_from(page_len).map_err(|_| invalid())?)
            .ok_or_else(invalid)?;
        if selected_revision == 0
            || selected_revision < receipt.accepted_revision()
            || end > total
            || paths.windows(2).any(|pair| pair[0].path() >= pair[1].path())
            || exclusions.iter().any(|value| !valid_page_text(value, 4_610))
            || external_effects.iter().any(|value| !valid_page_text(value, 512))
            || !valid_next(
                section,
                end,
                [total_paths, total_exclusions, total_external_effects],
                fingerprint,
                next,
            )
        {
            return Err(invalid());
        }
        Ok(Self {
            checkpoint: receipt.checkpoint(),
            query: receipt.query(),
            selected_revision,
            accepted_revision: receipt.accepted_revision(),
            name: receipt.name().clone(),
            references: receipt.references(),
            fingerprint,
            total_paths,
            total_exclusions,
            total_external_effects,
            section,
            offset,
            paths,
            exclusions,
            external_effects,
            next,
        })
    }
    /// Returns checkpoint identity.
    #[must_use]
    pub const fn checkpoint(&self) -> ControlOperationId {
        self.checkpoint
    }
    /// Returns original conversation and workspace scope.
    #[must_use]
    pub const fn query(&self) -> WorkbenchQuery {
        self.query
    }
    /// Returns checkpoint acceptance revision.
    #[must_use]
    pub const fn accepted_revision(&self) -> u64 {
        self.accepted_revision
    }
    /// Returns the conversation revision at which this page was requested.
    #[must_use]
    pub const fn selected_revision(&self) -> u64 {
        self.selected_revision
    }
    /// Borrows the checkpoint name.
    #[must_use]
    pub const fn name(&self) -> &WorkbenchCheckpointName {
        &self.name
    }
    /// Returns retained historical references.
    #[must_use]
    pub const fn references(&self) -> WorkbenchCheckpointReferences {
        self.references
    }
    /// Returns the fingerprint over all retained facts, independent of page boundaries.
    #[must_use]
    pub const fn fingerprint(&self) -> Sha256Digest {
        self.fingerprint
    }
    /// Returns the complete covered-path count.
    #[must_use]
    pub const fn total_paths(&self) -> u64 {
        self.total_paths
    }
    /// Returns the complete exclusion count.
    #[must_use]
    pub const fn total_exclusions(&self) -> u64 {
        self.total_exclusions
    }
    /// Returns the complete external-effect count.
    #[must_use]
    pub const fn total_external_effects(&self) -> u64 {
        self.total_external_effects
    }
    /// Returns the section carried by this page.
    #[must_use]
    pub const fn section(&self) -> WorkbenchCoverageSection {
        self.section
    }
    /// Returns the zero-based section offset.
    #[must_use]
    pub const fn offset(&self) -> u64 {
        self.offset
    }
    /// Borrows covered paths on this page.
    #[must_use]
    pub fn paths(&self) -> &[WorkbenchCheckpointPath] {
        &self.paths
    }
    /// Borrows exclusions on this page.
    #[must_use]
    pub fn exclusions(&self) -> &[String] {
        &self.exclusions
    }
    /// Borrows external effects on this page.
    #[must_use]
    pub fn external_effects(&self) -> &[String] {
        &self.external_effects
    }
    /// Returns the exact continuation, if coverage remains.
    #[must_use]
    pub const fn next(&self) -> Option<WorkbenchCoverageCursor> {
        self.next
    }
}

/// Read-only request for one bounded page of a rewind preview.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchRewindPageRequest {
    request: WorkbenchRewindRequest,
    cursor: Option<WorkbenchCoverageCursor>,
}
impl WorkbenchRewindPageRequest {
    /// Constructs one page request bound to the exact rewind selection.
    #[must_use]
    pub const fn new(
        request: WorkbenchRewindRequest,
        cursor: Option<WorkbenchCoverageCursor>,
    ) -> Self {
        Self { request, cursor }
    }
    /// Returns the exact revision-fenced rewind selection.
    #[must_use]
    pub const fn request(self) -> WorkbenchRewindRequest {
        self.request
    }
    /// Returns the continuation when this is not the first page.
    #[must_use]
    pub const fn cursor(self) -> Option<WorkbenchCoverageCursor> {
        self.cursor
    }
}

const fn section_total(
    section: WorkbenchCoverageSection,
    paths: u64,
    exclusions: u64,
    effects: u64,
) -> u64 {
    match section {
        WorkbenchCoverageSection::Paths => paths,
        WorkbenchCoverageSection::Exclusions => exclusions,
        WorkbenchCoverageSection::ExternalEffects => effects,
    }
}

fn valid_next(
    current: WorkbenchCoverageSection,
    end: u64,
    totals: [u64; 3],
    fingerprint: Sha256Digest,
    next: Option<WorkbenchCoverageCursor>,
) -> bool {
    let current_index = usize::from(current.order() - 1);
    let expected = if end < totals[current_index] {
        Some((current, end))
    } else {
        (current_index + 1..totals.len())
            .find(|index| totals[*index] > 0)
            .map(|index| (section_at(index), 0))
    };
    match (expected, next) {
        (None, None) => true,
        (Some((section, offset)), Some(cursor)) => {
            cursor.fingerprint() == fingerprint
                && cursor.section() == section
                && cursor.offset() == offset
        }
        _ => false,
    }
}

const fn section_at(index: usize) -> WorkbenchCoverageSection {
    match index {
        0 => WorkbenchCoverageSection::Paths,
        1 => WorkbenchCoverageSection::Exclusions,
        _ => WorkbenchCoverageSection::ExternalEffects,
    }
}

fn valid_page_text(value: &str, maximum: usize) -> bool {
    !value.is_empty() && value.len() <= maximum && !value.chars().any(char::is_control)
}
