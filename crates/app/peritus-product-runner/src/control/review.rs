//! Digest-bound conversational change feedback and hard path constraints.

use std::{
    collections::BTreeSet,
    path::{Component, Path, PathBuf},
};

use peritus_types::{RunId, Sha256Digest, WorkspaceId};
use serde::Deserialize;
use serde::Serialize;

use super::{
    ControlError, ControlText, InputId, InputLedger, InputSelection, InputState, InvocationId,
    OperationId, QueueIntent,
};

const MAX_REVIEW_COMMENTS: usize = 512;
const MAX_REVIEW_PATH_BYTES: usize = 4096;

/// Whether feedback covers the complete file diff or one exact hunk.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewTarget {
    /// Complete file-level change selected in the structured panel.
    File,
    /// One parsed unified-diff hunk selected in the structured panel.
    Hunk,
}

/// Exact old/new line range carried in addition to content identities.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewRange {
    old_start: u32,
    old_lines: u32,
    new_start: u32,
    new_lines: u32,
}

impl ReviewRange {
    /// Creates a checked hunk range. At least one side must contain content.
    ///
    /// # Errors
    /// Rejects an empty range or a positive count paired with line zero.
    pub const fn hunk(
        old_start: u32,
        old_lines: u32,
        new_start: u32,
        new_lines: u32,
    ) -> Result<Self, ControlError> {
        if old_lines == 0 && new_lines == 0
            || old_lines > 0 && old_start == 0
            || new_lines > 0 && new_start == 0
        {
            Err(ControlError::InvalidInput)
        } else {
            Ok(Self { old_start, old_lines, new_start, new_lines })
        }
    }

    /// Canonical sentinel for complete-file selection.
    #[must_use]
    pub const fn file() -> Self {
        Self { old_start: 0, old_lines: 0, new_start: 0, new_lines: 0 }
    }

    /// Old-image first line, or zero for a complete-file selection.
    #[must_use]
    pub const fn old_start(self) -> u32 {
        self.old_start
    }
    /// Old-image line count.
    #[must_use]
    pub const fn old_lines(self) -> u32 {
        self.old_lines
    }
    /// New-image first line, or zero for a complete-file selection.
    #[must_use]
    pub const fn new_start(self) -> u32 {
        self.new_start
    }
    /// New-image line count.
    #[must_use]
    pub const fn new_lines(self) -> u32 {
        self.new_lines
    }

    fn valid_for(self, target: ReviewTarget) -> bool {
        match target {
            ReviewTarget::File => self == Self::file(),
            ReviewTarget::Hunk => {
                Self::hunk(self.old_start, self.old_lines, self.new_start, self.new_lines).is_ok()
            }
        }
    }
}

/// Exact immutable review target. Line numbers aid display but never identify it by themselves.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewAnchor {
    run: [u8; 16],
    workspace: [u8; 16],
    candidate_digest: [u8; 32],
    diff_digest: [u8; 32],
    path: PathBuf,
    before_blob_digest: [u8; 32],
    after_blob_digest: [u8; 32],
    context_digest: [u8; 32],
    target: ReviewTarget,
    range: ReviewRange,
}

impl ReviewAnchor {
    /// Creates an exact scoped file or hunk anchor.
    ///
    /// # Errors
    /// Rejects non-relative, non-normal, metadata, oversized, or range-inconsistent paths.
    #[allow(clippy::too_many_arguments, reason = "every independent freshness binding is explicit")]
    pub fn new(
        run: RunId,
        workspace: WorkspaceId,
        candidate_digest: Sha256Digest,
        diff_digest: Sha256Digest,
        path: PathBuf,
        before_blob_digest: Sha256Digest,
        after_blob_digest: Sha256Digest,
        context_digest: Sha256Digest,
        target: ReviewTarget,
        range: ReviewRange,
    ) -> Result<Self, ControlError> {
        validate_path(&path)?;
        if !range.valid_for(target) {
            return Err(ControlError::InvalidInput);
        }
        Ok(Self {
            run: run.into_bytes(),
            workspace: workspace.into_bytes(),
            candidate_digest: candidate_digest.into_bytes(),
            diff_digest: diff_digest.into_bytes(),
            path,
            before_blob_digest: before_blob_digest.into_bytes(),
            after_blob_digest: after_blob_digest.into_bytes(),
            context_digest: context_digest.into_bytes(),
            target,
            range,
        })
    }

    /// Governing execution lineage.
    ///
    /// # Panics
    ///
    /// Never panics after construction because the run identifier is validated by `new`.
    #[must_use]
    pub fn run(&self) -> RunId {
        RunId::new(self.run).expect("validated review run")
    }
    /// Governing managed workspace lineage.
    ///
    /// # Panics
    ///
    /// Never panics after construction because the workspace identifier is validated by `new`.
    #[must_use]
    pub fn workspace(&self) -> WorkspaceId {
        WorkspaceId::new(self.workspace).expect("validated review workspace")
    }
    /// Exact candidate content identity.
    #[must_use]
    pub const fn candidate_digest(&self) -> Sha256Digest {
        Sha256Digest::new(self.candidate_digest)
    }
    /// Exact raw diff identity from which the structured target was derived.
    #[must_use]
    pub const fn diff_digest(&self) -> Sha256Digest {
        Sha256Digest::new(self.diff_digest)
    }
    /// Relative candidate path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Canonical old-image blob digest for this target.
    #[must_use]
    pub const fn before_blob_digest(&self) -> Sha256Digest {
        Sha256Digest::new(self.before_blob_digest)
    }
    /// Canonical new-image blob digest for this target.
    #[must_use]
    pub const fn after_blob_digest(&self) -> Sha256Digest {
        Sha256Digest::new(self.after_blob_digest)
    }
    /// Context identity preventing coincidental line-number rebinding.
    #[must_use]
    pub const fn context_digest(&self) -> Sha256Digest {
        Sha256Digest::new(self.context_digest)
    }
    /// File or hunk selection.
    #[must_use]
    pub const fn target(&self) -> ReviewTarget {
        self.target
    }
    /// Exact selected old/new line range.
    #[must_use]
    pub const fn range(&self) -> ReviewRange {
        self.range
    }

    pub(super) fn validate(&self) -> Result<(), ControlError> {
        validate_path(&self.path)?;
        RunId::new(self.run).map_err(|_| ControlError::InvalidInput)?;
        WorkspaceId::new(self.workspace).map_err(|_| ControlError::InvalidInput)?;
        if !self.range.valid_for(self.target) {
            return Err(ControlError::InvalidInput);
        }
        Ok(())
    }
}

fn validate_path(path: &Path) -> Result<(), ControlError> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path.to_string_lossy().len() > MAX_REVIEW_PATH_BYTES
        || path.starts_with(".git")
        || path.components().any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(ControlError::InvalidInput);
    }
    Ok(())
}

/// Closed user meaning for one anchored change comment.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewFeedback {
    /// Read-only request for a source-grounded explanation.
    Explain,
    /// Explicit request to revise the candidate through ordinary work admission.
    RequestRevision,
    /// Semantic behavior preference; explicitly not a hard path constraint.
    KeepBehavior,
    /// Hard exact target constraint enforced below the model.
    LeaveAlone,
}

/// Durable user-controlled lifecycle. Staleness is projected against the current diff, not stored.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewCommentState {
    /// The feedback remains applicable to its exact anchor.
    Open,
    /// A host-published reply incorporated the read-only explanation request.
    Addressed,
    /// The user explicitly dismissed the feedback or constraint.
    Dismissed,
}

/// One immutable-history review comment with an editable exact anchor revision.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewComment {
    id: OperationId,
    revision: u64,
    anchor: ReviewAnchor,
    feedback: ReviewFeedback,
    message: ControlText<8192>,
    input: InputSelection,
    state: ReviewCommentState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    addressed_by: Option<OperationId>,
}

impl ReviewComment {
    /// Stable creation operation used as the comment identity.
    #[must_use]
    pub const fn id(&self) -> OperationId {
        self.id
    }
    /// Monotonic comment-specific anchor/state revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
    /// Current exact anchor; prior values remain in the control event journal.
    #[must_use]
    pub const fn anchor(&self) -> &ReviewAnchor {
        &self.anchor
    }
    /// Typed user meaning.
    #[must_use]
    pub const fn feedback(&self) -> ReviewFeedback {
        self.feedback
    }
    /// Exact user-authored comment text.
    #[must_use]
    pub fn message(&self) -> &str {
        self.message.as_str()
    }
    /// Exact queue input carrying this revision into a model request.
    #[must_use]
    pub const fn input(&self) -> InputSelection {
        self.input
    }
    /// Durable lifecycle independent of projected anchor freshness.
    #[must_use]
    pub const fn state(&self) -> ReviewCommentState {
        self.state
    }
    /// Host-published reply operation that addressed an explanation.
    #[must_use]
    pub const fn addressed_by(&self) -> Option<OperationId> {
        self.addressed_by
    }

    fn active_constraint(&self) -> bool {
        self.state != ReviewCommentState::Dismissed && self.feedback == ReviewFeedback::LeaveAlone
    }
}

/// Bounded review ledger embedded in the authoritative conversation root.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewLedger {
    comments: Vec<ReviewComment>,
}

mod ledger;
#[cfg(test)]
mod tests;
