//! Anchored review comments, evidence freshness, and bounded page projections.

use super::{
    MAX_WORKBENCH_DIFF_FILES, MAX_WORKBENCH_DIFF_HUNKS, MAX_WORKBENCH_DIFF_LINES,
    MAX_WORKBENCH_REVIEW_PAGE, WorkbenchDiffFile, WorkbenchReviewAnchor, WorkbenchReviewFeedback,
    WorkbenchReviewQuery, malformed,
};
use crate::{AppProtocolError, ControlOperationId, WorkbenchInputSelection, WorkbenchInputText};
use peritus_types::Sha256Digest;

/// Projected comment lifecycle against the current exact diff.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkbenchReviewCommentState {
    /// Exact anchor remains current and feedback is open.
    Open,
    /// A host-published reply addressed a read-only explanation request.
    Addressed,
    /// Anchor no longer exists exactly and requires explicit rebinding.
    Stale,
    /// User explicitly dismissed the comment or hard constraint.
    Dismissed,
}

impl WorkbenchReviewCommentState {
    /// Stable canonical tag.
    #[must_use]
    pub const fn tag(self) -> u16 {
        match self {
            Self::Open => 1,
            Self::Addressed => 2,
            Self::Stale => 3,
            Self::Dismissed => 4,
        }
    }
    /// Decodes a stable tag.
    #[must_use]
    pub const fn from_tag(tag: u16) -> Option<Self> {
        match tag {
            1 => Some(Self::Open),
            2 => Some(Self::Addressed),
            3 => Some(Self::Stale),
            4 => Some(Self::Dismissed),
            _ => None,
        }
    }
}

/// One bounded comment row; text is public user-authored input, never hidden reasoning.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchReviewComment {
    id: ControlOperationId,
    revision: u64,
    anchor: WorkbenchReviewAnchor,
    feedback: WorkbenchReviewFeedback,
    message: WorkbenchInputText,
    input: WorkbenchInputSelection,
    state: WorkbenchReviewCommentState,
}

impl WorkbenchReviewComment {
    /// Creates a checked positive-revision row.
    ///
    /// # Errors
    /// Rejects revision zero.
    pub fn new(
        id: ControlOperationId,
        revision: u64,
        anchor: WorkbenchReviewAnchor,
        feedback: WorkbenchReviewFeedback,
        message: WorkbenchInputText,
        input: WorkbenchInputSelection,
        state: WorkbenchReviewCommentState,
    ) -> Result<Self, AppProtocolError> {
        if revision == 0 {
            return Err(malformed());
        }
        Ok(Self { id, revision, anchor, feedback, message, input, state })
    }
    /// Stable creation identity.
    #[must_use]
    pub const fn id(&self) -> ControlOperationId {
        self.id
    }
    /// Comment-specific revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
    /// Current stored anchor.
    #[must_use]
    pub const fn anchor(&self) -> &WorkbenchReviewAnchor {
        &self.anchor
    }
    /// Typed user meaning.
    #[must_use]
    pub const fn feedback(&self) -> WorkbenchReviewFeedback {
        self.feedback
    }
    /// Exact user comment.
    #[must_use]
    pub const fn message(&self) -> &WorkbenchInputText {
        &self.message
    }
    /// Queue input carrying the current comment revision.
    #[must_use]
    pub const fn input(&self) -> WorkbenchInputSelection {
        self.input
    }
    /// Current projected lifecycle.
    #[must_use]
    pub const fn state(&self) -> WorkbenchReviewCommentState {
        self.state
    }
}

/// Qualification evidence category displayed beside anchored feedback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkbenchReviewEvidenceKind {
    /// Fresh exact-target deterministic checks.
    Checks,
    /// Fresh independent reviewer conclusion.
    IndependentReview,
}

impl WorkbenchReviewEvidenceKind {
    /// Stable tag.
    #[must_use]
    pub const fn tag(self) -> u16 {
        match self {
            Self::Checks => 1,
            Self::IndependentReview => 2,
        }
    }
    /// Decodes a stable tag.
    #[must_use]
    pub const fn from_tag(tag: u16) -> Option<Self> {
        match tag {
            1 => Some(Self::Checks),
            2 => Some(Self::IndependentReview),
            _ => None,
        }
    }
}

/// Fail-closed evidence freshness/result state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkbenchReviewEvidenceState {
    /// No evidence retained.
    Missing,
    /// Positive current evidence.
    Current,
    /// Current evidence with a negative result.
    Failed,
    /// Retained evidence bound to an older candidate or conversation revision.
    Stale,
}

impl WorkbenchReviewEvidenceState {
    /// Stable tag.
    #[must_use]
    pub const fn tag(self) -> u16 {
        match self {
            Self::Missing => 1,
            Self::Current => 2,
            Self::Failed => 3,
            Self::Stale => 4,
        }
    }
    /// Decodes a stable tag.
    #[must_use]
    pub const fn from_tag(tag: u16) -> Option<Self> {
        match tag {
            1 => Some(Self::Missing),
            2 => Some(Self::Current),
            3 => Some(Self::Failed),
            4 => Some(Self::Stale),
            _ => None,
        }
    }
}

/// Candidate-bound evidence receipt projection without diagnostic/private output.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchReviewEvidence {
    kind: WorkbenchReviewEvidenceKind,
    state: WorkbenchReviewEvidenceState,
    candidate_digest: Option<Sha256Digest>,
    conversation_revision: Option<u64>,
    checkpoint_sequence: Option<u64>,
}

impl WorkbenchReviewEvidence {
    /// Constructs a consistently optional provenance tuple.
    ///
    /// # Errors
    /// Rejects partial provenance or a zero checkpoint sequence.
    pub fn new(
        kind: WorkbenchReviewEvidenceKind,
        state: WorkbenchReviewEvidenceState,
        candidate_digest: Option<Sha256Digest>,
        conversation_revision: Option<u64>,
        checkpoint_sequence: Option<u64>,
    ) -> Result<Self, AppProtocolError> {
        let all = candidate_digest.is_some()
            && conversation_revision.is_some()
            && checkpoint_sequence.is_some_and(|sequence| sequence > 0);
        let none = candidate_digest.is_none()
            && conversation_revision.is_none()
            && checkpoint_sequence.is_none();
        if !(all || none) || state != WorkbenchReviewEvidenceState::Missing && none {
            return Err(malformed());
        }
        Ok(Self { kind, state, candidate_digest, conversation_revision, checkpoint_sequence })
    }
    /// Evidence category.
    #[must_use]
    pub const fn kind(self) -> WorkbenchReviewEvidenceKind {
        self.kind
    }
    /// Freshness/result state.
    #[must_use]
    pub const fn state(self) -> WorkbenchReviewEvidenceState {
        self.state
    }
    /// Producing candidate, when retained.
    #[must_use]
    pub const fn candidate_digest(self) -> Option<Sha256Digest> {
        self.candidate_digest
    }
    /// Producing conversation revision.
    #[must_use]
    pub const fn conversation_revision(self) -> Option<u64> {
        self.conversation_revision
    }
    /// Producing checkpoint sequence.
    #[must_use]
    pub const fn checkpoint_sequence(self) -> Option<u64> {
        self.checkpoint_sequence
    }
}

/// Complete structured diff plus a revision-fenced page of feedback and evidence mappings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchReviewPage {
    query: WorkbenchReviewQuery,
    candidate_digest: Sha256Digest,
    diff_digest: Sha256Digest,
    files: Vec<WorkbenchDiffFile>,
    comments: Vec<WorkbenchReviewComment>,
    total_comments: u32,
    evidence: Vec<WorkbenchReviewEvidence>,
}

impl WorkbenchReviewPage {
    /// Creates a checked current page.
    ///
    /// # Errors
    /// Rejects absent revision, inconsistent identities, or collection limits.
    pub fn new(
        query: WorkbenchReviewQuery,
        candidate_digest: Sha256Digest,
        diff_digest: Sha256Digest,
        files: Vec<WorkbenchDiffFile>,
        comments: Vec<WorkbenchReviewComment>,
        total_comments: u32,
        evidence: Vec<WorkbenchReviewEvidence>,
    ) -> Result<Self, AppProtocolError> {
        let hunk_count = files.iter().map(|file| file.hunks().len()).sum::<usize>();
        let line_count = files
            .iter()
            .flat_map(WorkbenchDiffFile::hunks)
            .map(|hunk| hunk.lines().len())
            .sum::<usize>();
        if query.revision() == 0
            || files.len() > MAX_WORKBENCH_DIFF_FILES
            || hunk_count > MAX_WORKBENCH_DIFF_HUNKS
            || line_count > MAX_WORKBENCH_DIFF_LINES
            || comments.len() > MAX_WORKBENCH_REVIEW_PAGE
            || comments.len() > total_comments as usize
            || evidence.len() > 2
            || files.iter().any(|file| {
                file.anchor().run() != query.run()
                    || file.anchor().workspace() != query.query().workspace()
                    || file.anchor().candidate_digest() != candidate_digest
                    || file.anchor().diff_digest() != diff_digest
            })
        {
            return Err(malformed());
        }
        Ok(Self { query, candidate_digest, diff_digest, files, comments, total_comments, evidence })
    }
    /// Exact response scope and revision.
    #[must_use]
    pub const fn query(&self) -> WorkbenchReviewQuery {
        self.query
    }
    /// Current complete candidate identity.
    #[must_use]
    pub const fn candidate_digest(&self) -> Sha256Digest {
        self.candidate_digest
    }
    /// Current raw-diff identity.
    #[must_use]
    pub const fn diff_digest(&self) -> Sha256Digest {
        self.diff_digest
    }
    /// Structured files.
    #[must_use]
    pub fn files(&self) -> &[WorkbenchDiffFile] {
        &self.files
    }
    /// Current comment page.
    #[must_use]
    pub fn comments(&self) -> &[WorkbenchReviewComment] {
        &self.comments
    }
    /// Complete comment count.
    #[must_use]
    pub const fn total_comments(&self) -> u32 {
        self.total_comments
    }
    /// Check/review evidence freshness mappings.
    #[must_use]
    pub fn evidence(&self) -> &[WorkbenchReviewEvidence] {
        &self.evidence
    }
}
