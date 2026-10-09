//! Checked bounded filesystem-tool input values.

use peritus_patch::{FileMode, LineEndingPolicy, Preimage, WorkspacePath};

use crate::{FsToolError, FsToolOperation};

/// Maximum subtree depth accepted by discovery or search.
pub const MAX_TRAVERSAL_DEPTH: u16 = 64;
/// Maximum entries visited by one discovery or search.
pub const MAX_TRAVERSAL_ENTRIES: u32 = 100_000;
/// Maximum literal search matches retained in one observation.
pub const MAX_SEARCH_MATCHES: u32 = 10_000;
/// Maximum aggregate bytes scanned by one search.
pub const MAX_SEARCH_BYTES: u64 = 64 * 1_024 * 1_024;
/// Maximum file bytes rendered inline by `fs.read`, including base64 expansion.
pub const MAX_TOOL_READ_BYTES: u64 = 48 * 1_024;

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
        validate_traversal(FsToolOperation::Discover, maximum_depth, maximum_entries)?;
        let root = root
            .map(WorkspacePath::new)
            .transpose()
            .map_err(|_| FsToolError::invalid(FsToolOperation::Discover, "root path is invalid"))?;
        Ok(Self { root, maximum_depth, maximum_entries })
    }
}

/// One exact bounded file read.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadInput {
    pub(crate) path: WorkspacePath,
    pub(crate) maximum_bytes: u64,
}

impl ReadInput {
    /// Creates one checked read input.
    ///
    /// # Errors
    /// Rejects invalid paths and zero or excessive file bounds.
    pub fn new(path: impl Into<String>, maximum_bytes: u64) -> Result<Self, FsToolError> {
        let path = WorkspacePath::new(path.into()).map_err(|_| {
            FsToolError::invalid(FsToolOperation::Read, "workspace path is invalid or protected")
        })?;
        if maximum_bytes == 0 || maximum_bytes > MAX_TOOL_READ_BYTES {
            return Err(FsToolError::invalid(FsToolOperation::Read, "file byte bound is invalid"));
        }
        Ok(Self { path, maximum_bytes })
    }
}

/// Bounded literal search input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchInput {
    pub(crate) root: Option<WorkspacePath>,
    pub(crate) literal: String,
    pub(crate) case_sensitive: bool,
    pub(crate) maximum_depth: u16,
    pub(crate) maximum_entries: u32,
    pub(crate) maximum_file_bytes: u64,
    pub(crate) maximum_total_bytes: u64,
    pub(crate) maximum_matches: u32,
}

impl SearchInput {
    /// Creates a regular-expression-free bounded literal search.
    ///
    /// # Errors
    /// Rejects invalid paths, empty/oversized literals, or excessive resource bounds.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        root: Option<String>,
        literal: String,
        case_sensitive: bool,
        maximum_depth: u16,
        maximum_entries: u32,
        maximum_file_bytes: u64,
        maximum_total_bytes: u64,
        maximum_matches: u32,
    ) -> Result<Self, FsToolError> {
        validate_traversal(FsToolOperation::Search, maximum_depth, maximum_entries)?;
        validate_file_bound(FsToolOperation::Search, maximum_file_bytes)?;
        if literal.is_empty()
            || literal.len() > 4_096
            || !crate::verified::search_bounds_valid(
                maximum_total_bytes,
                maximum_matches,
                MAX_SEARCH_BYTES,
                MAX_SEARCH_MATCHES,
            )
        {
            return Err(FsToolError::invalid(
                FsToolOperation::Search,
                "literal or search bounds are invalid",
            ));
        }
        let root = root
            .map(WorkspacePath::new)
            .transpose()
            .map_err(|_| FsToolError::invalid(FsToolOperation::Search, "root path is invalid"))?;
        Ok(Self {
            root,
            literal,
            case_sensitive,
            maximum_depth,
            maximum_entries,
            maximum_file_bytes,
            maximum_total_bytes,
            maximum_matches,
        })
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
    pub bytes: Vec<u8>,
    pub mode: FileMode,
    pub line_endings: LineEndingPolicy,
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
    pub bytes: Vec<u8>,
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
            bytes: final_input.bytes,
            mode: final_input.mode,
            line_endings: final_input.line_endings,
        }))
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
    /// Rejects empty or excessive operation counts.
    pub fn new(edits: Vec<PatchEdit>) -> Result<Self, FsToolError> {
        if edits.is_empty() || edits.len() > peritus_patch::MAX_PATCH_OPERATIONS {
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
    if bytes.len() > peritus_patch::MAX_PATCH_BYTES {
        return Err(FsToolError::invalid(operation, "final file exceeds the patch byte bound"));
    }
    Ok(FinalInput { path, bytes, mode, line_endings })
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
    if !crate::verified::traversal_bounds_valid(
        maximum_depth,
        maximum_entries,
        MAX_TRAVERSAL_DEPTH,
        MAX_TRAVERSAL_ENTRIES,
    ) {
        return Err(FsToolError::invalid(operation, "traversal bounds are invalid"));
    }
    Ok(())
}

const fn validate_file_bound(
    operation: FsToolOperation,
    maximum_bytes: u64,
) -> Result<(), FsToolError> {
    if maximum_bytes == 0 || maximum_bytes > peritus_workspace::MAX_INSPECTION_FILE_BYTES {
        return Err(FsToolError::invalid(operation, "file byte bound is invalid"));
    }
    Ok(())
}
