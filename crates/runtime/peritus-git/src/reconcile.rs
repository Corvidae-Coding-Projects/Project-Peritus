//! Deterministic reconciliation of exact Git observations with expected state.

use peritus_types::Sha256Digest;

use crate::status::put_bytes;
use crate::{CommitId, GitError, GitRepository, RegisteredWorktree, StatusObservation, TreeId};

/// One exact reason that a worktree cannot be classified clean.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DirtyReason {
    /// Managed HEAD is attached to a branch instead of detached.
    AttachedHead,
    /// Current HEAD differs from the expected commit.
    HeadMismatch,
    /// Current index tree differs from the expected tree.
    IndexMismatch,
    /// Tracked filesystem or submodule content differs from the index.
    WorktreeModified,
    /// One or more untracked paths exist.
    Untracked,
    /// One or more ignored paths exist and were not allowed by the request.
    Ignored,
    /// Unmerged entries prevent an exact tree observation.
    Conflict,
    /// Git could not produce an index tree despite reporting no explicit conflict.
    IndexUnavailable,
}

/// Closed reconciliation disposition over one successfully parsed observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReconcileDisposition {
    /// All expected identities match and no disallowed dirtiness exists.
    Clean,
    /// State is exactly observed but differs for the listed reasons.
    Dirty(Vec<DirtyReason>),
    /// Conflicts or missing exact state prevent a safe clean/dirty conclusion.
    Indeterminate(Vec<DirtyReason>),
}

/// Expected exact state used for deterministic reconciliation.
#[derive(Clone, Copy, Debug)]
pub struct ReconcileExpectation<'a> {
    worktree: &'a RegisteredWorktree,
    head: CommitId,
    tree: TreeId,
    allow_ignored: bool,
}

impl<'a> ReconcileExpectation<'a> {
    /// Binds reconciliation to one registered worktree, HEAD, and index tree.
    #[must_use]
    pub const fn new(worktree: &'a RegisteredWorktree, head: CommitId, tree: TreeId) -> Self {
        Self { worktree, head, tree, allow_ignored: false }
    }

    /// Selects whether ignored paths are compatible with a clean result.
    #[must_use]
    pub const fn allow_ignored(mut self, allow: bool) -> Self {
        self.allow_ignored = allow;
        self
    }
}

/// Status, classification, and canonical evidence digest from reconciliation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReconcileObservation {
    status: StatusObservation,
    disposition: ReconcileDisposition,
    evidence_digest: Sha256Digest,
}

impl ReconcileObservation {
    /// Returns the complete typed Git observation.
    #[must_use]
    pub const fn status(&self) -> &StatusObservation {
        &self.status
    }

    /// Returns the closed deterministic disposition.
    #[must_use]
    pub const fn disposition(&self) -> &ReconcileDisposition {
        &self.disposition
    }

    /// Returns the canonical digest binding expectations, status, and disposition.
    #[must_use]
    pub const fn evidence_digest(&self) -> Sha256Digest {
        self.evidence_digest
    }
}

impl GitRepository {
    /// Reconciles exact HEAD, index tree, worktree changes, untracked files, and ignored files.
    ///
    /// # Errors
    ///
    /// Returns a typed registration, Git command, object, or porcelain parsing failure. Successfully
    /// parsed dirty and conflicted states are returned as dispositions, not errors.
    pub fn reconcile(
        &self,
        expectation: ReconcileExpectation<'_>,
    ) -> Result<ReconcileObservation, GitError> {
        let status = self.status(expectation.worktree)?;
        let disposition = classify(&status, &expectation);
        let evidence_digest = evidence_digest(&status, &expectation, &disposition);
        Ok(ReconcileObservation { status, disposition, evidence_digest })
    }
}

fn classify(
    status: &StatusObservation,
    expectation: &ReconcileExpectation<'_>,
) -> ReconcileDisposition {
    if !status.is_detached() {
        return ReconcileDisposition::Indeterminate(vec![DirtyReason::AttachedHead]);
    }
    let head_matches = status.head() == expectation.head;
    let tree_matches = status.index_tree() == Some(expectation.tree);
    let worktree_changed = status.has_worktree_change();
    let untracked = status.has_untracked();
    let ignored = status.has_ignored();
    if status.has_unmerged() {
        return ReconcileDisposition::Indeterminate(vec![DirtyReason::Conflict]);
    }
    if status.index_tree().is_none() {
        return ReconcileDisposition::Indeterminate(vec![DirtyReason::IndexUnavailable]);
    }
    if crate::verified::reconciliation_is_clean(
        status.is_detached(),
        head_matches,
        tree_matches,
        worktree_changed,
        untracked,
        ignored,
        expectation.allow_ignored,
    ) {
        return ReconcileDisposition::Clean;
    }
    let mut reasons = Vec::new();
    if !head_matches {
        reasons.push(DirtyReason::HeadMismatch);
    }
    if !tree_matches {
        reasons.push(DirtyReason::IndexMismatch);
    }
    if worktree_changed {
        reasons.push(DirtyReason::WorktreeModified);
    }
    if untracked {
        reasons.push(DirtyReason::Untracked);
    }
    if ignored && !expectation.allow_ignored {
        reasons.push(DirtyReason::Ignored);
    }
    ReconcileDisposition::Dirty(reasons)
}

fn evidence_digest(
    status: &StatusObservation,
    expectation: &ReconcileExpectation<'_>,
    disposition: &ReconcileDisposition,
) -> Sha256Digest {
    let mut bytes = b"PERITUS-GIT-RECONCILIATION-V1\0".to_vec();
    bytes.extend_from_slice(status.repository_digest().as_bytes());
    put_bytes(&mut bytes, expectation.head.object_id().as_bytes());
    put_bytes(&mut bytes, expectation.tree.object_id().as_bytes());
    bytes.push(u8::from(expectation.allow_ignored));
    bytes.extend_from_slice(status.digest().as_bytes());
    match disposition {
        ReconcileDisposition::Clean => bytes.push(0),
        ReconcileDisposition::Dirty(reasons) => {
            bytes.push(1);
            put_reasons(&mut bytes, reasons);
        }
        ReconcileDisposition::Indeterminate(reasons) => {
            bytes.push(2);
            put_reasons(&mut bytes, reasons);
        }
    }
    peritus_codec::sha256(&bytes)
}

fn put_reasons(bytes: &mut Vec<u8>, reasons: &[DirtyReason]) {
    bytes.extend_from_slice(&(reasons.len() as u64).to_be_bytes());
    for reason in reasons {
        bytes.push(match reason {
            DirtyReason::AttachedHead => 0,
            DirtyReason::HeadMismatch => 1,
            DirtyReason::IndexMismatch => 2,
            DirtyReason::WorktreeModified => 3,
            DirtyReason::Untracked => 4,
            DirtyReason::Ignored => 5,
            DirtyReason::Conflict => 6,
            DirtyReason::IndexUnavailable => 7,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{DirtyReason, ReconcileDisposition, put_reasons};

    #[test]
    fn reason_encoding_is_order_sensitive_and_total() {
        let mut bytes = Vec::new();
        put_reasons(
            &mut bytes,
            &[
                DirtyReason::AttachedHead,
                DirtyReason::HeadMismatch,
                DirtyReason::IndexMismatch,
                DirtyReason::WorktreeModified,
                DirtyReason::Untracked,
                DirtyReason::Ignored,
                DirtyReason::Conflict,
                DirtyReason::IndexUnavailable,
            ],
        );
        assert_eq!(&bytes[8..], &[0, 1, 2, 3, 4, 5, 6, 7]);
        assert_ne!(ReconcileDisposition::Clean, ReconcileDisposition::Dirty(Vec::new()));
    }
}
