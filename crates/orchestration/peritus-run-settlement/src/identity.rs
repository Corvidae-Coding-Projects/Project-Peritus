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
    /// Logical view of the governing run identity.
    pub closed spec fn spec_run_id(&self) -> RunId { self.run_id }
    /// Logical view of the managed workspace identity.
    pub closed spec fn spec_workspace_id(&self) -> WorkspaceId { self.workspace_id }
    /// Logical view of the candidate content digest.
    pub closed spec fn spec_candidate_digest(&self) -> Sha256Digest { self.candidate_digest }
    /// Logical view of the incorporated conversation revision.
    pub closed spec fn spec_conversation_revision(&self) -> u64 { self.conversation_revision }
    /// Logical view of the checkpoint sequence.
    pub closed spec fn spec_checkpoint_sequence(&self) -> u64 { self.checkpoint_sequence }

    /// Both exact run and workspace byte identities agree.
    pub open spec fn spec_same_lineage(&self, other: &Self) -> bool {
        self.spec_run_id().spec_bytes()@ == other.spec_run_id().spec_bytes()@
            && self.spec_workspace_id().spec_bytes()@ == other.spec_workspace_id().spec_bytes()@
    }

    /// Exact content and conversation identity agree within the same lineage.
    pub open spec fn spec_same_candidate(&self, other: &Self) -> bool {
        self.spec_same_lineage(other)
            && self.spec_candidate_digest().spec_bytes()@
                == other.spec_candidate_digest().spec_bytes()@
            && self.spec_conversation_revision() == other.spec_conversation_revision()
    }

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
    ) -> (result: Result<Self, SettlementError>)
        ensures
            result.is_ok() == (checkpoint_sequence > 0),
            match result {
                Ok(value) => value.spec_run_id() == run_id
                    && value.spec_workspace_id() == workspace_id
                    && value.spec_candidate_digest() == candidate_digest
                    && value.spec_conversation_revision() == conversation_revision
                    && value.spec_checkpoint_sequence() == checkpoint_sequence,
                Err(error) => error.spec_kind() == SettlementErrorKind::ZeroCheckpointSequence,
            },
    {
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
    pub const fn run_id(&self) -> (value: RunId)
        ensures value == self.spec_run_id(),
    { self.run_id }

    /// Managed workspace lineage.
    #[must_use]
    pub const fn workspace_id(&self) -> (value: WorkspaceId)
        ensures value == self.spec_workspace_id(),
    { self.workspace_id }

    /// Digest of the exact candidate content.
    #[must_use]
    pub const fn candidate_digest(&self) -> (value: Sha256Digest)
        ensures value == self.spec_candidate_digest(),
    { self.candidate_digest }

    /// User-conversation revision incorporated by the candidate.
    #[must_use]
    pub const fn conversation_revision(&self) -> (value: u64)
        ensures value == self.spec_conversation_revision(),
    { self.conversation_revision }

    /// Monotonic observation sequence within the run.
    #[must_use]
    pub const fn checkpoint_sequence(&self) -> (value: u64)
        ensures value == self.spec_checkpoint_sequence(),
    { self.checkpoint_sequence }

    /// Returns whether both values refer to the same run and managed workspace.
    #[must_use]
    pub fn same_lineage(&self, other: &Self) -> (same: bool)
        ensures same == self.spec_same_lineage(other),
    {
        bytes_equal(self.run_id.as_bytes(), other.run_id.as_bytes())
            && bytes_equal(self.workspace_id.as_bytes(), other.workspace_id.as_bytes())
    }

    /// Returns whether both values refer to the same candidate and conversation revision.
    #[must_use]
    pub fn same_candidate(&self, other: &Self) -> (same: bool)
        ensures same == self.spec_same_candidate(other),
    {
        self.same_lineage(other)
            && bytes_equal(self.candidate_digest.as_bytes(), other.candidate_digest.as_bytes())
            && self.conversation_revision == other.conversation_revision
    }
}

fn bytes_equal<const N: usize>(left: &[u8; N], right: &[u8; N]) -> (equal: bool)
    ensures equal == (left@ == right@),
{
    let mut index = 0;
    while index < N
        invariant
            index <= N,
            forall |prior: int| 0 <= prior < index ==> left@[prior] == right@[prior],
        decreases N - index,
    {
        if left[index] != right[index] {
            return false;
        }
        index += 1;
    }
    assert(left@ =~= right@);
    true
}

} // verus!
