//! Exact identity of one observed workspace candidate.

use crate::{SettlementError, SettlementErrorKind};
use peritus_types::{RunId, Sha256Digest, WorkspaceId};

#[doc = " Host-observed content, repository, execution, requirements, and checkpoint identity."]
#[doc = ""]
#[doc = " `repository_digest` is the exact workspace snapshot used to fence handoff operations."]
#[doc = " `content_digest` deliberately excludes Git history. `execution_digest` is absent until the"]
#[doc = " host has observed the execution context for the current process. Evidence declares which of"]
#[doc = " these axes it depends on instead of treating the repository snapshot as every kind of fact."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CandidateIdentity {
    run_id: RunId,
    workspace_id: WorkspaceId,
    content_digest: Sha256Digest,
    repository_digest: Sha256Digest,
    execution_digest: Option<Sha256Digest>,
    requirements_revision: u64,
    checkpoint_sequence: u64,
}
impl CandidateIdentity {
    #[doc = " Creates an identity from separately observed host facts."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`SettlementErrorKind::ZeroCheckpointSequence`] for sequence zero."]
    pub const fn new(
        run_id: RunId,
        workspace_id: WorkspaceId,
        content_digest: Sha256Digest,
        repository_digest: Sha256Digest,
        execution_digest: Option<Sha256Digest>,
        requirements_revision: u64,
        checkpoint_sequence: u64,
    ) -> Result<Self, SettlementError> {
        if checkpoint_sequence == 0 {
            Err(SettlementError::new(SettlementErrorKind::ZeroCheckpointSequence))
        } else {
            Ok(Self {
                run_id,
                workspace_id,
                content_digest,
                repository_digest,
                execution_digest,
                requirements_revision,
                checkpoint_sequence,
            })
        }
    }
    #[doc = " Governing coding run."]
    #[must_use]
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }
    #[doc = " Managed workspace lineage."]
    #[must_use]
    pub const fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }
    #[doc = " Source-content digest independent of repository history."]
    #[must_use]
    pub const fn content_digest(&self) -> Sha256Digest {
        self.content_digest
    }
    #[doc = " Exact repository snapshot used to fence handoff operations."]
    #[must_use]
    pub const fn repository_digest(&self) -> Sha256Digest {
        self.repository_digest
    }
    #[doc = " Host-observed execution context, absent until acquired in this process."]
    #[must_use]
    pub const fn execution_digest(&self) -> Option<Sha256Digest> {
        self.execution_digest
    }
    #[doc = " Public-requirements revision incorporated by the candidate."]
    #[must_use]
    pub const fn requirements_revision(&self) -> u64 {
        self.requirements_revision
    }
    #[doc = " Monotonic observation sequence within the run."]
    #[must_use]
    pub const fn checkpoint_sequence(&self) -> u64 {
        self.checkpoint_sequence
    }
    #[doc = " Returns whether both values refer to the same run and managed workspace."]
    #[must_use]
    pub fn same_lineage(&self, other: &Self) -> bool {
        bytes_equal(self.run_id.as_bytes(), other.run_id.as_bytes())
            && bytes_equal(self.workspace_id.as_bytes(), other.workspace_id.as_bytes())
    }
    #[doc = " Returns whether both values refer to the same candidate and conversation revision."]
    #[must_use]
    pub fn same_candidate(&self, other: &Self) -> bool {
        self.same_lineage(other)
            && bytes_equal(self.content_digest.as_bytes(), other.content_digest.as_bytes())
            && bytes_equal(self.repository_digest.as_bytes(), other.repository_digest.as_bytes())
            && execution_equal(self.execution_digest, other.execution_digest)
            && self.requirements_revision == other.requirements_revision
    }
    #[doc = " Whether source content and public requirements agree within this lineage."]
    #[must_use]
    pub fn same_content_and_requirements(&self, other: &Self) -> bool {
        self.matches_evidence_axes(other, false)
    }
    #[doc = " Compares source, requirements, and any declared execution context."]
    #[must_use]
    pub fn matches_evidence_axes(&self, other: &Self, execution: bool) -> bool {
        self.same_lineage(other)
            && bytes_equal(self.content_digest.as_bytes(), other.content_digest.as_bytes())
            && self.requirements_revision == other.requirements_revision
            && (!execution
                || execution_observed_equal(self.execution_digest, other.execution_digest))
    }
}
fn execution_equal(left: Option<Sha256Digest>, right: Option<Sha256Digest>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => bytes_equal(left.as_bytes(), right.as_bytes()),
        (None, None) => true,
        _ => false,
    }
}
fn execution_observed_equal(left: Option<Sha256Digest>, right: Option<Sha256Digest>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => bytes_equal(left.as_bytes(), right.as_bytes()),
        _ => false,
    }
}
fn bytes_equal<const N: usize>(left: &[u8; N], right: &[u8; N]) -> bool {
    let mut index = 0;
    while index < N {
        if left[index] != right[index] {
            return false;
        }
        index += 1;
    }
    true
}
