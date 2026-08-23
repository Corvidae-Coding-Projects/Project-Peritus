//! Reusable C1 Git, patch, and workspace conformance contract.

mod cases;

pub use cases::workspace_suite;

/// Stable subject failure classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceConformanceError {
    /// The real Git or filesystem boundary could not complete.
    Infrastructure,
}

/// Result of requesting one workspace mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceMutationDisposition {
    /// The exact authorized patch was installed.
    Applied,
    /// The requested generation or revision was stale.
    Stale,
    /// The action or resource was not authorized.
    Unauthorized,
    /// The target was an immutable read-only snapshot.
    ReadOnly,
}

/// Typed restart-reconciliation result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceReconciliationDisposition {
    /// Git, files, and transaction state match the last snapshot.
    Clean,
    /// Observable local changes require explicit handling.
    Dirty,
    /// The requested correlation names a generation that is no longer current.
    Fenced,
    /// The subject could not prove either clean or dirty safely.
    Indeterminate,
}

/// Fixed two-file patch input supplied by the C1 conformance suite.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspacePatchFixture {
    workspace_id: [u8; 16],
    resource_id: [u8; 16],
    generation: u64,
    revision: u64,
    first_path: &'static str,
    first_contents: &'static [u8],
    second_path: &'static str,
    second_contents: &'static [u8],
}

impl WorkspacePatchFixture {
    /// Returns the exact workspace identity.
    #[must_use]
    pub const fn workspace_id(&self) -> [u8; 16] {
        self.workspace_id
    }

    /// Returns the exact capability-addressable resource identity.
    #[must_use]
    pub const fn resource_id(&self) -> [u8; 16] {
        self.resource_id
    }

    /// Returns the expected workspace generation.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Returns the expected workspace revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the first relative target.
    #[must_use]
    pub const fn first_path(&self) -> &'static str {
        self.first_path
    }

    /// Returns the first target bytes.
    #[must_use]
    pub const fn first_contents(&self) -> &'static [u8] {
        self.first_contents
    }

    /// Returns the second relative target.
    #[must_use]
    pub const fn second_path(&self) -> &'static str {
        self.second_path
    }

    /// Returns the second target bytes.
    #[must_use]
    pub const fn second_contents(&self) -> &'static [u8] {
        self.second_contents
    }
}

/// Exact observable workspace state used by the conformance cases.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceSnapshot {
    generation: u64,
    revision: u64,
    tree_id: Vec<u8>,
    first_contents: Option<Vec<u8>>,
    second_contents: Option<Vec<u8>>,
    user_ref_unchanged: bool,
    manifest_finalized: bool,
    prior_candidate_retained: bool,
}

impl WorkspaceSnapshot {
    /// Creates one complete subject observation.
    #[must_use]
    #[allow(clippy::too_many_arguments, reason = "the conformance observation remains explicit")]
    pub const fn new(
        generation: u64,
        revision: u64,
        tree_id: Vec<u8>,
        first_contents: Option<Vec<u8>>,
        second_contents: Option<Vec<u8>>,
        user_ref_unchanged: bool,
        manifest_finalized: bool,
        prior_candidate_retained: bool,
    ) -> Self {
        Self {
            generation,
            revision,
            tree_id,
            first_contents,
            second_contents,
            user_ref_unchanged,
            manifest_finalized,
            prior_candidate_retained,
        }
    }

    /// Returns the observed generation.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Returns the observed revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Borrows the exact Git tree identifier bytes.
    #[must_use]
    pub fn tree_id(&self) -> &[u8] {
        &self.tree_id
    }

    /// Borrows the first file, or absence.
    #[must_use]
    pub fn first_contents(&self) -> Option<&[u8]> {
        self.first_contents.as_deref()
    }

    /// Borrows the second file, or absence.
    #[must_use]
    pub fn second_contents(&self) -> Option<&[u8]> {
        self.second_contents.as_deref()
    }

    /// Returns whether the user's branch ref stayed unchanged.
    #[must_use]
    pub const fn user_ref_unchanged(&self) -> bool {
        self.user_ref_unchanged
    }

    /// Returns whether the candidate manifest reached finalized artifact storage.
    #[must_use]
    pub const fn manifest_finalized(&self) -> bool {
        self.manifest_finalized
    }

    /// Returns whether rollback retained the superseded candidate.
    #[must_use]
    pub const fn prior_candidate_retained(&self) -> bool {
        self.prior_candidate_retained
    }
}

/// Observable mutation result paired with the resulting workspace snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceMutationObservation {
    disposition: WorkspaceMutationDisposition,
    snapshot: WorkspaceSnapshot,
}

impl WorkspaceMutationObservation {
    /// Creates a mutation observation supplied by a production adapter.
    #[must_use]
    pub const fn new(
        disposition: WorkspaceMutationDisposition,
        snapshot: WorkspaceSnapshot,
    ) -> Self {
        Self { disposition, snapshot }
    }

    /// Returns the stable mutation classification.
    #[must_use]
    pub const fn disposition(&self) -> WorkspaceMutationDisposition {
        self.disposition
    }

    /// Borrows the complete state observed after the request.
    #[must_use]
    pub const fn snapshot(&self) -> &WorkspaceSnapshot {
        &self.snapshot
    }
}

/// Adapter implemented by a production C1 workspace under conformance test.
pub trait WorkspaceConformanceSubject: Send {
    /// Reads the current exact state.
    ///
    /// # Errors
    ///
    /// Returns [`WorkspaceConformanceError::Infrastructure`] when the state cannot be observed.
    fn snapshot(&self) -> Result<WorkspaceSnapshot, WorkspaceConformanceError>;

    /// Attempts an authorized writable patch.
    ///
    /// # Errors
    ///
    /// Returns [`WorkspaceConformanceError::Infrastructure`] when the request cannot be exercised.
    fn apply(
        &mut self,
        fixture: &WorkspacePatchFixture,
    ) -> Result<WorkspaceMutationObservation, WorkspaceConformanceError>;

    /// Attempts the same patch through a read-only snapshot surface.
    ///
    /// # Errors
    ///
    /// Returns [`WorkspaceConformanceError::Infrastructure`] when the request cannot be exercised.
    fn apply_read_only(
        &mut self,
        fixture: &WorkspacePatchFixture,
    ) -> Result<WorkspaceMutationObservation, WorkspaceConformanceError>;

    /// Restores the initial snapshot as a new logical revision.
    ///
    /// # Errors
    ///
    /// Returns [`WorkspaceConformanceError::Infrastructure`] when restoration cannot complete.
    fn rollback(&mut self) -> Result<WorkspaceSnapshot, WorkspaceConformanceError>;

    /// Closes and reopens the subject at a restart boundary.
    ///
    /// # Errors
    ///
    /// Returns [`WorkspaceConformanceError::Infrastructure`] when restart cannot complete.
    fn restart(&mut self) -> Result<(), WorkspaceConformanceError>;

    /// Makes one ordinary uncommitted workspace edit for reconciliation testing.
    ///
    /// # Errors
    ///
    /// Returns [`WorkspaceConformanceError::Infrastructure`] when the edit cannot be installed.
    fn make_dirty(&mut self) -> Result<(), WorkspaceConformanceError>;

    /// Makes the next reconciliation inspection incomplete or ambiguous.
    ///
    /// # Errors
    ///
    /// Returns [`WorkspaceConformanceError::Infrastructure`] when the condition cannot be created.
    fn make_indeterminate(&mut self) -> Result<(), WorkspaceConformanceError>;

    /// Classifies the current Git/filesystem/transaction state for one expected generation.
    ///
    /// # Errors
    ///
    /// Returns [`WorkspaceConformanceError::Infrastructure`] when inspection cannot complete.
    fn reconcile(
        &mut self,
        expected_generation: u64,
    ) -> Result<WorkspaceReconciliationDisposition, WorkspaceConformanceError>;
}
