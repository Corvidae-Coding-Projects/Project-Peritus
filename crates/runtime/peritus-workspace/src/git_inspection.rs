//! Structured Git observations anchored to an immutable workspace snapshot.

use peritus_git::{
    DiffRequest, GitDiffObservation, GitError, GitHistoryObservation, HistoryRequest,
};

use crate::ReadOnlyWorkspace;

impl ReadOnlyWorkspace {
    /// Observes a bounded diff from one resolved baseline to this exact snapshot.
    ///
    /// # Errors
    /// Returns a typed Git failure when the baseline, registration, or bounded output is invalid.
    pub fn git_diff(
        &self,
        base_revision: &str,
        maximum_entries: u32,
        maximum_patch_bytes: u64,
    ) -> Result<GitDiffObservation, GitError> {
        let base = self.repository().resolve_baseline(base_revision)?.commit();
        self.repository().diff(DiffRequest::new(
            self.worktree(),
            base,
            self.snapshot().commit(),
            maximum_entries,
            maximum_patch_bytes,
        )?)
    }

    /// Observes bounded history starting at this exact immutable snapshot.
    ///
    /// # Errors
    /// Returns a typed Git failure when registration or bounded history observation fails.
    pub fn git_history(&self, maximum_commits: u16) -> Result<GitHistoryObservation, GitError> {
        self.repository().history(HistoryRequest::new(
            self.worktree(),
            self.snapshot().commit(),
            maximum_commits,
        )?)
    }
}
