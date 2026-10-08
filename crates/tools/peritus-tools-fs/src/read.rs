//! Bounded immutable filesystem observations.

mod search;
mod traversal;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use peritus_patch::WorkspacePath;
use peritus_types::Sha256Digest;
use peritus_workspace::{
    FileReadSelection, ReadOnlyWorkspace, WorkspaceEntryKind, WorkspaceError, WorkspaceMetadata,
};

use crate::{
    DiscoverInput, FsToolError, FsToolErrorKind, FsToolOperation, MetadataInput, ReadInput,
    RecoveryClass, read_digest::DiscoverDigestBuilder,
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
    omission_reason: Option<OmissionReason>,
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
    /// Returns the reason this entry was omitted from recursive processing, if any.
    #[must_use]
    pub const fn omission_reason(&self) -> Option<OmissionReason> {
        self.omission_reason
    }
}

/// Complete bounded deterministic subtree observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoverObservation {
    root: Option<WorkspacePath>,
    entries: Vec<DiscoverEntry>,
    omissions: Vec<ScopeOmission>,
    page_start: u64,
    next_offset: Option<u64>,
    observed_count: u64,
    omission_count: u64,
    digest: Sha256Digest,
}

impl DiscoverObservation {
    /// Returns the requested root, or `None` for the workspace root.
    #[must_use]
    pub const fn root(&self) -> Option<&WorkspacePath> {
        self.root.as_ref()
    }
    /// Returns canonical traversal-order entries retained for the requested page.
    #[must_use]
    pub fn entries(&self) -> &[DiscoverEntry] {
        &self.entries
    }
    /// Returns page-local paths intentionally outside the reported traversal scope.
    #[must_use]
    pub fn omissions(&self) -> &[ScopeOmission] {
        &self.omissions
    }
    /// Returns the global entry index represented by the first page entry.
    #[must_use]
    pub const fn page_start(&self) -> u64 {
        self.page_start
    }
    /// Returns the next global entry index, if more entries remain.
    #[must_use]
    pub const fn next_offset(&self) -> Option<u64> {
        self.next_offset
    }
    /// Returns the exact entry count for the complete traversal.
    #[must_use]
    pub const fn observed_count(&self) -> u64 {
        self.observed_count
    }
    /// Returns the exact omission count for the complete traversal.
    #[must_use]
    pub const fn omission_count(&self) -> u64 {
        self.omission_count
    }
    /// Returns the digest over the complete structured observation.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

/// Reason one path was observed but excluded from recursive or textual processing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OmissionReason {
    /// Recursion stopped because the caller-selected maximum depth was reached.
    DepthLimit,
    /// Entry is a symlink or special node and was not opened or followed.
    UnsafeEntry,
    /// File exceeds the caller-selected per-file search byte bound.
    FileByteLimit,
    /// File does not contain valid UTF-8 search text.
    BinaryContent,
}

/// Explicit path and reason for a scope omission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScopeOmission {
    path: WorkspacePath,
    reason: OmissionReason,
}

impl ScopeOmission {
    /// Returns the exact path that was skipped.
    #[must_use]
    pub const fn path(&self) -> &WorkspacePath {
        &self.path
    }
    /// Returns the stable omission reason.
    #[must_use]
    pub const fn reason(&self) -> OmissionReason {
        self.reason
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
    source_digest: Sha256Digest,
    range: (u64, u64),
    continuation_offset: Option<u64>,
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
    /// Returns the complete source digest that binds every range page.
    #[must_use]
    pub const fn source_digest(&self) -> Sha256Digest {
        self.source_digest
    }
    /// Returns the exact half-open byte range returned in this page.
    #[must_use]
    pub const fn range(&self) -> (u64, u64) {
        self.range
    }
    /// Returns the next byte offset, or `None` when the source is complete.
    #[must_use]
    pub const fn continuation_offset(&self) -> Option<u64> {
        self.continuation_offset
    }
}

pub use search::{SearchMatch, SearchObservation};

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
        self.read_cancellable(input, &|| false)?.ok_or_else(|| {
            bound_error(FsToolOperation::Read, "file read was cancelled without an outcome")
        })
    }

    /// Reads an exact source range with cancellation checks between hashed chunks.
    ///
    /// # Errors
    /// Returns a typed no-follow C1 inspection or drift failure.
    pub fn read_cancellable(
        &self,
        input: &ReadInput,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<FileObservation>, FsToolError> {
        let metadata = self
            .workspace
            .metadata(&input.path)
            .map_err(|error| inspection_error(FsToolOperation::Read, &error))?;
        let end = input.offset.saturating_add(input.maximum_bytes).min(metadata.size());
        let selection = FileReadSelection::bytes(input.offset, end)
            .map_err(|error| inspection_error(FsToolOperation::Read, &error))?;
        let inspected = self
            .workspace
            .read_file_selection_cancellable(&input.path, selection, input.maximum_bytes, || {
                cancelled()
            })
            .map_err(|error| inspection_error(FsToolOperation::Read, &error))?;
        let Some(inspected) = inspected else {
            return Ok(None);
        };
        let bytes = inspected.bytes().to_vec();
        let content_digest = peritus_codec::sha256(&bytes);
        let content = String::from_utf8(bytes.clone())
            .map_or_else(|_| FileContent::Base64(STANDARD.encode(bytes)), FileContent::Utf8);
        let (start, next) = inspected.range();
        Ok(Some(FileObservation {
            metadata: project_metadata(&metadata),
            content,
            content_digest,
            source_digest: inspected.source_digest(),
            range: (start, next),
            continuation_offset: (next < inspected.source_bytes()).then_some(next),
        }))
    }

    /// Discovers a bounded subtree without following any symlink.
    ///
    /// # Errors
    /// Returns a typed C1 failure or rejects a result exceeding caller-selected bounds.
    pub fn discover(&self, input: &DiscoverInput) -> Result<DiscoverObservation, FsToolError> {
        self.discover_cancellable(input, &|| false)?.ok_or_else(|| {
            bound_error(FsToolOperation::Discover, "discovery was cancelled without an outcome")
        })
    }

    /// Discovers a subtree with cancellation checks between deterministically ordered entries.
    ///
    /// # Errors
    /// Returns a typed C1 inspection or traversal failure.
    pub fn discover_cancellable(
        &self,
        input: &DiscoverInput,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<DiscoverObservation>, FsToolError> {
        let mut entries = Vec::new();
        let mut omissions = Vec::new();
        let mut digest = DiscoverDigestBuilder::new(input.root.as_ref());
        let mut observed_count = 0_u64;
        let mut omission_count = 0_u64;
        let mut overflowed = false;
        let page_end =
            input.continuation_offset.checked_add(u64::from(input.maximum_entries)).ok_or_else(
                || bound_error(FsToolOperation::Discover, "discovery continuation overflowed"),
            )?;
        let completed = self.walk_visit(
            input.root.as_ref(),
            input.maximum_depth,
            FsToolOperation::Discover,
            cancelled,
            |metadata, depth, omission| {
                let index = observed_count;
                let omission_reason = omission.as_ref().map(ScopeOmission::reason);
                digest.entry(&metadata, depth);
                let Some(next_count) = observed_count.checked_add(1) else {
                    overflowed = true;
                    return false;
                };
                observed_count = next_count;
                if let Some(omission) = omission {
                    digest.omission(&omission);
                    let Some(next_omission_count) = omission_count.checked_add(1) else {
                        overflowed = true;
                        return false;
                    };
                    omission_count = next_omission_count;
                    if index >= input.continuation_offset && index < page_end {
                        omissions.push(omission);
                    }
                }
                if index >= input.continuation_offset && index < page_end {
                    entries.push(DiscoverEntry { metadata, depth, omission_reason });
                }
                true
            },
        )?;
        if !completed {
            if overflowed {
                return Err(bound_error(FsToolOperation::Discover, "discovery count overflowed"));
            }
            return Ok(None);
        }
        let next_offset = (observed_count > page_end).then_some(page_end);
        let digest = digest.finish(observed_count, omission_count);
        Ok(Some(DiscoverObservation {
            root: input.root.clone(),
            entries,
            omissions,
            page_start: input.continuation_offset,
            next_offset,
            observed_count,
            omission_count,
            digest,
        }))
    }
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
