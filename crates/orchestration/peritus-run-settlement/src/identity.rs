//! Exact identity of one observed workspace candidate.

use crate::{SettlementError, SettlementErrorKind};
use peritus_types::{RunId, Sha256Digest, WorkspaceId};
use vstd::prelude::*;

verus! {

/// Exact run, workspace, content, conversation, and checkpoint identity of one candidate.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CandidateIdentity {
    run_id: RunId,
    workspace_id: WorkspaceId,
    candidate_digest: Sha256Digest,
    conversation_revision: u64,
    checkpoint_sequence: u64,
}

impl CandidateIdentity {
    /// Creates a candidate identity at a nonzero checkpoint sequence.
    ///
    /// # Errors
    ///
    /// Returns [`SettlementErrorKind::ZeroCheckpointSequence`] for sequence zero.
    pub const fn new(
        run_id: RunId,
        workspace_id: WorkspaceId,
        candidate_digest: Sha256Digest,
        conversation_revision: u64,
        checkpoint_sequence: u64,
    ) -> Result<Self, SettlementError> {
        if checkpoint_sequence == 0 {
            Err(SettlementError::new(SettlementErrorKind::ZeroCheckpointSequence))
        } else {
            Ok(Self {
                run_id,
                workspace_id,
                candidate_digest,
                conversation_revision,
                checkpoint_sequence,
            })
        }
    }

    /// Governing coding run.
    #[must_use]
    pub const fn run_id(&self) -> RunId { self.run_id }

    /// Managed workspace lineage.
    #[must_use]
    pub const fn workspace_id(&self) -> WorkspaceId { self.workspace_id }

    /// Digest of the exact candidate content.
    #[must_use]
    pub const fn candidate_digest(&self) -> Sha256Digest { self.candidate_digest }

    /// User-conversation revision incorporated by the candidate.
    #[must_use]
    pub const fn conversation_revision(&self) -> u64 { self.conversation_revision }

    /// Monotonic observation sequence within the run.
    #[must_use]
    pub const fn checkpoint_sequence(&self) -> u64 { self.checkpoint_sequence }

    /// Returns whether both values refer to the same run and managed workspace.
    #[must_use]
    pub fn same_lineage(&self, other: &Self) -> bool {
        self.run_id == other.run_id && self.workspace_id == other.workspace_id
    }

    /// Returns whether both values refer to the same candidate and conversation revision.
    #[must_use]
    pub fn same_candidate(&self, other: &Self) -> bool {
        self.same_lineage(other)
            && self.candidate_digest == other.candidate_digest
            && self.conversation_revision == other.conversation_revision
    }
}

} // verus!
