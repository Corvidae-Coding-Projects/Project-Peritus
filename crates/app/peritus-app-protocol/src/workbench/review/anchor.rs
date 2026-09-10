//! Exact content- and lineage-bound structured-review anchors.

use super::{malformed, validate_path};
use crate::{AppProtocolError, WorkbenchQuery};
use peritus_types::{RunId, Sha256Digest, WorkspaceId};

/// Complete file selection or one exact unified-diff hunk.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkbenchReviewTarget {
    /// Complete file change.
    File,
    /// Exact parsed hunk.
    Hunk,
}

impl WorkbenchReviewTarget {
    /// Stable canonical tag.
    #[must_use]
    pub const fn tag(self) -> u16 {
        match self {
            Self::File => 1,
            Self::Hunk => 2,
        }
    }
    /// Decodes a stable canonical tag.
    #[must_use]
    pub const fn from_tag(tag: u16) -> Option<Self> {
        match tag {
            1 => Some(Self::File),
            2 => Some(Self::Hunk),
            _ => None,
        }
    }
}

/// Exact old/new line range; file targets use the canonical all-zero sentinel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchReviewRange {
    old_start: u32,
    old_lines: u32,
    new_start: u32,
    new_lines: u32,
}

impl WorkbenchReviewRange {
    /// Creates one nonempty checked hunk range.
    ///
    /// # Errors
    /// Rejects empty ranges or positive counts paired with line zero.
    pub const fn hunk(
        old_start: u32,
        old_lines: u32,
        new_start: u32,
        new_lines: u32,
    ) -> Result<Self, AppProtocolError> {
        if old_lines == 0 && new_lines == 0
            || old_lines > 0 && old_start == 0
            || new_lines > 0 && new_start == 0
        {
            Err(malformed())
        } else {
            Ok(Self { old_start, old_lines, new_start, new_lines })
        }
    }
    /// Canonical complete-file range sentinel.
    #[must_use]
    pub const fn file() -> Self {
        Self { old_start: 0, old_lines: 0, new_start: 0, new_lines: 0 }
    }
    /// Old-image first line.
    #[must_use]
    pub const fn old_start(self) -> u32 {
        self.old_start
    }
    /// Old-image line count.
    #[must_use]
    pub const fn old_lines(self) -> u32 {
        self.old_lines
    }
    /// New-image first line.
    #[must_use]
    pub const fn new_start(self) -> u32 {
        self.new_start
    }
    /// New-image line count.
    #[must_use]
    pub const fn new_lines(self) -> u32 {
        self.new_lines
    }

    fn valid_for(self, target: WorkbenchReviewTarget) -> bool {
        match target {
            WorkbenchReviewTarget::File => self == Self::file(),
            WorkbenchReviewTarget::Hunk => {
                Self::hunk(self.old_start, self.old_lines, self.new_start, self.new_lines).is_ok()
            }
        }
    }
}

/// Immutable structured-review identity copied back verbatim by mutation requests.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchReviewAnchor {
    run: RunId,
    workspace: WorkspaceId,
    candidate_digest: Sha256Digest,
    diff_digest: Sha256Digest,
    path: String,
    before_blob_digest: Sha256Digest,
    after_blob_digest: Sha256Digest,
    context_digest: Sha256Digest,
    target: WorkbenchReviewTarget,
    range: WorkbenchReviewRange,
}

impl WorkbenchReviewAnchor {
    /// Creates one fully content- and lineage-bound target.
    ///
    /// # Errors
    /// Rejects unsafe paths or range/target mismatches.
    #[allow(clippy::too_many_arguments, reason = "freshness bindings must remain independent")]
    pub fn new(
        run: RunId,
        workspace: WorkspaceId,
        candidate_digest: Sha256Digest,
        diff_digest: Sha256Digest,
        path: String,
        before_blob_digest: Sha256Digest,
        after_blob_digest: Sha256Digest,
        context_digest: Sha256Digest,
        target: WorkbenchReviewTarget,
        range: WorkbenchReviewRange,
    ) -> Result<Self, AppProtocolError> {
        validate_path(&path)?;
        if !range.valid_for(target) {
            return Err(malformed());
        }
        Ok(Self {
            run,
            workspace,
            candidate_digest,
            diff_digest,
            path,
            before_blob_digest,
            after_blob_digest,
            context_digest,
            target,
            range,
        })
    }
    /// Governing run lineage.
    #[must_use]
    pub const fn run(&self) -> RunId {
        self.run
    }
    /// Governing workspace lineage.
    #[must_use]
    pub const fn workspace(&self) -> WorkspaceId {
        self.workspace
    }
    /// Exact complete candidate identity.
    #[must_use]
    pub const fn candidate_digest(&self) -> Sha256Digest {
        self.candidate_digest
    }
    /// Exact raw-diff identity.
    #[must_use]
    pub const fn diff_digest(&self) -> Sha256Digest {
        self.diff_digest
    }
    /// Relative file path.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }
    /// Canonical old target blob identity.
    #[must_use]
    pub const fn before_blob_digest(&self) -> Sha256Digest {
        self.before_blob_digest
    }
    /// Canonical new target blob identity.
    #[must_use]
    pub const fn after_blob_digest(&self) -> Sha256Digest {
        self.after_blob_digest
    }
    /// Exact hunk/file context identity.
    #[must_use]
    pub const fn context_digest(&self) -> Sha256Digest {
        self.context_digest
    }
    /// File or hunk target.
    #[must_use]
    pub const fn target(&self) -> WorkbenchReviewTarget {
        self.target
    }
    /// Exact display range.
    #[must_use]
    pub const fn range(&self) -> WorkbenchReviewRange {
        self.range
    }
}

/// Typed conversational meaning; only request-revision denotes mutation work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkbenchReviewFeedback {
    /// Read-only source-grounded explanation.
    Explain,
    /// Explicit ordinary revision request.
    RequestRevision,
    /// Semantic preference, not a protected path.
    KeepBehavior,
    /// Hard target constraint enforced below the model.
    LeaveAlone,
}

impl WorkbenchReviewFeedback {
    /// Stable canonical tag.
    #[must_use]
    pub const fn tag(self) -> u16 {
        match self {
            Self::Explain => 1,
            Self::RequestRevision => 2,
            Self::KeepBehavior => 3,
            Self::LeaveAlone => 4,
        }
    }
    /// Decodes a stable canonical tag.
    #[must_use]
    pub const fn from_tag(tag: u16) -> Option<Self> {
        match tag {
            1 => Some(Self::Explain),
            2 => Some(Self::RequestRevision),
            3 => Some(Self::KeepBehavior),
            4 => Some(Self::LeaveAlone),
            _ => None,
        }
    }
}

/// One revision-fenced structured review query.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchReviewQuery {
    query: WorkbenchQuery,
    run: RunId,
    revision: u64,
    offset: u32,
}

impl WorkbenchReviewQuery {
    /// Constructs a query. Revision zero requests the current page.
    #[must_use]
    pub const fn new(query: WorkbenchQuery, run: RunId, revision: u64, offset: u32) -> Self {
        Self { query, run, revision, offset }
    }
    /// Conversation/workspace scope.
    #[must_use]
    pub const fn query(self) -> WorkbenchQuery {
        self.query
    }
    /// Exact selected run.
    #[must_use]
    pub const fn run(self) -> RunId {
        self.run
    }
    /// Inspected aggregate revision or zero for latest.
    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision
    }
    /// Comment-page offset.
    #[must_use]
    pub const fn offset(self) -> u32 {
        self.offset
    }
}
