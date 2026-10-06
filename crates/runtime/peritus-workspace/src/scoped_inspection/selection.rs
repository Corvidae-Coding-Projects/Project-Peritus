//! Explicit exact selections; line ranges resolve to byte ranges during one source scan.

use super::{WorkspaceError, invalid};

/// Explicit whole-file, half-open byte, or one-based inclusive line selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileReadSelection(pub(super) Selection);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Selection {
    All,
    Bytes { start: u64, end: u64 },
    Lines { first: u64, last: u64 },
}

impl FileReadSelection {
    /// Selects the complete file; streaming inspection has no total inclusion ceiling.
    #[must_use]
    pub const fn all() -> Self {
        Self(Selection::All)
    }
    /// Selects an exact nonempty half-open byte range; bounds are rechecked against the source.
    ///
    /// # Errors
    /// Rejects an empty or reversed range.
    pub const fn bytes(start: u64, end: u64) -> Result<Self, WorkspaceError> {
        if start >= end {
            return Err(invalid("byte selection is outside inspection bounds"));
        }
        Ok(Self(Selection::Bytes { start, end }))
    }
    /// Selects complete lines including their original terminators, without UTF-8 rewriting.
    ///
    /// # Errors
    /// Rejects zero or reversed line bounds.
    pub const fn lines(first: u32, last: u32) -> Result<Self, WorkspaceError> {
        Self::lines_u64(first as u64, last as u64)
    }
    /// Selects inclusive one-based lines using the source's full-width byte-count domain.
    ///
    /// # Errors
    /// Rejects zero or reversed bounds; absent lines are rejected when the source is scanned.
    pub const fn lines_u64(first: u64, last: u64) -> Result<Self, WorkspaceError> {
        if first == 0 || first > last {
            return Err(invalid("line selection is outside inspection bounds"));
        }
        Ok(Self(Selection::Lines { first, last }))
    }
}
