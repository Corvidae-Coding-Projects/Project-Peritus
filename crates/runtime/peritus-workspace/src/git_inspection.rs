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
        self.git_diff_page(
            base_revision,
            maximum_entries,
            maximum_patch_bytes,
            peritus_git::DiffCursor::default(),
            None,
        )
    }

    /// Reads a page bound to an exact immutable diff identity and source offsets.
    ///
    /// # Errors
    /// Rejects stale identities, invalid coordinates, or Git observation failures.
    pub fn git_diff_page(
        &self,
        base_revision: &str,
        maximum_entries: u32,
        maximum_patch_bytes: u64,
        cursor: peritus_git::DiffCursor,
        digest: Option<peritus_types::Sha256Digest>,
    ) -> Result<GitDiffObservation, GitError> {
        let base = self.repository().resolve_baseline(base_revision)?.commit();
        self.repository().diff(
            DiffRequest::new(
                self.worktree(),
                base,
                self.snapshot().commit(),
                maximum_entries,
                maximum_patch_bytes,
            )?
            .with_cursor(cursor, digest),
        )
    }

    /// Observes bounded history starting at this exact immutable snapshot.
    ///
    /// # Errors
    /// Returns a typed Git failure when registration or bounded history observation fails.
    pub fn git_history(&self, maximum_commits: u16) -> Result<GitHistoryObservation, GitError> {
        self.git_history_page(maximum_commits, 0, 0, 0)
    }

    /// Observes one exact commit-offset page of history from this immutable snapshot.
    ///
    /// # Errors
    /// Returns a typed Git failure when registration or bounded history observation fails.
    pub fn git_history_page(
        &self,
        maximum_commits: u16,
        offset: u64,
        parent_offset: u32,
        subject_offset: u32,
    ) -> Result<GitHistoryObservation, GitError> {
        self.repository().history(
            HistoryRequest::page(
                self.worktree(),
                self.snapshot().commit(),
                maximum_commits,
                offset,
            )?
            .with_parent_offset(parent_offset)
            .with_subject_offset(subject_offset),
        )
    }
}
