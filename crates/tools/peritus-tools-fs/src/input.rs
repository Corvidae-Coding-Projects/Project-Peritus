//! Checked bounded filesystem-tool input values.

use peritus_patch::{FileMode, LineEndingPolicy, Preimage, WorkspacePath};
use peritus_tool_protocol::ArtifactReference;

use crate::{FsToolError, FsToolOperation};

mod search;
pub use search::{SearchInput, SearchMatchField};

/// One exact checked workspace-relative metadata path input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataInput {
    pub(crate) path: WorkspacePath,
}

impl MetadataInput {
    /// Creates the checked metadata path input.
    ///
    /// # Errors
    /// Rejects an invalid or protected workspace path.
    pub fn new(path: impl Into<String>) -> Result<Self, FsToolError> {
        let path = WorkspacePath::new(path.into()).map_err(|_| {
            FsToolError::invalid(
                FsToolOperation::Metadata,
                "workspace path is invalid or protected",
            )
        })?;
        Ok(Self { path })
    }
}

/// Bounded recursive workspace discovery input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoverInput {
    pub(crate) root: Option<WorkspacePath>,
    pub(crate) maximum_depth: u16,
    pub(crate) maximum_entries: u32,
    pub(crate) continuation_offset: u64,
    pub(crate) path_offset: Option<u64>,
}

impl DiscoverInput {
    /// Creates bounded discovery rooted at the workspace root or one checked subdirectory.
    ///
    /// # Errors
    /// Rejects invalid paths and zero or excessive traversal bounds.
    pub fn new(
        root: Option<String>,
        maximum_depth: u16,
        maximum_entries: u32,
    ) -> Result<Self, FsToolError> {
        Self::page(root, maximum_depth, maximum_entries, 0)
    }

    /// Creates a discovery page following an exact count of preceding entries.
    ///
    /// # Errors
    /// Rejects invalid paths and zero or excessive traversal bounds.
    pub fn page(
        root: Option<String>,
        maximum_depth: u16,
        maximum_entries: u32,
        continuation_offset: u64,
    ) -> Result<Self, FsToolError> {
        validate_traversal(FsToolOperation::Discover, maximum_depth, maximum_entries)?;
        let root = root
            .map(WorkspacePath::new)
            .transpose()
            .map_err(|_| FsToolError::invalid(FsToolOperation::Discover, "root path is invalid"))?;
        Ok(Self { root, maximum_depth, maximum_entries, continuation_offset, path_offset: None })
    }

    /// Requests a byte range of the entry path at the current continuation offset.
    #[must_use]
    pub const fn with_path_offset(mut self, offset: Option<u64>) -> Self {
        self.path_offset = offset;
        self
    }
}

/// One exact bounded file read.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadInput {
    pub(crate) path: WorkspacePath,
    pub(crate) offset: u64,
    pub(crate) maximum_bytes: u64,
}

impl ReadInput {
    /// Creates one checked read input.
    ///
    /// # Errors
    /// Rejects invalid paths and zero or excessive file bounds.
    pub fn new(path: impl Into<String>, maximum_bytes: u64) -> Result<Self, FsToolError> {
        Self::range(path, 0, maximum_bytes)
    }

    /// Creates one exact half-open file byte-range page.
    ///
    /// # Errors
    /// Rejects invalid paths, a zero page size, or offset arithmetic overflow.
    pub fn range(
        path: impl Into<String>,
        offset: u64,
        maximum_bytes: u64,
    ) -> Result<Self, FsToolError> {
        let path = WorkspacePath::new(path.into()).map_err(|_| {
            FsToolError::invalid(FsToolOperation::Read, "workspace path is invalid or protected")
        })?;
        if maximum_bytes == 0 || offset.checked_add(maximum_bytes).is_none() {
            return Err(FsToolError::invalid(FsToolOperation::Read, "file byte bound is invalid"));
        }
        Ok(Self { path, offset, maximum_bytes })
    }
}

/// Exact create input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateInput(pub(crate) FinalInput);
/// Explicit create-or-replace input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WriteInput(pub(crate) WriteFields);
/// Exact deletion input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoveInput(pub(crate) ExistingInput);
/// Exact replacement input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplaceInput(pub(crate) ReplaceFields);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinalInput {
    pub path: WorkspacePath,
    pub content: MutationContent,
    pub mode: FileMode,
    pub line_endings: LineEndingPolicy,
}

/// Inline mutation bytes or a complete immutable artifact reference.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MutationContent {
    /// Exact bytes carried in the bounded tool call.
    Inline(Vec<u8>),
    /// Exact bytes resolved by the dispatcher-bound caller authority.
    Artifact(ArtifactReference),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExistingInput {
    pub path: WorkspacePath,
    pub preimage: Preimage,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WriteFields {
    pub final_input: FinalInput,
    pub preimage: Preimage,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplaceFields {
    pub existing: ExistingInput,
    pub content: MutationContent,
    pub mode: FileMode,
    pub line_endings: LineEndingPolicy,
}

impl CreateInput {
    /// Creates an absent-target file input.
    ///
    /// # Errors
    /// Rejects an invalid path or oversized final content.
    pub fn new(
        path: impl Into<String>,
        bytes: Vec<u8>,
        mode: FileMode,
        line_endings: LineEndingPolicy,
    ) -> Result<Self, FsToolError> {
        Ok(Self(final_input(FsToolOperation::Create, path, bytes, mode, line_endings)?))
    }
}

impl WriteInput {
    /// Creates an explicit absent-or-present write input.
    ///
    /// # Errors
    /// Rejects an invalid path, preimage, or oversized final content.
    pub fn new(
        path: impl Into<String>,
        preimage: Preimage,
        bytes: Vec<u8>,
        mode: FileMode,
        line_endings: LineEndingPolicy,
    ) -> Result<Self, FsToolError> {
        let final_input = final_input(FsToolOperation::Write, path, bytes, mode, line_endings)?;
        Ok(Self(WriteFields { final_input, preimage }))
    }
}

impl RemoveInput {
    /// Creates an exact present-target deletion input.
    ///
    /// # Errors
    /// Rejects an invalid path or absent preimage.
    pub fn new(path: impl Into<String>, preimage: Preimage) -> Result<Self, FsToolError> {
        Ok(Self(existing_input(FsToolOperation::Remove, path, preimage)?))
    }
}

impl ReplaceInput {
    /// Creates an exact present-target replacement input.
    ///
    /// # Errors
    /// Rejects an invalid path, absent preimage, or oversized final content.
    pub fn new(
        path: impl Into<String>,
        preimage: Preimage,
        bytes: Vec<u8>,
        mode: FileMode,
        line_endings: LineEndingPolicy,
    ) -> Result<Self, FsToolError> {
        let final_input = final_input(FsToolOperation::Replace, path, bytes, mode, line_endings)?;
        let existing =
            existing_input(FsToolOperation::Replace, final_input.path.as_str(), preimage)?;
        Ok(Self(ReplaceFields {
            existing,
            content: final_input.content,
            mode: final_input.mode,
            line_endings: final_input.line_endings,
        }))
    }

    /// Creates an exact replacement using an immutable artifact reference.
    ///
    /// # Errors
    /// Rejects an invalid path or absent preimage.
    pub fn from_artifact(
        path: impl Into<String>,
        preimage: Preimage,
        reference: ArtifactReference,
        mode: FileMode,
        line_endings: LineEndingPolicy,
    ) -> Result<Self, FsToolError> {
        let existing = existing_input(FsToolOperation::Replace, path, preimage)?;
        Ok(Self(ReplaceFields {
            existing,
            content: MutationContent::Artifact(reference),
            mode,
            line_endings,
        }))
    }
}

impl CreateInput {
    /// Creates an absent-target file input using an immutable artifact reference.
    ///
    /// # Errors
    /// Rejects an invalid path.
    pub fn from_artifact(
        path: impl Into<String>,
        reference: ArtifactReference,
        mode: FileMode,
        line_endings: LineEndingPolicy,
    ) -> Result<Self, FsToolError> {
        let path = WorkspacePath::new(path.into()).map_err(|_| {
            FsToolError::invalid(FsToolOperation::Create, "workspace path is invalid or protected")
        })?;
        Ok(Self(FinalInput {
            path,
            content: MutationContent::Artifact(reference),
            mode,
            line_endings,
        }))
    }
}

impl WriteInput {
    /// Creates an explicit write input using an immutable artifact reference.
    ///
    /// # Errors
    /// Rejects an invalid path or absent preimage.
    pub fn from_artifact(
        path: impl Into<String>,
        preimage: Preimage,
        reference: ArtifactReference,
        mode: FileMode,
        line_endings: LineEndingPolicy,
    ) -> Result<Self, FsToolError> {
        let final_input = CreateInput::from_artifact(path, reference, mode, line_endings)?;
        Ok(Self(WriteFields { final_input: final_input.0, preimage }))
    }
}

/// One explicit operation within an atomic multi-file patch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PatchEdit {
    /// Create an absent file.
    Create(CreateInput),
    /// Replace an exact present file.
    Replace(ReplaceInput),
    /// Delete an exact present file.
    Remove(RemoveInput),
}

/// Nonempty bounded atomic multi-file patch input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PatchInput {
    pub(crate) edits: Vec<PatchEdit>,
}

impl PatchInput {
    /// Creates a checked nonempty patch input.
    ///
    /// # Errors
    /// Rejects an empty patch.
    pub fn new(edits: Vec<PatchEdit>) -> Result<Self, FsToolError> {
        if edits.is_empty() {
            return Err(FsToolError::invalid(
                FsToolOperation::Patch,
                "patch operation count is outside its bound",
            ));
        }
        Ok(Self { edits })
    }
}

fn final_input(
    operation: FsToolOperation,
    path: impl Into<String>,
    bytes: Vec<u8>,
    mode: FileMode,
    line_endings: LineEndingPolicy,
) -> Result<FinalInput, FsToolError> {
    let path = WorkspacePath::new(path.into())
        .map_err(|_| FsToolError::invalid(operation, "workspace path is invalid or protected"))?;
    Ok(FinalInput { path, content: MutationContent::Inline(bytes), mode, line_endings })
}

fn existing_input(
    operation: FsToolOperation,
    path: impl Into<String>,
    preimage: Preimage,
) -> Result<ExistingInput, FsToolError> {
    if preimage == Preimage::Absent {
        return Err(FsToolError::invalid(operation, "operation requires a present preimage"));
    }
    let path = WorkspacePath::new(path.into())
        .map_err(|_| FsToolError::invalid(operation, "workspace path is invalid or protected"))?;
    Ok(ExistingInput { path, preimage })
}

const fn validate_traversal(
    operation: FsToolOperation,
    maximum_depth: u16,
    maximum_entries: u32,
) -> Result<(), FsToolError> {
    if !crate::verified::traversal_bounds_valid(maximum_depth, maximum_entries, u16::MAX, u32::MAX)
    {
        return Err(FsToolError::invalid(operation, "traversal bounds are invalid"));
    }
    Ok(())
}

const fn validate_file_bound(
    operation: FsToolOperation,
    maximum_bytes: u64,
) -> Result<(), FsToolError> {
    if maximum_bytes == 0 {
        return Err(FsToolError::invalid(operation, "file byte bound is invalid"));
    }
    Ok(())
}
