//! Bounded immutable filesystem observations.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use peritus_patch::WorkspacePath;
use peritus_types::Sha256Digest;
use peritus_workspace::{
    FileReadSelection, ReadOnlyWorkspace, WorkspaceEntryKind, WorkspaceError, WorkspaceMetadata,
};

use crate::{
    DiscoverExclusion, DiscoverInput, FsToolError, FsToolErrorKind, FsToolOperation, MetadataInput,
    ReadInput, RecoveryClass, SearchInput,
    cursor::{
        PageCursor, PageKind, ReadCursor, discover_request, search_request, snapshot_binding,
    },
    exclusion::TraversalOmission,
    read_digest::{discover_digest, search_digest},
};

mod walk;

/// Stable metadata projected for a filesystem tool result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataObservation {
    path: WorkspacePath,
    kind: WorkspaceEntryKind,
    size: u64,
    executable: bool,
}

impl MetadataObservation {
    /// Returns the canonical workspace-relative path.
    #[must_use]
    pub const fn path(&self) -> &WorkspacePath {
        &self.path
    }
    /// Returns the closed C1 entry kind.
    #[must_use]
    pub const fn kind(&self) -> WorkspaceEntryKind {
        self.kind
    }
    /// Returns exact regular-file size, or zero for a directory.
    #[must_use]
    pub const fn size(&self) -> u64 {
        self.size
    }
    /// Returns the portable executable observation.
    #[must_use]
    pub const fn executable(&self) -> bool {
        self.executable
    }
}

/// One deterministic discovered entry with root-relative depth.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoverEntry {
    metadata: MetadataObservation,
    depth: u16,
}

impl DiscoverEntry {
    /// Returns structured entry metadata.
    #[must_use]
    pub const fn metadata(&self) -> &MetadataObservation {
        &self.metadata
    }
    /// Returns one-based depth below the requested root.
    #[must_use]
    pub const fn depth(&self) -> u16 {
        self.depth
    }
}

/// Complete bounded deterministic subtree observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoverObservation {
    root: Option<WorkspacePath>,
    entries: Vec<DiscoverEntry>,
    exclusions: Vec<DiscoverExclusion>,
    omissions: Vec<TraversalOmission>,
    record_count: u64,
    entry_count: u64,
    exclusion_count: u64,
    omission_count: u64,
    digest: Sha256Digest,
    cursor: String,
    next_cursor: Option<String>,
}

impl DiscoverObservation {
    /// Returns the requested root, or `None` for the workspace root.
    #[must_use]
    pub const fn root(&self) -> Option<&WorkspacePath> {
        self.root.as_ref()
    }
    /// Returns canonical traversal-order entries.
    #[must_use]
    pub fn entries(&self) -> &[DiscoverEntry] {
        &self.entries
    }
    /// Borrows exact native child exclusions rather than silently omitting unsupported entries.
    #[must_use]
    pub fn exclusions(&self) -> &[DiscoverExclusion] {
        &self.exclusions
    }
    pub(crate) fn omissions(&self) -> &[TraversalOmission] {
        &self.omissions
    }
    /// Returns the complete logical record count across all pages.
    #[must_use]
    pub const fn record_count(&self) -> u64 {
        self.record_count
    }
    /// Returns the complete supported-entry count across all pages.
    #[must_use]
    pub const fn entry_count(&self) -> u64 {
        self.entry_count
    }
    /// Returns the complete native-exclusion count across all pages.
    #[must_use]
    pub const fn exclusion_count(&self) -> u64 {
        self.exclusion_count
    }
    /// Returns the complete depth-omission count across all pages.
    #[must_use]
    pub const fn omission_count(&self) -> u64 {
        self.omission_count
    }
    /// Returns the digest over the complete structured observation.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
    /// Returns the replay cursor for this exact page.
    #[must_use]
    pub fn cursor(&self) -> &str {
        &self.cursor
    }
    /// Returns the persistent next-page cursor when more records remain.
    #[must_use]
    pub fn next_cursor(&self) -> Option<&str> {
        self.next_cursor.as_deref()
    }
}

/// Explicit textual or base64 file content.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FileContent {
    /// Exact UTF-8 text.
    Utf8(String),
    /// Exact standard-padded base64 bytes.
    Base64(String),
}

/// Complete bounded file observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileObservation {
    metadata: MetadataObservation,
    content: FileContent,
    content_digest: Sha256Digest,
    source_bytes: u64,
    source_digest: Sha256Digest,
    range: (u64, u64),
    cursor: String,
    next_cursor: Option<String>,
}

impl FileObservation {
    /// Returns exact structured metadata.
    #[must_use]
    pub const fn metadata(&self) -> &MetadataObservation {
        &self.metadata
    }
    /// Returns explicit encoded file content.
    #[must_use]
    pub const fn content(&self) -> &FileContent {
        &self.content
    }
    /// Returns SHA-256 of original unencoded bytes.
    #[must_use]
    pub const fn content_digest(&self) -> Sha256Digest {
        self.content_digest
    }
    /// Returns the complete source size independently of this physical page.
    #[must_use]
    pub const fn source_bytes(&self) -> u64 {
        self.source_bytes
    }
    /// Returns SHA-256 of the complete source independently of this page.
    #[must_use]
    pub const fn source_digest(&self) -> Sha256Digest {
        self.source_digest
    }
    /// Returns the exact half-open source interval in this page.
    #[must_use]
    pub const fn range(&self) -> (u64, u64) {
        self.range
    }
    /// Returns the persistent replay cursor for this exact page.
    #[must_use]
    pub fn cursor(&self) -> &str {
        &self.cursor
    }
    /// Returns the persistent cursor for the next exact page.
    #[must_use]
    pub fn next_cursor(&self) -> Option<&str> {
        self.next_cursor.as_deref()
    }
}

/// One bounded literal search match.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchMatch {
    path: WorkspacePath,
    line: u64,
    column_bytes: u32,
    preview: String,
}

impl SearchMatch {
    /// Returns the matched path.
    #[must_use]
    pub const fn path(&self) -> &WorkspacePath {
        &self.path
    }
    /// Returns the one-based line number.
    #[must_use]
    pub const fn line(&self) -> u64 {
        self.line
    }
    /// Returns the zero-based UTF-8 byte column.
    #[must_use]
    pub const fn column_bytes(&self) -> u32 {
        self.column_bytes
    }
    /// Returns a bounded line preview.
    #[must_use]
    pub fn preview(&self) -> &str {
        &self.preview
    }
}

/// Why one supported regular file was not content-searched.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchOmissionReason {
    /// The file exceeded the caller-selected per-file search boundary.
    FileBytes,
    /// Admitting the file would exceed the caller-selected aggregate search boundary.
    TotalBytes,
    /// The complete file was observed but is not UTF-8 text.
    NonUtf8,
}

impl SearchOmissionReason {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::FileBytes => "file_byte_bound",
            Self::TotalBytes => "total_byte_bound",
            Self::NonUtf8 => "non_utf8",
        }
    }
}

/// One exact authority-safe file omitted from literal content search.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchOmission {
    path: WorkspacePath,
    reason: SearchOmissionReason,
}

impl SearchOmission {
    /// Returns the exact omitted workspace path.
    #[must_use]
    pub const fn path(&self) -> &WorkspacePath {
        &self.path
    }
    /// Returns the stable omission reason.
    #[must_use]
    pub const fn reason(&self) -> SearchOmissionReason {
        self.reason
    }
}

/// Complete bounded literal-search observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchObservation {
    matches: Vec<SearchMatch>,
    exclusions: Vec<DiscoverExclusion>,
    traversal_omissions: Vec<TraversalOmission>,
    omissions: Vec<SearchOmission>,
    match_count: u64,
    exclusion_count: u64,
    traversal_omission_count: u64,
    omission_count: u64,
    scanned_files: u32,
    scanned_bytes: u64,
    digest: Sha256Digest,
    cursor: String,
    next_cursor: Option<String>,
}

impl SearchObservation {
    /// Returns canonical path/line/column-ordered matches.
    #[must_use]
    pub fn matches(&self) -> &[SearchMatch] {
        &self.matches
    }
    /// Borrows native children excluded from authority-path traversal or content search.
    #[must_use]
    pub fn exclusions(&self) -> &[DiscoverExclusion] {
        &self.exclusions
    }
    pub(crate) fn traversal_omissions(&self) -> &[TraversalOmission] {
        &self.traversal_omissions
    }
    /// Borrows supported files explicitly omitted from content matching.
    #[must_use]
    pub fn omissions(&self) -> &[SearchOmission] {
        &self.omissions
    }
    /// Returns the complete match count across all pages.
    #[must_use]
    pub const fn match_count(&self) -> u64 {
        self.match_count
    }
    /// Returns the complete native-exclusion count across all pages.
    #[must_use]
    pub const fn exclusion_count(&self) -> u64 {
        self.exclusion_count
    }
    /// Returns the complete depth-omission count across all pages.
    #[must_use]
    pub const fn traversal_omission_count(&self) -> u64 {
        self.traversal_omission_count
    }
    /// Returns the complete content-omission count across all pages.
    #[must_use]
    pub const fn omission_count(&self) -> u64 {
        self.omission_count
    }
    /// Returns the number of UTF-8 regular files searched.
    #[must_use]
    pub const fn scanned_files(&self) -> u32 {
        self.scanned_files
    }
    /// Returns exact bytes searched.
    #[must_use]
    pub const fn scanned_bytes(&self) -> u64 {
        self.scanned_bytes
    }
    /// Returns the digest over the complete structured result.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
    /// Returns the persistent replay cursor for this page.
    #[must_use]
    pub fn cursor(&self) -> &str {
        &self.cursor
    }
    /// Returns the persistent next-page cursor when more records remain.
    #[must_use]
    pub fn next_cursor(&self) -> Option<&str> {
        self.next_cursor.as_deref()
    }
}

/// Read-only filesystem service fixed to one C1 immutable snapshot handle.
pub struct FsReadService<'a> {
    workspace: &'a ReadOnlyWorkspace,
}

struct WalkObservation {
    records: Vec<WalkRecord>,
}

#[derive(Clone)]
pub(crate) enum WalkRecord {
    Entry(MetadataObservation, u16),
    Exclusion(DiscoverExclusion),
    TraversalOmission(TraversalOmission),
}

#[derive(Clone)]
enum SearchCoverageRecord {
    Exclusion(DiscoverExclusion),
    TraversalOmission(TraversalOmission),
    ContentOmission(SearchOmission),
}

const RESULT_PAGE_ITEMS: usize = 500;

impl<'a> FsReadService<'a> {
    /// Binds inspection to one checked immutable workspace handle.
    #[must_use]
    pub const fn new(workspace: &'a ReadOnlyWorkspace) -> Self {
        Self { workspace }
    }

    /// Observes one exact entry.
    ///
    /// # Errors
    /// Returns a typed no-follow C1 inspection failure.
    pub fn metadata(&self, input: &MetadataInput) -> Result<MetadataObservation, FsToolError> {
        self.workspace
            .metadata(&input.path)
            .map(|metadata| project_metadata(&metadata))
            .map_err(|error| inspection_error(FsToolOperation::Metadata, &error))
    }

    /// Reads one exact bounded regular file.
    ///
    /// # Errors
    /// Returns a typed no-follow C1 inspection or drift failure.
    pub fn read(&self, input: &ReadInput) -> Result<FileObservation, FsToolError> {
        let metadata = self
            .workspace
            .metadata(&input.path)
            .map_err(|error| inspection_error(FsToolOperation::Read, &error))?;
        let snapshot = snapshot_binding(self.workspace);
        if input.expected.is_some_and(|expected| expected.snapshot != snapshot) {
            return Err(cursor_error(FsToolOperation::Read));
        }
        let source_bytes = input.expected.map_or(metadata.size(), |expected| expected.bytes);
        if metadata.size() != source_bytes
            || input.offset > source_bytes
            || (source_bytes != 0 && input.offset == source_bytes)
        {
            return Err(FsToolError::new(
                FsToolErrorKind::Inspection,
                FsToolOperation::Read,
                RecoveryClass::Reobserve,
                "read cursor source size or offset differs from the immutable workspace",
            ));
        }
        let selection = if source_bytes == 0 {
            FileReadSelection::all()
        } else {
            FileReadSelection::bytes(
                input.offset,
                input.offset.saturating_add(input.maximum_bytes).min(source_bytes),
            )
            .map_err(|error| inspection_error(FsToolOperation::Read, &error))?
        };
        let mut bytes = Vec::with_capacity(input.maximum_bytes as usize);
        let observed = self
            .workspace
            .copy_selection(&input.path, selection, &mut bytes)
            .map_err(|error| inspection_error(FsToolOperation::Read, &error))?;
        if observed.source_bytes() != source_bytes
            || input.expected.is_some_and(|expected| expected.digest != observed.source_digest())
        {
            return Err(FsToolError::new(
                FsToolErrorKind::Inspection,
                FsToolOperation::Read,
                RecoveryClass::Reobserve,
                "read cursor source digest differs from the immutable workspace",
            ));
        }
        let content_digest = observed.digest();
        let content = String::from_utf8(bytes.clone())
            .map_or_else(|_| FileContent::Base64(STANDARD.encode(bytes)), FileContent::Utf8);
        let range = observed.range();
        let cursor = ReadCursor {
            snapshot,
            path: input.path.clone(),
            source_bytes,
            source_digest: observed.source_digest(),
            offset: range.0,
        }
        .encode();
        let next_cursor = (range.1 < source_bytes)
            .then(|| ReadCursor {
                snapshot,
                path: input.path.clone(),
                source_bytes,
                source_digest: observed.source_digest(),
                offset: range.1,
            })
            .map(|cursor| cursor.encode());
        Ok(FileObservation {
            metadata: project_metadata(&metadata),
            content,
            content_digest,
            source_bytes,
            source_digest: observed.source_digest(),
            range,
            cursor,
            next_cursor,
        })
    }

    /// Discovers a bounded subtree without following any symlink.
    ///
    /// # Errors
    /// Returns a typed C1 failure or rejects a result exceeding caller-selected bounds.
    pub fn discover(&self, input: &DiscoverInput) -> Result<DiscoverObservation, FsToolError> {
        let observed = self.walk(
            input.root.as_ref(),
            input.maximum_depth,
            FsToolOperation::Discover,
        )?;
        let digest = discover_digest(input.root.as_ref(), &observed.records);
        let snapshot = snapshot_binding(self.workspace);
        let request = discover_request(input.root.as_ref(), input.maximum_depth);
        let start = page_start(input.cursor, PageKind::Discover, snapshot, request)?.0;
        let start = usize::try_from(start).map_err(|_| cursor_error(FsToolOperation::Discover))?;
        if start > observed.records.len() {
            return Err(cursor_error(FsToolOperation::Discover));
        }
        let capacity = (input.maximum_entries as usize).min(RESULT_PAGE_ITEMS);
        let end = start.saturating_add(capacity).min(observed.records.len());
        let mut entries = Vec::new();
        let mut exclusions = Vec::new();
        let mut omissions = Vec::new();
        for record in &observed.records[start..end] {
            match record {
                WalkRecord::Entry(metadata, depth) => {
                    entries.push(DiscoverEntry { metadata: metadata.clone(), depth: *depth });
                }
                WalkRecord::Exclusion(value) => exclusions.push(value.clone()),
                WalkRecord::TraversalOmission(value) => omissions.push(value.clone()),
            }
        }
        let entry_count = observed
            .records
            .iter()
            .filter(|record| matches!(record, WalkRecord::Entry(..)))
            .count() as u64;
        let exclusion_count = observed
            .records
            .iter()
            .filter(|record| matches!(record, WalkRecord::Exclusion(_)))
            .count() as u64;
        let omission_count = observed.records.len() as u64 - entry_count - exclusion_count;
        let cursor =
            PageCursor::new(snapshot, request, start as u64, 0).encode(PageKind::Discover);
        let next_cursor = (end < observed.records.len()).then(|| {
            PageCursor::new(snapshot, request, end as u64, 0).encode(PageKind::Discover)
        });
        Ok(DiscoverObservation {
            root: input.root.clone(),
            entries,
            exclusions,
            omissions,
            record_count: observed.records.len() as u64,
            entry_count,
            exclusion_count,
            omission_count,
            digest,
            cursor,
            next_cursor,
        })
    }

    /// Searches literal UTF-8 content under explicit traversal and byte bounds.
    ///
    /// Binary and over-per-file-bound files are represented by traversal but not searched.
    ///
    /// # Errors
    /// Returns typed inspection, traversal, aggregate-byte, or match-bound failure.
    pub fn search(&self, input: &SearchInput) -> Result<SearchObservation, FsToolError> {
        let observed = self.walk(
            input.root.as_ref(),
            input.maximum_depth,
            FsToolOperation::Search,
        )?;
        let mut all_matches = Vec::new();
        let mut coverage = Vec::new();
        let mut scanned_files = 0_u32;
        let mut scanned_bytes = 0_u64;
        let mut admitted_bytes = 0_u64;
        for record in observed.records {
            let metadata = match record {
                WalkRecord::Entry(metadata, _) => metadata,
                WalkRecord::Exclusion(value) => {
                    coverage.push(SearchCoverageRecord::Exclusion(value));
                    continue;
                }
                WalkRecord::TraversalOmission(value) => {
                    coverage.push(SearchCoverageRecord::TraversalOmission(value));
                    continue;
                }
            };
            if metadata.kind != WorkspaceEntryKind::File {
                continue;
            }
            if metadata.size > input.maximum_file_bytes {
                coverage.push(SearchCoverageRecord::ContentOmission(SearchOmission {
                    path: metadata.path,
                    reason: SearchOmissionReason::FileBytes,
                }));
                continue;
            }
            let Some(next_total) = admitted_bytes.checked_add(metadata.size) else {
                coverage.push(SearchCoverageRecord::ContentOmission(SearchOmission {
                    path: metadata.path,
                    reason: SearchOmissionReason::TotalBytes,
                }));
                continue;
            };
            if next_total > input.maximum_total_bytes {
                coverage.push(SearchCoverageRecord::ContentOmission(SearchOmission {
                    path: metadata.path,
                    reason: SearchOmissionReason::TotalBytes,
                }));
                continue;
            }
            admitted_bytes = next_total;
            let bytes = self
                .workspace
                .read_file(&metadata.path, input.maximum_file_bytes)
                .map_err(|error| inspection_error(FsToolOperation::Search, &error))?;
            scanned_bytes = scanned_bytes
                .checked_add(bytes.len() as u64)
                .ok_or_else(|| cursor_error(FsToolOperation::Search))?;
            scanned_files = scanned_files.saturating_add(1);
            let Ok(text) = std::str::from_utf8(&bytes) else {
                coverage.push(SearchCoverageRecord::ContentOmission(SearchOmission {
                    path: metadata.path,
                    reason: SearchOmissionReason::NonUtf8,
                }));
                continue;
            };
            collect_matches(input, &metadata.path, text, &mut all_matches);
        }
        let mut complete = SearchObservation {
            match_count: all_matches.len() as u64,
            exclusion_count: coverage
                .iter()
                .filter(|record| matches!(record, SearchCoverageRecord::Exclusion(_)))
                .count() as u64,
            traversal_omission_count: coverage
                .iter()
                .filter(|record| matches!(record, SearchCoverageRecord::TraversalOmission(_)))
                .count() as u64,
            omission_count: coverage
                .iter()
                .filter(|record| matches!(record, SearchCoverageRecord::ContentOmission(_)))
                .count() as u64,
            matches: all_matches,
            exclusions: coverage
                .iter()
                .filter_map(|record| match record {
                    SearchCoverageRecord::Exclusion(value) => Some(value.clone()),
                    _ => None,
                })
                .collect(),
            traversal_omissions: coverage
                .iter()
                .filter_map(|record| match record {
                    SearchCoverageRecord::TraversalOmission(value) => Some(value.clone()),
                    _ => None,
                })
                .collect(),
            omissions: coverage
                .iter()
                .filter_map(|record| match record {
                    SearchCoverageRecord::ContentOmission(value) => Some(value.clone()),
                    _ => None,
                })
                .collect(),
            scanned_files,
            scanned_bytes,
            digest: Sha256Digest::new([0; 32]),
            cursor: String::new(),
            next_cursor: None,
        };
        complete.digest = search_digest(&complete);
        let snapshot = snapshot_binding(self.workspace);
        let request = search_request(
            input.root.as_ref(),
            &input.literal,
            input.case_sensitive,
            input.maximum_depth,
            input.maximum_file_bytes,
            input.maximum_total_bytes,
        );
        let (match_start, coverage_start) =
            page_start(input.cursor, PageKind::Search, snapshot, request)?;
        let match_start =
            usize::try_from(match_start).map_err(|_| cursor_error(FsToolOperation::Search))?;
        let coverage_start =
            usize::try_from(coverage_start).map_err(|_| cursor_error(FsToolOperation::Search))?;
        if match_start > complete.matches.len() || coverage_start > coverage.len() {
            return Err(cursor_error(FsToolOperation::Search));
        }
        let match_capacity = (input.maximum_matches as usize).min(RESULT_PAGE_ITEMS);
        let match_end = match_start.saturating_add(match_capacity).min(complete.matches.len());
        let remaining = RESULT_PAGE_ITEMS - (match_end - match_start);
        let coverage_capacity = (input.maximum_entries as usize).min(remaining);
        let coverage_end = coverage_start.saturating_add(coverage_capacity).min(coverage.len());
        let mut page_exclusions = Vec::new();
        let mut page_traversal = Vec::new();
        let mut page_omissions = Vec::new();
        for record in &coverage[coverage_start..coverage_end] {
            match record {
                SearchCoverageRecord::Exclusion(value) => page_exclusions.push(value.clone()),
                SearchCoverageRecord::TraversalOmission(value) => {
                    page_traversal.push(value.clone());
                }
                SearchCoverageRecord::ContentOmission(value) => page_omissions.push(value.clone()),
            }
        }
        complete.matches = complete.matches[match_start..match_end].to_vec();
        complete.exclusions = page_exclusions;
        complete.traversal_omissions = page_traversal;
        complete.omissions = page_omissions;
        complete.cursor = PageCursor::new(
            snapshot,
            request,
            match_start as u64,
            coverage_start as u64,
        )
        .encode(PageKind::Search);
        complete.next_cursor =
            (match_end < complete.match_count as usize || coverage_end < coverage.len()).then(|| {
                PageCursor::new(snapshot, request, match_end as u64, coverage_end as u64)
                    .encode(PageKind::Search)
            });
        Ok(complete)
    }
}

fn collect_matches(
    input: &SearchInput,
    path: &WorkspacePath,
    text: &str,
    matches: &mut Vec<SearchMatch>,
) {
    let needle = if input.case_sensitive {
        input.literal.clone()
    } else {
        input.literal.to_ascii_lowercase()
    };
    for (line_index, line) in text.lines().enumerate() {
        let haystack =
            if input.case_sensitive { line.to_owned() } else { line.to_ascii_lowercase() };
        for (column, _) in haystack.match_indices(&needle) {
            matches.push(SearchMatch {
                path: path.clone(),
                line: line_index as u64 + 1,
                column_bytes: u32::try_from(column).unwrap_or(u32::MAX),
                preview: bounded_preview(line),
            });
        }
    }
}

fn page_start(
    cursor: Option<PageCursor>,
    kind: PageKind,
    snapshot: Sha256Digest,
    request: Sha256Digest,
) -> Result<(u64, u64), FsToolError> {
    let Some(cursor) = cursor else { return Ok((0, 0)) };
    if cursor.snapshot != snapshot || cursor.request != request {
        return Err(cursor_error(match kind {
            PageKind::Discover => FsToolOperation::Discover,
            PageKind::Search => FsToolOperation::Search,
        }));
    }
    Ok((cursor.first, cursor.second))
}

fn bounded_preview(line: &str) -> String {
    const LIMIT: usize = 512;
    if line.len() <= LIMIT {
        return line.to_owned();
    }
    let mut end = LIMIT;
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    line[..end].to_owned()
}

fn project_metadata(value: &WorkspaceMetadata) -> MetadataObservation {
    MetadataObservation {
        path: value.path().clone(),
        kind: value.kind(),
        size: value.size(),
        executable: value.executable(),
    }
}

const fn inspection_error(operation: FsToolOperation, error: &WorkspaceError) -> FsToolError {
    let recovery = match error.recovery() {
        peritus_workspace::RecoveryClass::CorrectRequest => RecoveryClass::CorrectInput,
        peritus_workspace::RecoveryClass::Reauthorize => RecoveryClass::Reauthorize,
        peritus_workspace::RecoveryClass::Reobserve => RecoveryClass::Reobserve,
        peritus_workspace::RecoveryClass::Reconcile
        | peritus_workspace::RecoveryClass::Quarantine => RecoveryClass::Reconcile,
    };
    FsToolError::new(
        FsToolErrorKind::Inspection,
        operation,
        recovery,
        "immutable workspace inspection failed",
    )
}

const fn cursor_error(operation: FsToolOperation) -> FsToolError {
    FsToolError::new(
        FsToolErrorKind::InvalidInput,
        operation,
        RecoveryClass::Reobserve,
        "filesystem continuation is stale, foreign, or outside the retained result",
    )
}
