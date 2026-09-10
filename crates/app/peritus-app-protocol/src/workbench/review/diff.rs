//! Bounded inert structured-diff files, hunks, and lines.

use super::{
    MAX_WORKBENCH_DIFF_HUNKS, MAX_WORKBENCH_DIFF_LINES, WorkbenchReviewAnchor,
    WorkbenchReviewTarget, malformed,
};
use crate::AppProtocolError;

/// Unified-diff line classification retained for structured rendering.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkbenchDiffLineKind {
    /// Unchanged context.
    Context,
    /// Old-image removal.
    Removed,
    /// New-image addition.
    Added,
    /// No-newline marker or other inert hunk metadata.
    Metadata,
}

impl WorkbenchDiffLineKind {
    /// Stable canonical tag.
    #[must_use]
    pub const fn tag(self) -> u16 {
        match self {
            Self::Context => 1,
            Self::Removed => 2,
            Self::Added => 3,
            Self::Metadata => 4,
        }
    }
    /// Decodes a stable tag.
    #[must_use]
    pub const fn from_tag(tag: u16) -> Option<Self> {
        match tag {
            1 => Some(Self::Context),
            2 => Some(Self::Removed),
            3 => Some(Self::Added),
            4 => Some(Self::Metadata),
            _ => None,
        }
    }
}

/// One bounded inert structured-diff line.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchDiffLine {
    kind: WorkbenchDiffLineKind,
    text: String,
}

impl WorkbenchDiffLine {
    /// Creates a line after removing its unified-diff prefix.
    ///
    /// # Errors
    /// Rejects terminal controls or a line above the app field bound.
    pub fn new(kind: WorkbenchDiffLineKind, text: String) -> Result<Self, AppProtocolError> {
        if text.len() > crate::MAX_PRODUCT_DETAIL_BYTES
            || text.chars().any(|character| character.is_control() && character != '\t')
        {
            return Err(malformed());
        }
        Ok(Self { kind, text })
    }
    /// Display classification.
    #[must_use]
    pub const fn kind(&self) -> WorkbenchDiffLineKind {
        self.kind
    }
    /// Inert source text without the diff prefix.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// One exact parsed hunk and its visible lines.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchDiffHunk {
    anchor: WorkbenchReviewAnchor,
    header: String,
    lines: Vec<WorkbenchDiffLine>,
}

impl WorkbenchDiffHunk {
    /// Checked construction used by the wire decoder and parser.
    pub(crate) fn new(
        anchor: WorkbenchReviewAnchor,
        header: String,
        lines: Vec<WorkbenchDiffLine>,
    ) -> Result<Self, AppProtocolError> {
        if anchor.target() != WorkbenchReviewTarget::Hunk
            || header.is_empty()
            || header.len() > crate::MAX_PRODUCT_DETAIL_BYTES
            || lines.is_empty()
            || lines.len() > MAX_WORKBENCH_DIFF_LINES
        {
            return Err(malformed());
        }
        Ok(Self { anchor, header, lines })
    }
    /// Exact feedback anchor.
    #[must_use]
    pub const fn anchor(&self) -> &WorkbenchReviewAnchor {
        &self.anchor
    }
    /// Unified-diff header.
    #[must_use]
    pub fn header(&self) -> &str {
        &self.header
    }
    /// Visible bounded hunk lines.
    #[must_use]
    pub fn lines(&self) -> &[WorkbenchDiffLine] {
        &self.lines
    }
}

/// One exact changed file and its parsed hunks.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchDiffFile {
    anchor: WorkbenchReviewAnchor,
    hunks: Vec<WorkbenchDiffHunk>,
}

impl WorkbenchDiffFile {
    /// Checked construction used by the wire decoder and parser.
    pub(crate) fn new(
        anchor: WorkbenchReviewAnchor,
        hunks: Vec<WorkbenchDiffHunk>,
    ) -> Result<Self, AppProtocolError> {
        if anchor.target() != WorkbenchReviewTarget::File
            || hunks.len() > MAX_WORKBENCH_DIFF_HUNKS
            || hunks.iter().any(|hunk| hunk.anchor().path() != anchor.path())
        {
            return Err(malformed());
        }
        Ok(Self { anchor, hunks })
    }
    /// Complete-file feedback anchor.
    #[must_use]
    pub const fn anchor(&self) -> &WorkbenchReviewAnchor {
        &self.anchor
    }
    /// Parsed hunks in raw-diff order.
    #[must_use]
    pub fn hunks(&self) -> &[WorkbenchDiffHunk] {
        &self.hunks
    }
}
