//! Structured Git observations anchored to C1 immutable identities.

use peritus_git::{
    CandidateSnapshot, CommitObservation, DiffEntry, GitDiffObservation, GitHistoryObservation,
    StatusEntry, StatusObservation,
};
use peritus_types::{Generation, RevisionNumber, Sha256Digest, SnapshotId, WorkspaceId};
use peritus_workspace::ReadOnlyWorkspace;

use crate::{
    DiffInput, GitToolError, GitToolErrorKind, GitToolOperation, HistoryInput, RecoveryClass,
    SnapshotInput, StatusInput, StatusPageInput,
    cursor::{
        PageCursor, PageKind, diff_request, history_request, snapshot_binding, status_request,
    },
};

const PAGE_ENTRIES: usize = 500;
const PAGE_PATCH_BYTES: usize = 48 * 1_024;
const PAGE_PARENTS: usize = 32;

/// Exact current C1 immutable snapshot observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotObservation {
    workspace_id: WorkspaceId,
    generation: Generation,
    revision: RevisionNumber,
    commit: peritus_git::CommitId,
    tree: peritus_git::TreeId,
    digest: Sha256Digest,
}

impl SnapshotObservation {
    /// Returns the owning workspace lineage.
    #[must_use]
    pub const fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }
    /// Returns the fenced generation.
    #[must_use]
    pub const fn generation(&self) -> Generation {
        self.generation
    }
    /// Returns the logical revision.
    #[must_use]
    pub const fn revision(&self) -> RevisionNumber {
        self.revision
    }
    /// Returns the immutable commit.
    #[must_use]
    pub const fn commit(&self) -> peritus_git::CommitId {
        self.commit
    }
    /// Returns the immutable root tree.
    #[must_use]
    pub const fn tree(&self) -> peritus_git::TreeId {
        self.tree
    }
    /// Returns the canonical snapshot observation digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

/// Exact retained C1 candidate-snapshot observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetainedSnapshotObservation {
    workspace_id: WorkspaceId,
    snapshot_id: SnapshotId,
    commit: peritus_git::CommitId,
    tree: peritus_git::TreeId,
    reference: String,
    manifest_digest: Sha256Digest,
}

impl RetainedSnapshotObservation {
    /// Returns the owning workspace lineage.
    #[must_use]
    pub const fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }
    /// Returns the stable retained snapshot identity.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }
    /// Returns the immutable retained commit.
    #[must_use]
    pub const fn commit(&self) -> peritus_git::CommitId {
        self.commit
    }
    /// Returns the immutable retained tree.
    #[must_use]
    pub const fn tree(&self) -> peritus_git::TreeId {
        self.tree
    }
    /// Returns the protected Peritus snapshot reference.
    #[must_use]
    pub fn reference(&self) -> &str {
        &self.reference
    }
    /// Returns the complete candidate-snapshot manifest digest.
    #[must_use]
    pub const fn manifest_digest(&self) -> Sha256Digest {
        self.manifest_digest
    }
}

#[derive(Clone, Copy)]
struct PageBinding {
    snapshot: Sha256Digest,
    request: Sha256Digest,
    observation: Sha256Digest,
}

impl PageBinding {
    const fn cursor(self, first: u64, second: u64) -> PageCursor {
        PageCursor::new(self.snapshot, self.request, self.observation, first, second)
    }
}

/// One bounded physical page of a complete immutable status observation.
pub struct StatusObservationPage {
    observation: StatusObservation,
    binding: PageBinding,
    start: usize,
    end: usize,
    cursor: String,
    next_cursor: Option<String>,
}

impl StatusObservationPage {
    /// Borrows the complete digest-bound observation metadata.
    #[must_use]
    pub const fn observation(&self) -> &StatusObservation {
        &self.observation
    }
    /// Borrows only the entries carried by this physical page.
    #[must_use]
    pub fn entries(&self) -> &[StatusEntry] {
        &self.observation.entries()[self.start..self.end]
    }
    /// Returns the inclusive page start in the complete entry sequence.
    #[must_use]
    pub fn entry_start(&self) -> u64 {
        u64::try_from(self.start).expect("bounded status entry offset fits u64")
    }
    /// Returns the exclusive page end in the complete entry sequence.
    #[must_use]
    pub fn entry_end(&self) -> u64 {
        u64::try_from(self.end).expect("bounded status entry offset fits u64")
    }
    /// Returns the opaque replay cursor for this exact page.
    #[must_use]
    pub fn cursor(&self) -> &str {
        &self.cursor
    }
    /// Returns the next exact physical page when entries remain.
    #[must_use]
    pub fn next_cursor(&self) -> Option<&str> {
        self.next_cursor.as_deref()
    }

    pub(crate) fn narrow_result_page(&mut self) -> bool {
        let count = self.end - self.start;
        if count <= 1 {
            return false;
        }
        self.end = self.start + (count / 2).max(1);
        self.refresh_next();
        true
    }

    fn refresh_next(&mut self) {
        self.next_cursor = (self.end < self.observation.entries().len()).then(|| {
            self.binding
                .cursor(u64_index(self.end), 0)
                .encode(PageKind::Status)
        });
    }
}

/// One bounded physical page of a complete immutable diff observation.
pub struct DiffObservationPage {
    observation: GitDiffObservation,
    binding: PageBinding,
    entry_start: usize,
    entry_end: usize,
    patch_start: usize,
    patch_end: usize,
    cursor: String,
    next_cursor: Option<String>,
}

impl DiffObservationPage {
    /// Borrows the complete digest-bound observation metadata.
    #[must_use]
    pub const fn observation(&self) -> &GitDiffObservation {
        &self.observation
    }
    /// Borrows changed paths carried by this page.
    #[must_use]
    pub fn entries(&self) -> &[DiffEntry] {
        &self.observation.entries()[self.entry_start..self.entry_end]
    }
    /// Borrows the exact patch byte range carried by this page.
    #[must_use]
    pub fn patch(&self) -> &[u8] {
        &self.observation.patch()[self.patch_start..self.patch_end]
    }
    /// Returns the changed-path range start.
    #[must_use]
    pub fn entry_start(&self) -> u64 {
        u64_index(self.entry_start)
    }
    /// Returns the changed-path range end.
    #[must_use]
    pub fn entry_end(&self) -> u64 {
        u64_index(self.entry_end)
    }
    /// Returns the patch byte range start.
    #[must_use]
    pub fn patch_start(&self) -> u64 {
        u64_index(self.patch_start)
    }
    /// Returns the patch byte range end.
    #[must_use]
    pub fn patch_end(&self) -> u64 {
        u64_index(self.patch_end)
    }
    /// Returns the opaque replay cursor for this exact page.
    #[must_use]
    pub fn cursor(&self) -> &str {
        &self.cursor
    }
    /// Returns the next exact physical page when paths or patch bytes remain.
    #[must_use]
    pub fn next_cursor(&self) -> Option<&str> {
        self.next_cursor.as_deref()
    }

    pub(crate) fn narrow_result_page(&mut self) -> bool {
        let entries = self.entry_end - self.entry_start;
        let patch = self.patch_end - self.patch_start;
        if entries > 1 {
            self.entry_end = self.entry_start + (entries / 2).max(1);
        } else if patch > 1 {
            self.patch_end = self.patch_start + (patch / 2).max(1);
        } else {
            return false;
        }
        self.refresh_next();
        true
    }

    fn refresh_next(&mut self) {
        let entry_total = self.observation.entries().len();
        let patch_total = self.observation.patch().len();
        self.next_cursor = if self.entry_end < entry_total {
            Some(
                self.binding
                    .cursor(u64_index(self.entry_end), u64_index(self.patch_start))
                    .encode(PageKind::Diff),
            )
        } else if self.patch_end < patch_total {
            Some(
                self.binding
                    .cursor(u64_index(entry_total), u64_index(self.patch_end))
                    .encode(PageKind::Diff),
            )
        } else {
            None
        };
    }
}

/// One bounded physical commit/parent page of a complete immutable history observation.
pub struct HistoryObservationPage {
    observation: GitHistoryObservation,
    binding: PageBinding,
    commit_index: usize,
    parent_start: usize,
    parent_end: usize,
    cursor: String,
    next_cursor: Option<String>,
}

impl HistoryObservationPage {
    /// Borrows the complete digest-bound observation metadata.
    #[must_use]
    pub const fn observation(&self) -> &GitHistoryObservation {
        &self.observation
    }
    /// Borrows the commit represented by this page, if the complete history is nonempty.
    #[must_use]
    pub fn commit(&self) -> Option<&CommitObservation> {
        self.observation.commits().get(self.commit_index)
    }
    /// Borrows only the ordered parent identities carried by this page.
    #[must_use]
    pub fn parents(&self) -> &[peritus_git::CommitId] {
        self.commit().map_or(&[][..], |commit| {
            &commit.parents()[self.parent_start..self.parent_end]
        })
    }
    /// Returns the zero-based commit position represented by this page.
    #[must_use]
    pub fn commit_index(&self) -> u64 {
        u64_index(self.commit_index)
    }
    /// Returns the parent range start for the represented commit.
    #[must_use]
    pub fn parent_start(&self) -> u64 {
        u64_index(self.parent_start)
    }
    /// Returns the parent range end for the represented commit.
    #[must_use]
    pub fn parent_end(&self) -> u64 {
        u64_index(self.parent_end)
    }
    /// Returns the opaque replay cursor for this exact page.
    #[must_use]
    pub fn cursor(&self) -> &str {
        &self.cursor
    }
    /// Returns the next exact commit or parent page when history remains.
    #[must_use]
    pub fn next_cursor(&self) -> Option<&str> {
        self.next_cursor.as_deref()
    }

    pub(crate) fn narrow_result_page(&mut self) -> bool {
        let parents = self.parent_end - self.parent_start;
        if parents <= 1 {
            return false;
        }
        self.parent_end = self.parent_start + (parents / 2).max(1);
        self.refresh_next();
        true
    }

    fn refresh_next(&mut self) {
        let Some(parent_total) = self
            .observation
            .commits()
            .get(self.commit_index)
            .map(|commit| commit.parents().len())
        else {
            self.next_cursor = None;
            return;
        };
        self.next_cursor = if self.parent_end < parent_total {
            Some(
                self.binding
                    .cursor(u64_index(self.commit_index), u64_index(self.parent_end))
                    .encode(PageKind::History),
            )
        } else if self.commit_index + 1 < self.observation.commits().len() {
            Some(
                self.binding
                    .cursor(u64_index(self.commit_index + 1), 0)
                    .encode(PageKind::History),
            )
        } else {
            None
        };
    }
}

/// Read-only Git service fixed to one C1 immutable snapshot handle.
pub struct GitReadService<'a> {
    workspace: &'a ReadOnlyWorkspace,
}

impl<'a> GitReadService<'a> {
    /// Binds structured Git observations to one checked immutable C1 handle.
    #[must_use]
    pub const fn new(workspace: &'a ReadOnlyWorkspace) -> Self {
        Self { workspace }
    }

    /// Observes exact structured status.
    ///
    /// # Errors
    /// Returns a typed C1 Git observation failure.
    pub fn status(&self, _input: StatusInput) -> Result<StatusObservation, GitToolError> {
        self.workspace.inspect().map_err(|error| git_error(GitToolOperation::Status, &error))
    }

    /// Observes one exact physical page of complete immutable status.
    ///
    /// # Errors
    /// Rejects observation drift, a cursor for another source/request, or invalid offsets.
    pub fn status_page(
        &self,
        input: StatusPageInput,
    ) -> Result<StatusObservationPage, GitToolError> {
        let observation = self.status(StatusInput)?;
        let binding = PageBinding {
            snapshot: snapshot_binding(self.workspace),
            request: status_request(),
            observation: observation.digest(),
        };
        let start = page_start(
            input.cursor,
            binding,
            PageKind::Status,
            GitToolOperation::Status,
        )?;
        if start.1 != 0
            || start.0 > observation.entries().len()
            || (input.cursor.is_some() && start.0 == observation.entries().len())
        {
            return Err(cursor_error(GitToolOperation::Status));
        }
        let end = start.0.saturating_add(PAGE_ENTRIES).min(observation.entries().len());
        let cursor = binding.cursor(u64_index(start.0), 0).encode(PageKind::Status);
        let mut page = StatusObservationPage {
            observation,
            binding,
            start: start.0,
            end,
            cursor,
            next_cursor: None,
        };
        page.refresh_next();
        Ok(page)
    }

    /// Observes a bounded diff from a resolved baseline to this immutable snapshot.
    ///
    /// # Errors
    /// Returns a typed C1 Git observation failure.
    pub fn diff(&self, input: &DiffInput) -> Result<GitDiffObservation, GitToolError> {
        self.workspace
            .git_diff(&input.base_revision, input.maximum_entries, input.maximum_patch_bytes)
            .map_err(|error| git_error(GitToolOperation::Diff, &error))
    }

    /// Observes one exact physical page of a complete immutable diff.
    ///
    /// Changed paths are exhausted before exact patch ranges begin, so no page repeats the first
    /// path prefix or patch prefix.
    pub fn diff_page(&self, input: &DiffInput) -> Result<DiffObservationPage, GitToolError> {
        let observation = self.diff(input)?;
        let binding = PageBinding {
            snapshot: snapshot_binding(self.workspace),
            request: diff_request(
                &input.base_revision,
                input.maximum_entries,
                input.maximum_patch_bytes,
            ),
            observation: observation.digest(),
        };
        let start = page_start(input.cursor, binding, PageKind::Diff, GitToolOperation::Diff)?;
        let entry_total = observation.entries().len();
        let patch_total = observation.patch().len();
        if start.0 > entry_total
            || start.1 > patch_total
            || (start.0 < entry_total && start.1 != 0)
            || (input.cursor.is_some() && start.0 == entry_total && start.1 == patch_total)
        {
            return Err(cursor_error(GitToolOperation::Diff));
        }
        let (entry_end, patch_end) = if start.0 < entry_total {
            (start.0.saturating_add(PAGE_ENTRIES).min(entry_total), start.1)
        } else {
            (entry_total, start.1.saturating_add(PAGE_PATCH_BYTES).min(patch_total))
        };
        let cursor = binding
            .cursor(u64_index(start.0), u64_index(start.1))
            .encode(PageKind::Diff);
        let mut page = DiffObservationPage {
            observation,
            binding,
            entry_start: start.0,
            entry_end,
            patch_start: start.1,
            patch_end,
            cursor,
            next_cursor: None,
        };
        page.refresh_next();
        Ok(page)
    }

    /// Observes bounded history from this immutable snapshot.
    ///
    /// # Errors
    /// Returns a typed C1 Git observation failure.
    pub fn history(&self, input: HistoryInput) -> Result<GitHistoryObservation, GitToolError> {
        self.workspace
            .git_history(input.maximum_commits)
            .map_err(|error| git_error(GitToolOperation::History, &error))
    }

    /// Observes one exact physical commit/parent page of immutable history.
    ///
    /// # Errors
    /// Rejects observation drift, a cursor for another source/request, or invalid offsets.
    pub fn history_page(
        &self,
        input: HistoryInput,
    ) -> Result<HistoryObservationPage, GitToolError> {
        let observation = self.history(input)?;
        let binding = PageBinding {
            snapshot: snapshot_binding(self.workspace),
            request: history_request(input.maximum_commits),
            observation: observation.digest(),
        };
        let start = page_start(
            input.cursor,
            binding,
            PageKind::History,
            GitToolOperation::History,
        )?;
        if start.0 > observation.commits().len()
            || (input.cursor.is_some() && start.0 == observation.commits().len())
        {
            return Err(cursor_error(GitToolOperation::History));
        }
        let parent_total = observation
            .commits()
            .get(start.0)
            .map_or(0, |commit| commit.parents().len());
        if start.1 > parent_total || (parent_total > 0 && start.1 == parent_total) {
            return Err(cursor_error(GitToolOperation::History));
        }
        let parent_end = start.1.saturating_add(PAGE_PARENTS).min(parent_total);
        let cursor = binding
            .cursor(u64_index(start.0), u64_index(start.1))
            .encode(PageKind::History);
        let mut page = HistoryObservationPage {
            observation,
            binding,
            commit_index: start.0,
            parent_start: start.1,
            parent_end,
            cursor,
            next_cursor: None,
        };
        page.refresh_next();
        Ok(page)
    }

    /// Observes the current immutable C1 snapshot identity.
    #[must_use]
    pub fn current_snapshot(&self) -> SnapshotObservation {
        let snapshot = self.workspace.snapshot();
        let mut bytes = b"PERITUS-GIT-TOOL-SNAPSHOT-V1\0".to_vec();
        bytes.extend_from_slice(snapshot.workspace_id().as_bytes());
        bytes.extend_from_slice(&snapshot.generation().get().to_be_bytes());
        bytes.extend_from_slice(&snapshot.revision().get().to_be_bytes());
        bytes.extend_from_slice(snapshot.commit().object_id().as_bytes());
        bytes.extend_from_slice(snapshot.tree().object_id().as_bytes());
        SnapshotObservation {
            workspace_id: snapshot.workspace_id(),
            generation: snapshot.generation(),
            revision: snapshot.revision(),
            commit: snapshot.commit(),
            tree: snapshot.tree(),
            digest: peritus_codec::sha256(&bytes),
        }
    }

    /// Projects one already-resolved retained C1 candidate snapshot.
    ///
    /// # Errors
    /// Rejects an identity or lineage different from the exact selector/current workspace.
    pub fn retained_snapshot(
        &self,
        input: SnapshotInput,
        retained: &CandidateSnapshot,
    ) -> Result<RetainedSnapshotObservation, GitToolError> {
        let SnapshotInput::Retained(expected) = input else {
            return Err(GitToolError::invalid(
                GitToolOperation::Snapshot,
                "current snapshot selection does not accept retained metadata",
            ));
        };
        if expected != retained.snapshot_id()
            || retained.workspace_id() != self.workspace.snapshot().workspace_id()
        {
            return Err(GitToolError::invalid(
                GitToolOperation::Snapshot,
                "retained snapshot identity or workspace lineage differs",
            ));
        }
        Ok(RetainedSnapshotObservation {
            workspace_id: retained.workspace_id(),
            snapshot_id: retained.snapshot_id(),
            commit: retained.commit(),
            tree: retained.tree(),
            reference: retained.reference().as_str().to_owned(),
            manifest_digest: retained.manifest_digest(),
        })
    }

    /// Reports unavailable branch delivery without invoking Git or mutating a reference.
    ///
    /// # Errors
    /// Always returns the frozen typed unsupported result until C1 owns merge delivery.
    pub const fn merge_unsupported(&self) -> Result<(), GitToolError> {
        Err(GitToolError::new(
            GitToolErrorKind::Unsupported,
            GitToolOperation::Merge,
            RecoveryClass::SelectSupportedOperation,
            "C1 has no authorized merge-delivery operation",
        ))
    }
}

fn page_start(
    cursor: Option<PageCursor>,
    binding: PageBinding,
    _kind: PageKind,
    operation: GitToolOperation,
) -> Result<(usize, usize), GitToolError> {
    let Some(cursor) = cursor else { return Ok((0, 0)) };
    if cursor.snapshot != binding.snapshot
        || cursor.request != binding.request
        || cursor.observation != binding.observation
    {
        return Err(cursor_error(operation));
    }
    let first = usize::try_from(cursor.first).map_err(|_| cursor_error(operation))?;
    let second = usize::try_from(cursor.second).map_err(|_| cursor_error(operation))?;
    Ok((first, second))
}

fn u64_index(value: usize) -> u64 {
    u64::try_from(value).expect("bounded Git observation offset fits u64")
}

const fn cursor_error(operation: GitToolOperation) -> GitToolError {
    GitToolError::new(
        GitToolErrorKind::InvalidInput,
        operation,
        RecoveryClass::Reobserve,
        "Git continuation differs from its immutable source, request, or observation",
    )
}

const fn git_error(operation: GitToolOperation, error: &peritus_git::GitError) -> GitToolError {
    let recovery = match error.recovery() {
        peritus_git::RecoveryClass::CorrectRequest => RecoveryClass::CorrectInput,
        peritus_git::RecoveryClass::Reobserve | peritus_git::RecoveryClass::Retry => {
            RecoveryClass::Reobserve
        }
        peritus_git::RecoveryClass::Reconcile | peritus_git::RecoveryClass::Quarantine => {
            RecoveryClass::Reconcile
        }
    };
    GitToolError::new(
        GitToolErrorKind::Git,
        operation,
        recovery,
        "structured C1 Git observation failed",
    )
}
