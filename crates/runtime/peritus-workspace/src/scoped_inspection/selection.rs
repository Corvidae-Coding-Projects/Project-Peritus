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
    /// Selects the complete file, rejecting it if the caller's inclusion bound is exceeded.
    #[must_use]
    pub const fn all() -> Self {
        Self(Selection::All)
    }
    /// Selects an exact half-open byte range; bounds are rechecked against the source.
    ///
    /// # Errors
    /// Rejects a reversed range.
    pub const fn bytes(start: u64, end: u64) -> Result<Self, WorkspaceError> {
        if start > end {
            return Err(invalid("byte selection is outside inspection bounds"));
        }
        Ok(Self(Selection::Bytes { start, end }))
    }
    /// Selects complete lines including their original terminators, without UTF-8 rewriting.
    ///
    /// # Errors
    /// Rejects zero or reversed line bounds.
    pub const fn lines(first: u64, last: u64) -> Result<Self, WorkspaceError> {
        if first == 0 || first > last {
            return Err(invalid("line selection is outside inspection bounds"));
        }
        Ok(Self(Selection::Lines { first, last }))
    }
}
