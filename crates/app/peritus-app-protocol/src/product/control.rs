//! Explicit control operations for daemon-owned product runs.

use peritus_types::RunId;

/// User control operation for one daemon-owned run.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProductRunControlAction {
    /// Cancel active provider and repository work.
    Cancel,
    /// Retry a failed, cancelled, or interrupted run from its original request.
    Retry,
    /// Record that the user accepts the completed managed deliverable.
    Accept,
    /// Commit the exact deliverable paths in the managed worktree.
    Commit,
    /// Export an exact patch artifact for the deliverable.
    Export,
    /// Revert and remove the exact deliverable paths.
    Discard,
}

impl ProductRunControlAction {
    /// Stable wire tag.
    #[must_use]
    pub const fn tag(self) -> u16 {
        match self {
            Self::Cancel => 1,
            Self::Retry => 2,
            Self::Accept => 3,
            Self::Commit => 4,
            Self::Export => 5,
            Self::Discard => 6,
        }
    }

    /// Decodes a stable wire tag.
    #[must_use]
    pub const fn from_tag(tag: u16) -> Option<Self> {
        match tag {
            1 => Some(Self::Cancel),
            2 => Some(Self::Retry),
            3 => Some(Self::Accept),
            4 => Some(Self::Commit),
            5 => Some(Self::Export),
            6 => Some(Self::Discard),
            _ => None,
        }
    }
}

/// Control request for one exact coding run.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProductRunControl {
    run_id: RunId,
    action: ProductRunControlAction,
}

impl ProductRunControl {
    /// Creates an exact run control request.
    #[must_use]
    pub const fn new(run_id: RunId, action: ProductRunControlAction) -> Self {
        Self { run_id, action }
    }
    /// Target run.
    #[must_use]
    pub const fn run_id(self) -> RunId {
        self.run_id
    }
    /// Requested action.
    #[must_use]
    pub const fn action(self) -> ProductRunControlAction {
        self.action
    }
}
