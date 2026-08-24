//! Bounded immutable filesystem observations.

use std::collections::VecDeque;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use peritus_patch::WorkspacePath;
use peritus_types::Sha256Digest;
use peritus_workspace::{ReadOnlyWorkspace, WorkspaceEntryKind, WorkspaceError, WorkspaceMetadata};

use crate::{
    DiscoverInput, FsToolError, FsToolErrorKind, FsToolOperation, MetadataInput, ReadInput,
    RecoveryClass, SearchInput,
    read_digest::{discover_digest, search_digest},
};

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
    digest: Sha256Digest,
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
    /// Returns the digest over the complete structured observation.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
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

/// Complete bounded literal-search observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchObservation {
    matches: Vec<SearchMatch>,
    scanned_files: u32,
    scanned_bytes: u64,
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
}

/// Read-only filesystem service fixed to one C1 immutable snapshot handle.
pub struct FsReadService<'a> {
    workspace: &'a ReadOnlyWorkspace,
}

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
        let bytes = self
            .workspace
            .read_file(&input.path, input.maximum_bytes)
            .map_err(|error| inspection_error(FsToolOperation::Read, &error))?;
        let content_digest = peritus_codec::sha256(&bytes);
        let content = String::from_utf8(bytes.clone())
            .map_or_else(|_| FileContent::Base64(STANDARD.encode(bytes)), FileContent::Utf8);
        Ok(FileObservation { metadata: project_metadata(&metadata), content, content_digest })
    }

    /// Discovers a bounded subtree without following any symlink.
    ///
    /// # Errors
    /// Returns a typed C1 failure or rejects a result exceeding caller-selected bounds.
    pub fn discover(&self, input: &DiscoverInput) -> Result<DiscoverObservation, FsToolError> {
        let entries = self.walk(
            input.root.as_ref(),
            input.maximum_depth,
            input.maximum_entries,
            FsToolOperation::Discover,
        )?;
        let entries = entries
            .into_iter()
            .map(|(metadata, depth)| DiscoverEntry { metadata, depth })
            .collect::<Vec<_>>();
        let digest = discover_digest(input.root.as_ref(), &entries);
        Ok(DiscoverObservation { root: input.root.clone(), entries, digest })
    }

    /// Searches literal UTF-8 content under explicit traversal and byte bounds.
    ///
    /// Binary and over-per-file-bound files are represented by traversal but not searched.
    ///
    /// # Errors
    /// Returns typed inspection, traversal, aggregate-byte, or match-bound failure.
    pub fn search(&self, input: &SearchInput) -> Result<SearchObservation, FsToolError> {
        let entries = self.walk(
            input.root.as_ref(),
            input.maximum_depth,
            input.maximum_entries,
            FsToolOperation::Search,
        )?;
        let mut observation = SearchObservation {
            matches: Vec::new(),
            scanned_files: 0,
            scanned_bytes: 0,
            digest: Sha256Digest::new([0; 32]),
        };
        for (metadata, _) in entries {
            if metadata.kind != WorkspaceEntryKind::File || metadata.size > input.maximum_file_bytes
            {
                continue;
            }
            let bytes = self
                .workspace
                .read_file(&metadata.path, input.maximum_file_bytes)
                .map_err(|error| inspection_error(FsToolOperation::Search, &error))?;
            let Ok(text) = std::str::from_utf8(&bytes) else { continue };
            observation.scanned_bytes = observation
                .scanned_bytes
                .checked_add(bytes.len() as u64)
                .filter(|total| *total <= input.maximum_total_bytes)
                .ok_or_else(|| {
                    bound_error(FsToolOperation::Search, "search byte bound exceeded")
                })?;
            observation.scanned_files = observation.scanned_files.saturating_add(1);
            collect_matches(input, &metadata.path, text, &mut observation.matches)?;
        }
        observation.digest = search_digest(&observation);
        Ok(observation)
    }

    fn walk(
        &self,
        root: Option<&WorkspacePath>,
        maximum_depth: u16,
        maximum_entries: u32,
        operation: FsToolOperation,
    ) -> Result<Vec<(MetadataObservation, u16)>, FsToolError> {
        let mut pending = VecDeque::from([(root.cloned(), 0_u16)]);
        let mut observed = Vec::new();
        while let Some((directory, parent_depth)) = pending.pop_front() {
            let children = self
                .workspace
                .list_directory(directory.as_ref())
                .map_err(|error| inspection_error(operation, &error))?;
            for child in children {
                if observed.len() >= maximum_entries as usize {
                    return Err(bound_error(operation, "workspace traversal entry bound exceeded"));
                }
                let depth = parent_depth.saturating_add(1);
                let metadata = project_metadata(child.metadata());
                if metadata.kind == WorkspaceEntryKind::Directory && depth < maximum_depth {
                    pending.push_back((Some(metadata.path.clone()), depth));
                }
                observed.push((metadata, depth));
            }
        }
        Ok(observed)
    }
}

fn collect_matches(
    input: &SearchInput,
    path: &WorkspacePath,
    text: &str,
    matches: &mut Vec<SearchMatch>,
) -> Result<(), FsToolError> {
    let needle = if input.case_sensitive {
        input.literal.clone()
    } else {
        input.literal.to_ascii_lowercase()
    };
    for (line_index, line) in text.lines().enumerate() {
        let haystack =
            if input.case_sensitive { line.to_owned() } else { line.to_ascii_lowercase() };
        for (column, _) in haystack.match_indices(&needle) {
            if matches.len() >= input.maximum_matches as usize {
                return Err(bound_error(FsToolOperation::Search, "search match bound exceeded"));
            }
            matches.push(SearchMatch {
                path: path.clone(),
                line: line_index as u64 + 1,
                column_bytes: u32::try_from(column).unwrap_or(u32::MAX),
                preview: bounded_preview(line),
            });
        }
    }
    Ok(())
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

const fn bound_error(operation: FsToolOperation, detail: &'static str) -> FsToolError {
    FsToolError::new(FsToolErrorKind::InvalidInput, operation, RecoveryClass::CorrectInput, detail)
}
