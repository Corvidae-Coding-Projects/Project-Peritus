//! Bounded digest-bound review metadata for peers using structured-diff pagination.

use super::{WorkbenchReviewComment, WorkbenchReviewEvidence, WorkbenchReviewQuery, malformed};
use crate::{AppProtocolError, MAX_WORKBENCH_REVIEW_PAGE};
use peritus_types::Sha256Digest;

/// Initial review metadata that leaves changed-file content to the paged diff endpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchReviewSummary {
    query: WorkbenchReviewQuery,
    candidate_digest: Sha256Digest,
    diff_digest: Sha256Digest,
    total_files: u32,
    total_hunks: u64,
    total_lines: u64,
    structured_available: bool,
    comments: Vec<WorkbenchReviewComment>,
    total_comments: u32,
    evidence: Vec<WorkbenchReviewEvidence>,
}

impl WorkbenchReviewSummary {
    /// Creates a checked bounded initial review response.
    ///
    /// # Errors
    /// Rejects an absent revision, oversized or inconsistent comment page, or excess evidence.
    pub fn new(
        query: WorkbenchReviewQuery,
        candidate_digest: Sha256Digest,
        diff_digest: Sha256Digest,
        total_files: u32,
        total_hunks: u64,
        total_lines: u64,
        structured_available: bool,
        comments: Vec<WorkbenchReviewComment>,
        total_comments: u32,
        evidence: Vec<WorkbenchReviewEvidence>,
    ) -> Result<Self, AppProtocolError> {
        if query.revision() == 0
            || (!structured_available && (total_files != 0 || total_hunks != 0 || total_lines != 0))
            || comments.len() > MAX_WORKBENCH_REVIEW_PAGE
            || comments.len() > usize::try_from(total_comments).unwrap_or(usize::MAX)
            || evidence.len() > 2
            || comments.iter().any(|comment| {
                comment.revision() > query.revision()
                    || comment.anchor().run() != query.run()
                    || comment.anchor().workspace() != query.query().workspace()
            })
        {
            return Err(malformed());
        }
        Ok(Self {
            query,
            candidate_digest,
            diff_digest,
            total_files,
            total_hunks,
            total_lines,
            structured_available,
            comments,
            total_comments,
            evidence,
        })
    }

    /// Exact review scope and revision.
    #[must_use]
    pub const fn query(&self) -> WorkbenchReviewQuery {
        self.query
    }
    /// Current candidate identity.
    #[must_use]
    pub const fn candidate_digest(&self) -> Sha256Digest {
        self.candidate_digest
    }
    /// Exact retained raw-diff identity.
    #[must_use]
    pub const fn diff_digest(&self) -> Sha256Digest {
        self.diff_digest
    }
    /// Total structured files.
    #[must_use]
    pub const fn total_files(&self) -> u32 {
        self.total_files
    }
    /// Total structured hunks.
    #[must_use]
    pub const fn total_hunks(&self) -> u64 {
        self.total_hunks
    }
    /// Total structured lines.
    #[must_use]
    pub const fn total_lines(&self) -> u64 {
        self.total_lines
    }
    /// Whether structured parsing is available for this exact retained diff.
    #[must_use]
    pub const fn structured_available(&self) -> bool {
        self.structured_available
    }
    /// Current bounded comment page.
    #[must_use]
    pub fn comments(&self) -> &[WorkbenchReviewComment] {
        &self.comments
    }
    /// Total comments at the current revision.
    #[must_use]
    pub const fn total_comments(&self) -> u32 {
        self.total_comments
    }
    /// Current qualification evidence projection.
    #[must_use]
    pub fn evidence(&self) -> &[WorkbenchReviewEvidence] {
        &self.evidence
    }
}
