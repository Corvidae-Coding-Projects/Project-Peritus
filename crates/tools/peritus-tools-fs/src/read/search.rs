//! Literal filesystem search with exact global continuation offsets.

mod scanner;
use super::{FsReadService, bound_error, inspection_error};
use crate::{
    FsToolError, FsToolErrorKind, FsToolOperation, OmissionReason, RecoveryClass, ScopeOmission,
    SearchInput,
};
use peritus_patch::WorkspacePath;
use peritus_types::Sha256Digest;
use peritus_workspace::{ReadOnlyWorkspace, WorkspaceEntryKind};
use scanner::SearchFile;
use sha2::Digest as _;

/// One bounded literal search match.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchMatch {
    path: WorkspacePath,
    line: u64,
    column_bytes: u64,
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
    pub const fn column_bytes(&self) -> u64 {
        self.column_bytes
    }
    /// Returns a bounded line preview.
    #[must_use]
    pub fn preview(&self) -> &str {
        &self.preview
    }
}

/// Complete bounded literal-search observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchObservation {
    matches: Vec<SearchMatch>,
    scanned_files: u64,
    scanned_bytes: u64,
    omissions: Vec<ScopeOmission>,
    page_start: u64,
    next_offset: Option<u64>,
    match_count: u64,
    omission_page_start: u64,
    next_omission_offset: Option<u64>,
    omission_count: u64,
    digest: Sha256Digest,
}

impl SearchObservation {
    /// Returns canonical path/line/column-ordered matches.
    #[must_use]
    pub fn matches(&self) -> &[SearchMatch] {
        &self.matches
    }
    /// Returns the number of UTF-8 regular files searched.
    #[must_use]
    pub const fn scanned_files(&self) -> u64 {
        self.scanned_files
    }
    /// Returns exact bytes searched.
    #[must_use]
    pub const fn scanned_bytes(&self) -> u64 {
        self.scanned_bytes
    }
    /// Returns paths intentionally excluded from literal matching.
    #[must_use]
    pub fn omissions(&self) -> &[ScopeOmission] {
        &self.omissions
    }
    /// Returns the first global match index in this page.
    #[must_use]
    pub const fn page_start(&self) -> u64 {
        self.page_start
    }
    /// Returns the next global match index, if more results exist.
    #[must_use]
    pub const fn next_offset(&self) -> Option<u64> {
        self.next_offset
    }
    /// Returns the exact match count across the complete traversal.
    #[must_use]
    pub const fn match_count(&self) -> u64 {
        self.match_count
    }
    /// Returns the global omission index represented by the retained omission page.
    #[must_use]
    pub const fn omission_page_start(&self) -> u64 {
        self.omission_page_start
    }
    /// Returns the next omission index, if more omissions remain.
    #[must_use]
    pub const fn next_omission_offset(&self) -> Option<u64> {
        self.next_omission_offset
    }
    /// Returns the exact omission count across the complete traversal.
    #[must_use]
    pub const fn omission_count(&self) -> u64 {
        self.omission_count
    }
    /// Returns the digest over the complete structured result.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

impl FsReadService<'_> {
    /// Searches literal UTF-8 content under explicit traversal and byte bounds.
    ///
    /// Binary and over-per-file-bound files are represented by traversal but not searched.
    ///
    /// # Errors
    /// Returns inspection, traversal, and continuation failures. Matches and omissions are
    /// retained as exact caller-selected pages while counts and digests cover the full traversal.
    pub fn search(&self, input: &SearchInput) -> Result<SearchObservation, FsToolError> {
        self.search_cancellable(input, &|| false)?.ok_or_else(|| {
            bound_error(FsToolOperation::Search, "search was cancelled without an outcome")
        })
    }

    /// Searches with cancellation checks between directory entries and source chunks.
    ///
    /// # Errors
    /// Returns typed inspection, traversal, or checked continuation failures.
    pub fn search_cancellable(
        &self,
        input: &SearchInput,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<SearchObservation>, FsToolError> {
        let mut scan = SearchTraversal::new(input)?;
        let mut diagnostics = Vec::new();
        let completed = self.walk_visit(
            input.root.as_ref(),
            input.maximum_depth,
            FsToolOperation::Search,
            cancelled,
            &mut diagnostics,
            |metadata, _depth, traversal_omission| {
                scan.visit_entry(self.workspace, input, metadata, traversal_omission, cancelled)
            },
        )?;
        diagnostics.sort_unstable_by(|left, right| {
            left.native_path_bytes().cmp(right.native_path_bytes())
        });
        for diagnostic in diagnostics {
            if !scan.record_omission(diagnostic) {
                return scan.finish(false, cancelled());
            }
        }
        scan.finish(completed, cancelled())
    }
}

struct SearchTraversal {
    observation: SearchObservation,
    digest: crate::read_digest::SearchDigestBuilder,
    global_match: u64,
    match_end: u64,
    omission_end: u64,
    omission_count: u64,
    cancelled: bool,
    error: Option<FsToolError>,
}

impl SearchTraversal {
    fn new(input: &SearchInput) -> Result<Self, FsToolError> {
        let match_end = input
            .continuation_offset
            .checked_add(u64::from(input.maximum_matches))
            .ok_or_else(|| bound_error(FsToolOperation::Search, "match continuation overflowed"))?;
        let omission_end =
            input.omission_offset.checked_add(u64::from(input.maximum_matches)).ok_or_else(
                || bound_error(FsToolOperation::Search, "omission continuation overflowed"),
            )?;
        Ok(Self {
            observation: SearchObservation {
                matches: Vec::new(),
                scanned_files: 0,
                scanned_bytes: 0,
                omissions: Vec::new(),
                page_start: input.continuation_offset,
                next_offset: None,
                match_count: 0,
                omission_page_start: input.omission_offset,
                next_omission_offset: None,
                omission_count: 0,
                digest: Sha256Digest::new([0; 32]),
            },
            digest: crate::read_digest::SearchDigestBuilder::new(
                input.continuation_offset,
                input.omission_offset,
            ),
            global_match: 0,
            match_end,
            omission_end,
            omission_count: 0,
            cancelled: false,
            error: None,
        })
    }

    fn visit_entry(
        &mut self,
        workspace: &ReadOnlyWorkspace,
        input: &SearchInput,
        metadata: super::MetadataObservation,
        traversal_omission: Option<ScopeOmission>,
        cancelled: &dyn Fn() -> bool,
    ) -> bool {
        if cancelled() {
            self.cancelled = true;
            return false;
        }
        if let Some(omission) = traversal_omission
            && !self.record_omission(omission)
        {
            return false;
        }
        if metadata.kind != WorkspaceEntryKind::File {
            return true;
        }
        if metadata.size > input.maximum_file_bytes {
            return self.record_omission(ScopeOmission::at_path(
                metadata.path,
                OmissionReason::FileByteLimit,
            ));
        }
        self.scan_file(workspace, input, &metadata, cancelled)
    }

    fn scan_file(
        &mut self,
        workspace: &ReadOnlyWorkspace,
        input: &SearchInput,
        metadata: &super::MetadataObservation,
        cancelled: &dyn Fn() -> bool,
    ) -> bool {
        let mut file = SearchFile::new(&metadata.path, input, self.global_match, self.match_end);
        let scanned = match workspace
            .scan_file_chunks(&metadata.path, cancelled, |_, bytes| file.accept_bytes(bytes))
        {
            Ok(Some(scanned)) => scanned,
            Ok(None) => {
                self.cancelled = true;
                return false;
            }
            Err(error) => return self.fail(inspection_error(FsToolOperation::Search, &error)),
        };
        if let Some(error) = file.failure.take() {
            return self.fail(error);
        }
        let (source_bytes, _) = scanned;
        if source_bytes != metadata.size {
            return self.fail(FsToolError::new(
                FsToolErrorKind::Inspection,
                FsToolOperation::Search,
                RecoveryClass::Reobserve,
                "file size changed between enumeration and streamed search",
            ));
        }
        file.finish();
        if let Some(error) = file.failure.take() {
            return self.fail(error);
        }
        if file.binary {
            return self.record_omission(ScopeOmission::at_path(
                metadata.path.clone(),
                OmissionReason::BinaryContent,
            ));
        }
        self.digest.match_group(
            &metadata.path,
            file.file_match_count,
            Sha256Digest::new(file.match_digest.finalize().into()),
        );
        let Some(scanned_bytes) = self.observation.scanned_bytes.checked_add(source_bytes) else {
            return self.fail(bound_error(FsToolOperation::Search, "search byte count overflowed"));
        };
        self.observation.scanned_bytes = scanned_bytes;
        let Some(scanned_files) = self.observation.scanned_files.checked_add(1) else {
            return self
                .fail(bound_error(FsToolOperation::Search, "searched file count overflowed"));
        };
        self.observation.scanned_files = scanned_files;
        self.global_match = file.global_match;
        self.observation.matches.extend(file.matches);
        true
    }

    fn record_omission(&mut self, omission: ScopeOmission) -> bool {
        let page_start = self.observation.omission_page_start();
        if record_search_omission(
            &mut self.digest,
            &mut self.observation.omissions,
            &mut self.omission_count,
            page_start,
            self.omission_end,
            omission,
        ) {
            true
        } else {
            self.fail(bound_error(FsToolOperation::Search, "omission count overflowed"))
        }
    }

    const fn fail(&mut self, error: FsToolError) -> bool {
        self.error = Some(error);
        false
    }

    fn finish(
        mut self,
        completed: bool,
        cancelled: bool,
    ) -> Result<Option<SearchObservation>, FsToolError> {
        if self.cancelled || cancelled {
            return Ok(None);
        }
        if let Some(error) = self.error {
            return Err(error);
        }
        if !completed {
            return Ok(None);
        }
        self.observation.match_count = self.global_match;
        self.observation.next_offset =
            (self.global_match > self.match_end).then_some(self.match_end);
        self.observation.omission_count = self.omission_count;
        self.observation.next_omission_offset =
            (self.omission_count > self.omission_end).then_some(self.omission_end);
        self.observation.digest = self.digest.finish(
            self.observation.scanned_files,
            self.observation.scanned_bytes,
            self.observation.next_offset,
            self.observation.match_count,
            self.observation.omission_count,
            self.observation.next_omission_offset,
        );
        Ok(Some(self.observation))
    }
}

fn record_search_omission(
    digest: &mut crate::read_digest::SearchDigestBuilder,
    retained: &mut Vec<ScopeOmission>,
    count: &mut u64,
    page_start: u64,
    page_end: u64,
    omission: ScopeOmission,
) -> bool {
    digest.omission(&omission);
    let index = *count;
    let Some(next_count) = count.checked_add(1) else { return false };
    *count = next_count;
    if index >= page_start && index < page_end {
        retained.push(omission);
    }
    true
}
