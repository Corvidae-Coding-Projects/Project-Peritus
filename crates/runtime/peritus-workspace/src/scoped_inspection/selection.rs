//! Explicit exact selections; line ranges resolve to byte ranges during one source scan.

use super::{WorkspaceError, invalid};

/// Explicit whole-file, half-open byte, or one-based inclusive line selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileReadSelection(pub(super) Selection);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Selection {
    All,
    Bytes { start: u64, end: u64 },
    Lines { first: u32, last: u32 },
}

impl FileReadSelection {
    /// Selects the complete file, rejecting it if the caller's inclusion bound is exceeded.
    #[must_use]
    pub const fn all() -> Self {
        Self(Selection::All)
    }
    /// Selects an exact nonempty half-open byte range; bounds are rechecked against the source.
    ///
    /// # Errors
    /// Rejects an empty/reversed range or one beyond the hard source ceiling.
    pub const fn bytes(start: u64, end: u64) -> Result<Self, WorkspaceError> {
        if start >= end || end > super::MAX_INSPECTION_SOURCE_BYTES {
            return Err(invalid("byte selection is outside inspection bounds"));
        }
        Ok(Self(Selection::Bytes { start, end }))
    }
    /// Selects complete lines including their original terminators, without UTF-8 rewriting.
    ///
    /// # Errors
    /// Rejects zero, reversed, or structurally impossible line bounds.
    pub const fn lines(first: u32, last: u32) -> Result<Self, WorkspaceError> {
        if first == 0 || first > last || last > 64 * 1024 * 1024 {
            return Err(invalid("line selection is outside inspection bounds"));
        }
        Ok(Self(Selection::Lines { first, last }))
    }
}
