//! Digest-bound structured-diff continuation over files, hunks, and line ranges.

use super::{
    MAX_WORKBENCH_DIFF_PAGE_LINES, WorkbenchDiffFile, WorkbenchDiffHunk, WorkbenchReviewAnchor,
    malformed,
};
use crate::{AppProtocolError, WorkbenchQuery};
use peritus_types::{RunId, Sha256Digest};

/// Maximum exact raw-diff bytes returned by one request.
pub const MAX_WORKBENCH_REVIEW_DIFF_BYTES: usize = 32 * 1024;

/// Request for one exact byte range from a retained raw diff.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchReviewDiffBytesQuery {
    query: WorkbenchQuery,
    run: RunId,
    revision: u64,
    candidate_digest: Sha256Digest,
    diff_digest: Sha256Digest,
    offset: u32,
    maximum_bytes: u32,
}

/// One exact bounded byte range from an authorized raw diff.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchReviewDiffBytes {
    query: WorkbenchReviewDiffBytesQuery,
    total_bytes: u32,
    bytes: Vec<u8>,
}

impl WorkbenchReviewDiffBytes {
    /// Constructs a range after validating its request binding and exact bounds.
    ///
    /// # Errors
    /// Rejects oversized, empty, or out-of-range content.
    pub fn new(
        query: WorkbenchReviewDiffBytesQuery,
        total_bytes: u32,
        bytes: Vec<u8>,
    ) -> Result<Self, AppProtocolError> {
        let end = query
            .offset()
            .checked_add(u32::try_from(bytes.len()).map_err(|_| malformed())?)
            .ok_or_else(malformed)?;
        if query.revision() == 0
            || query.maximum_bytes() == 0
            || query.maximum_bytes() as usize > MAX_WORKBENCH_REVIEW_DIFF_BYTES
            || bytes.len() > query.maximum_bytes() as usize
            || end > total_bytes
            || (query.offset() < total_bytes && bytes.is_empty())
        {
            return Err(malformed());
        }
        Ok(Self { query, total_bytes, bytes })
    }
    /// Returns the exact range request.
    #[must_use]
    pub const fn query(&self) -> WorkbenchReviewDiffBytesQuery {
        self.query
    }
    /// Returns the full raw diff length.
    #[must_use]
    pub const fn total_bytes(&self) -> u32 {
        self.total_bytes
    }
    /// Returns the exact requested bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl WorkbenchReviewDiffBytesQuery {
    /// Creates a query for one bounded range of an immutable, digest-bound diff.
    #[must_use]
    pub const fn new(
        query: WorkbenchQuery,
        run: RunId,
        revision: u64,
        candidate_digest: Sha256Digest,
        diff_digest: Sha256Digest,
        offset: u32,
        maximum_bytes: u32,
    ) -> Self {
        Self { query, run, revision, candidate_digest, diff_digest, offset, maximum_bytes }
    }
    /// Returns the workspace query scope.
    #[must_use]
    pub const fn query(self) -> WorkbenchQuery {
        self.query
    }
    /// Returns the bound run.
    #[must_use]
    pub const fn run(self) -> RunId {
        self.run
    }
    /// Returns the bound conversation revision.
    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision
    }
    /// Returns the candidate digest.
    #[must_use]
    pub const fn candidate_digest(self) -> Sha256Digest {
        self.candidate_digest
    }
    /// Returns the raw diff digest.
    #[must_use]
    pub const fn diff_digest(self) -> Sha256Digest {
        self.diff_digest
    }
    /// Returns the requested byte offset.
    #[must_use]
    pub const fn offset(self) -> u32 {
        self.offset
    }
    /// Returns the requested maximum byte count.
    #[must_use]
    pub const fn maximum_bytes(self) -> u32 {
        self.maximum_bytes
    }
}

/// Cursor for one exact diff view in a conversation/run/revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchReviewDiffQuery {
    query: WorkbenchQuery,
    run: RunId,
    revision: u64,
    file_offset: u32,
    hunk_offset: u32,
    line_offset: u32,
}

impl WorkbenchReviewDiffQuery {
    /// Creates a cursor. The first page uses zero offsets and revision zero means latest.
    /// Returns the enclosing workspace query.
    #[must_use]
    pub const fn new(
        query: WorkbenchQuery,
        run: RunId,
        revision: u64,
        file_offset: u32,
        hunk_offset: u32,
        line_offset: u32,
    ) -> Self {
        Self { query, run, revision, file_offset, hunk_offset, line_offset }
    }
    /// Returns the enclosing workspace query.
    #[must_use]
    pub const fn query(self) -> WorkbenchQuery {
        self.query
    }
    /// Returns the exact run identity.
    #[must_use]
    pub const fn run(self) -> RunId {
        self.run
    }
    /// Returns the revision pinned by this cursor.
    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision
    }
    /// Returns the zero-based changed-file offset.
    #[must_use]
    pub const fn file_offset(self) -> u32 {
        self.file_offset
    }
    /// Returns the zero-based hunk offset within the selected file.
    #[must_use]
    pub const fn hunk_offset(self) -> u32 {
        self.hunk_offset
    }
    /// Returns the zero-based line offset within the selected hunk.
    #[must_use]
    pub const fn line_offset(self) -> u32 {
        self.line_offset
    }
}

/// One safe source-line preview bound to an exact raw-diff byte range.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchReviewDiffLine {
    kind: super::WorkbenchDiffLineKind,
    preview: String,
    raw_offset: u32,
    raw_length: u32,
    truncated: bool,
}

impl WorkbenchReviewDiffLine {
    pub(crate) fn from_wire(
        kind: super::WorkbenchDiffLineKind,
        preview: String,
        raw_offset: u32,
        raw_length: u32,
        truncated: bool,
    ) -> Result<Self, AppProtocolError> {
        if preview.len() > 1025 || raw_offset.checked_add(raw_length).is_none() {
            return Err(malformed());
        }
        Ok(Self { kind, preview, raw_offset, raw_length, truncated })
    }

    /// Returns the line classification.
    #[must_use]
    pub const fn kind(&self) -> super::WorkbenchDiffLineKind {
        self.kind
    }
    /// Returns the sanitized, bounded text preview.
    #[must_use]
    pub fn preview(&self) -> &str {
        &self.preview
    }
    /// Returns the byte offset of the source line in the retained diff.
    #[must_use]
    pub const fn raw_offset(&self) -> u32 {
        self.raw_offset
    }
    /// Returns the source-line byte length in the retained diff.
    #[must_use]
    pub const fn raw_length(&self) -> u32 {
        self.raw_length
    }
    /// Returns whether the preview omits source text.
    #[must_use]
    pub const fn is_truncated(&self) -> bool {
        self.truncated
    }
}

/// Bounded structured view with file, hunk, and within-hunk continuation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchReviewDiffPage {
    query: WorkbenchReviewDiffQuery,
    candidate_digest: Sha256Digest,
    diff_digest: Sha256Digest,
    file_anchor: WorkbenchReviewAnchor,
    hunk: Option<WorkbenchDiffHunk>,
    lines: Vec<WorkbenchReviewDiffLine>,
    total_files: u32,
    total_hunks: u64,
    total_lines: u64,
    next: Option<WorkbenchReviewDiffQuery>,
}

impl WorkbenchReviewDiffPage {
    pub(crate) fn from_wire_parts(
        query: WorkbenchReviewDiffQuery,
        candidate_digest: Sha256Digest,
        diff_digest: Sha256Digest,
        file_anchor: WorkbenchReviewAnchor,
        hunk: Option<WorkbenchDiffHunk>,
        lines: Vec<WorkbenchReviewDiffLine>,
        total_files: u32,
        total_hunks: u64,
        total_lines: u64,
        next: Option<WorkbenchReviewDiffQuery>,
    ) -> Result<Self, AppProtocolError> {
        if query.revision() == 0
            || total_files == 0
            || query.file_offset() >= total_files
            || lines.len() > MAX_WORKBENCH_DIFF_PAGE_LINES
            || file_anchor.run() != query.run()
            || file_anchor.workspace() != query.query().workspace()
            || file_anchor.candidate_digest() != candidate_digest
            || file_anchor.diff_digest() != diff_digest
            || hunk.as_ref().is_some_and(|value| {
                value.anchor().path() != file_anchor.path()
                    || value.anchor().run() != query.run()
                    || value.anchor().candidate_digest() != candidate_digest
                    || value.anchor().diff_digest() != diff_digest
            })
            || next.is_some_and(|value| {
                value.query() != query.query()
                    || value.run() != query.run()
                    || value.revision() != query.revision()
            })
        {
            return Err(malformed());
        }
        Ok(Self {
            query,
            candidate_digest,
            diff_digest,
            file_anchor,
            hunk,
            lines,
            total_files,
            total_hunks,
            total_lines,
            next,
        })
    }

    /// Projects one bounded view from the exact parsed diff. Full file/hunk anchors remain
    /// content-bound even when only a small line range is returned.
    ///
    /// # Errors
    /// Returns an error when the cursor is outside the exact diff or exceeds page bounds.
    #[allow(clippy::too_many_lines)]
    pub fn project(
        query: WorkbenchReviewDiffQuery,
        candidate_digest: Sha256Digest,
        diff_digest: Sha256Digest,
        files: &[WorkbenchDiffFile],
    ) -> Result<Self, AppProtocolError> {
        if query.revision() == 0 || files.is_empty() {
            return Err(malformed());
        }
        let file_index = usize::try_from(query.file_offset()).map_err(|_| malformed())?;
        let file = files.get(file_index).ok_or_else(malformed)?;
        if file.anchor().run() != query.run()
            || file.anchor().workspace() != query.query().workspace()
            || file.anchor().candidate_digest() != candidate_digest
            || file.anchor().diff_digest() != diff_digest
        {
            return Err(malformed());
        }
        let total_files = u32::try_from(files.len()).map_err(|_| malformed())?;
        let total_hunks =
            files.iter().map(|item| u64::try_from(item.hunks().len()).unwrap_or(u64::MAX)).sum();
        let total_lines = files
            .iter()
            .flat_map(WorkbenchDiffFile::hunks)
            .map(|hunk| u64::try_from(hunk.lines().len()).unwrap_or(u64::MAX))
            .sum();
        let hunk_index = usize::try_from(query.hunk_offset()).map_err(|_| malformed())?;
        let hunk = file.hunks().get(hunk_index);
        let selected_hunk = if file.hunks().is_empty() {
            if query.hunk_offset() != 0 || query.line_offset() != 0 {
                return Err(malformed());
            }
            None
        } else {
            Some(hunk.ok_or_else(malformed)?)
        };
        let mut lines = Vec::new();
        let mut next = None;
        let selected = if let Some(hunk) = selected_hunk {
            let start = usize::try_from(query.line_offset()).map_err(|_| malformed())?;
            if start >= hunk.lines().len() {
                return Err(malformed());
            }
            let mut bytes = 0_usize;
            let mut end = start;
            while end < hunk.lines().len() && end - start < MAX_WORKBENCH_DIFF_PAGE_LINES {
                let line = &hunk.lines()[end];
                let size = line.text().len();
                if end > start && bytes.saturating_add(size) > 32 * 1024 {
                    break;
                }
                bytes = bytes.saturating_add(size);
                lines.push(WorkbenchReviewDiffLine {
                    kind: line.kind(),
                    preview: line.text().to_owned(),
                    raw_offset: line.raw_offset(),
                    raw_length: line.raw_length(),
                    truncated: line.is_truncated(),
                });
                end += 1;
            }
            if end < hunk.lines().len() {
                next = Some(WorkbenchReviewDiffQuery::new(
                    query.query(),
                    query.run(),
                    query.revision(),
                    query.file_offset(),
                    query.hunk_offset(),
                    u32::try_from(end).map_err(|_| malformed())?,
                ));
            } else if hunk_index + 1 < file.hunks().len() {
                next = Some(WorkbenchReviewDiffQuery::new(
                    query.query(),
                    query.run(),
                    query.revision(),
                    query.file_offset(),
                    query.hunk_offset() + 1,
                    0,
                ));
            } else if file_index + 1 < files.len() {
                next = Some(WorkbenchReviewDiffQuery::new(
                    query.query(),
                    query.run(),
                    query.revision(),
                    query.file_offset() + 1,
                    0,
                    0,
                ));
            }
            Some(WorkbenchDiffHunk::new(
                hunk.anchor().clone(),
                hunk.header().to_owned(),
                hunk.lines()[start..end].to_vec(),
            )?)
        } else {
            if file_index + 1 < files.len() {
                next = Some(WorkbenchReviewDiffQuery::new(
                    query.query(),
                    query.run(),
                    query.revision(),
                    query.file_offset() + 1,
                    0,
                    0,
                ));
            }
            None
        };
        Ok(Self {
            query,
            candidate_digest,
            diff_digest,
            file_anchor: file.anchor().clone(),
            hunk: selected,
            lines,
            total_files,
            total_hunks,
            total_lines,
            next,
        })
    }

    /// Returns the request cursor that produced this page.
    #[must_use]
    pub const fn query(&self) -> WorkbenchReviewDiffQuery {
        self.query
    }
    /// Returns the candidate identity bound to this page.
    #[must_use]
    pub const fn candidate_digest(&self) -> Sha256Digest {
        self.candidate_digest
    }
    /// Returns the exact diff identity bound to this page.
    #[must_use]
    pub const fn diff_digest(&self) -> Sha256Digest {
        self.diff_digest
    }
    /// Returns the exact changed-file anchor.
    #[must_use]
    pub const fn file_anchor(&self) -> &WorkbenchReviewAnchor {
        &self.file_anchor
    }
    /// Returns the selected hunk with only this page's lines.
    #[must_use]
    pub const fn hunk(&self) -> Option<&WorkbenchDiffHunk> {
        self.hunk.as_ref()
    }
    /// Returns safe previews of the selected source lines.
    #[must_use]
    pub fn lines(&self) -> &[WorkbenchReviewDiffLine] {
        &self.lines
    }
    /// Returns the total number of changed files in the diff.
    #[must_use]
    pub const fn total_files(&self) -> u32 {
        self.total_files
    }
    /// Returns the total number of hunks across all files.
    #[must_use]
    pub const fn total_hunks(&self) -> u64 {
        self.total_hunks
    }
    /// Returns the total number of structured lines across all hunks.
    #[must_use]
    pub const fn total_lines(&self) -> u64 {
        self.total_lines
    }
    /// Returns the next page cursor, if more diff content remains.
    #[must_use]
    pub const fn next(&self) -> Option<WorkbenchReviewDiffQuery> {
        self.next
    }
}
