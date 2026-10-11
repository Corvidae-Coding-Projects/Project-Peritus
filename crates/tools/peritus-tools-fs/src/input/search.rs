//! Search inputs and resumable field cursors.

use peritus_patch::WorkspacePath;

use crate::{FsToolError, FsToolOperation};

use super::validate_file_bound;

/// Bounded literal search input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchInput {
    pub(crate) root: Option<WorkspacePath>,
    pub(crate) literal: String,
    pub(crate) case_sensitive: bool,
    pub(crate) maximum_depth: u16,
    pub(crate) maximum_file_bytes: u64,
    pub(crate) maximum_matches: u32,
    pub(crate) continuation_offset: u64,
    pub(crate) omission_offset: u64,
    pub(crate) match_field: Option<SearchMatchField>,
    pub(crate) match_field_offset: u64,
    pub(crate) omission_path_offset: Option<u64>,
}

/// Search match string that can be resumed with a byte-range cursor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchMatchField {
    /// Exact workspace-relative path.
    Path,
    /// UTF-8 source-line preview.
    Preview,
}

impl SearchMatchField {
    /// Returns the stable schema spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Path => "path",
            Self::Preview => "preview",
        }
    }
}

impl SearchInput {
    /// Creates a regular-expression-free bounded literal search.
    ///
    /// # Errors
    /// Rejects invalid paths, empty/oversized literals, zero bounds, or zero traversal depth.
    pub fn new(
        root: Option<String>,
        literal: String,
        case_sensitive: bool,
        maximum_depth: u16,
        maximum_file_bytes: u64,
        maximum_matches: u32,
    ) -> Result<Self, FsToolError> {
        Self::page(
            root,
            literal,
            case_sensitive,
            maximum_depth,
            maximum_file_bytes,
            maximum_matches,
        )
    }

    /// Creates a bounded literal-search page whose continuation offsets initially start at zero.
    ///
    /// # Errors
    /// Rejects invalid paths, empty/oversized literals, zero bounds, or zero traversal depth.
    pub fn page(
        root: Option<String>,
        literal: String,
        case_sensitive: bool,
        maximum_depth: u16,
        maximum_file_bytes: u64,
        maximum_matches: u32,
    ) -> Result<Self, FsToolError> {
        validate_file_bound(FsToolOperation::Search, maximum_file_bytes)?;
        if maximum_depth == 0 || literal.is_empty() || literal.len() > 4_096 || maximum_matches == 0
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
            maximum_file_bytes,
            maximum_matches,
            continuation_offset: 0,
            omission_offset: 0,
            match_field: None,
            match_field_offset: 0,
            omission_path_offset: None,
        })
    }

    /// Sets the global match and omission offsets for this page.
    #[must_use]
    pub const fn with_continuation_offsets(
        mut self,
        match_offset: u64,
        omission_offset: u64,
    ) -> Self {
        self.continuation_offset = match_offset;
        self.omission_offset = omission_offset;
        self
    }

    /// Adds a field-range cursor for one oversized match or omission item.
    #[must_use]
    pub const fn with_field_ranges(
        mut self,
        match_field: Option<SearchMatchField>,
        match_field_offset: u64,
        omission_path_offset: Option<u64>,
    ) -> Self {
        self.match_field = match_field;
        self.match_field_offset = match_field_offset;
        self.omission_path_offset = omission_path_offset;
        self
    }

    /// Validates that one resumable item-field cursor was selected.
    ///
    /// # Errors
    /// Rejects conflicting match and omission field cursors.
    pub fn validate_field_ranges(self) -> Result<Self, FsToolError> {
        if (self.match_field.is_none() && self.match_field_offset != 0)
            || (self.match_field.is_some() && self.omission_path_offset.is_some())
        {
            return Err(FsToolError::invalid(
                FsToolOperation::Search,
                "search item field cursors conflict",
            ));
        }
        Ok(self)
    }
}
