//! Exact native exclusions retained by discovery and search.

use peritus_patch::WorkspacePath;
use peritus_workspace::{DirectoryExclusionReason, NativeEntryName};

/// One observed native child excluded from path-based discovery or content search.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoverExclusion {
    pub(crate) directory: Option<WorkspacePath>,
    pub(crate) name: NativeEntryName,
    pub(crate) reason: DirectoryExclusionReason,
    pub(crate) depth: u16,
}
impl DiscoverExclusion {
    /// Returns the authority-safe parent directory, or the selected workspace root.
    #[must_use]
    pub const fn directory(&self) -> Option<&WorkspacePath> {
        self.directory.as_ref()
    }
    /// Borrows the exact native identity. Display text does not authorize effects.
    #[must_use]
    pub const fn name(&self) -> &NativeEntryName {
        &self.name
    }
    /// Returns the reason content or traversal is unavailable for this child.
    #[must_use]
    pub const fn reason(&self) -> DirectoryExclusionReason {
        self.reason
    }
    /// Returns one-based depth below the requested root.
    #[must_use]
    pub const fn depth(&self) -> u16 {
        self.depth
    }
}
