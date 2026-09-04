//! Exact candidate identity inspection for external handoff controls.

use std::path::Path;

use peritus_types::Sha256Digest;

use super::ProductRunner;
use crate::{ProductRunnerError, progress::WorkspaceCheckpoint};

impl ProductRunner {
    /// Computes the exact current candidate digest used by checkpoint identity.
    ///
    /// # Errors
    ///
    /// Returns a repository error when the managed worktree cannot be inspected completely.
    pub fn candidate_digest(workspace_root: &Path) -> Result<Sha256Digest, ProductRunnerError> {
        WorkspaceCheckpoint::capture(workspace_root).map(|checkpoint| checkpoint.digest())
    }
}
