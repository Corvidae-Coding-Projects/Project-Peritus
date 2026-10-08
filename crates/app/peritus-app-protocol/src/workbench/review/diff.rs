//! Bounded inert structured-diff files, hunks, and lines.

use super::{WorkbenchReviewAnchor, WorkbenchReviewTarget, malformed};
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
    raw_offset: u32,
    raw_length: u32,
    truncated: bool,
}

impl WorkbenchDiffLine {
    pub(crate) fn from_wire(
        kind: WorkbenchDiffLineKind,
        text: String,
        raw_offset: u32,
        raw_length: u32,
        truncated: bool,
    ) -> Result<Self, AppProtocolError> {
        if text.len() > 1025
            || text.chars().any(|ch| ch.is_control() && ch != '\t')
            || raw_offset.checked_add(raw_length).is_none()
            || (truncated && !text.ends_with('…'))
        {
            return Err(malformed());
        }
        Ok(Self { kind, text, raw_offset, raw_length, truncated })
    }

    /// Creates a line after removing its unified-diff prefix.
    ///
    /// # Errors
    /// Rejects terminal controls or a line above the app field bound.
    pub fn new(kind: WorkbenchDiffLineKind, text: &str) -> Result<Self, AppProtocolError> {
        let raw_length = u32::try_from(text.len()).map_err(|_| malformed())?;
        let (text, truncated) = safe_preview(text);
        Ok(Self { kind, text, raw_offset: 0, raw_length, truncated })
    }

    pub(crate) fn from_source(
        kind: WorkbenchDiffLineKind,
        text: &str,
        raw_offset: usize,
    ) -> Result<Self, AppProtocolError> {
        let raw_length = u32::try_from(text.len()).map_err(|_| malformed())?;
        let (text, truncated) = safe_preview(text);
        Ok(Self {
            kind,
            text,
            raw_offset: u32::try_from(raw_offset).map_err(|_| malformed())?,
            raw_length,
            truncated,
        })
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
    /// Byte offset of the exact source text in the retained raw diff.
    #[must_use]
    pub const fn raw_offset(&self) -> u32 {
        self.raw_offset
    }
    /// Exact source-text byte length in the retained raw diff.
    #[must_use]
    pub const fn raw_length(&self) -> u32 {
        self.raw_length
    }
    /// Whether the visible safe preview omits source bytes.
    #[must_use]
    pub const fn is_truncated(&self) -> bool {
        self.truncated
    }
}

fn safe_preview(value: &str) -> (String, bool) {
    const PREVIEW_BYTES: usize = 1024;
    let mut preview = String::new();
    for character in value.chars() {
        let character = if character.is_control() && character != '\t' { '�' } else { character };
        if preview.len().saturating_add(character.len_utf8()) > PREVIEW_BYTES {
            break;
        }
        preview.push(character);
    }
    let truncated = preview.len() < value.len();
    if truncated {
        preview.push('…');
    }
    (preview, truncated)
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
