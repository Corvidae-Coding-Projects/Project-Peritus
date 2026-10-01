//! Exact identity of one observed workspace candidate.

use crate::{SettlementError, SettlementErrorKind};
use peritus_types::{RunId, Sha256Digest, WorkspaceId};
use vstd::prelude::*;

verus! {

/// Host-observed content, repository, execution, requirements, and checkpoint identity.
///
/// `repository_digest` is the exact workspace snapshot used to fence handoff operations.
/// `content_digest` deliberately excludes Git history. `execution_digest` is absent until the
/// host has observed the execution context for the current process. Evidence declares which of
/// these axes it depends on instead of treating the repository snapshot as every kind of fact.
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
    /// Logical view of the governing run identity.
    pub closed spec fn spec_run_id(&self) -> RunId { self.run_id }
    /// Logical view of the managed workspace identity.
    pub closed spec fn spec_workspace_id(&self) -> WorkspaceId { self.workspace_id }
    /// Logical view of source bytes, kinds, and modes independent of Git history.
    pub closed spec fn spec_content_digest(&self) -> Sha256Digest { self.content_digest }
    /// Logical view of the exact repository snapshot used for handoff fencing.
    pub closed spec fn spec_repository_digest(&self) -> Sha256Digest { self.repository_digest }
    /// Logical view of the observed execution context, when one has been acquired.
    pub closed spec fn spec_execution_digest(&self) -> Option<Sha256Digest> {
        self.execution_digest
    }
    /// Logical view of the incorporated public-requirements revision.
    pub closed spec fn spec_requirements_revision(&self) -> u64 { self.requirements_revision }
    /// Logical view of the checkpoint sequence.
    pub closed spec fn spec_checkpoint_sequence(&self) -> u64 { self.checkpoint_sequence }

    /// Both exact run and workspace byte identities agree.
    pub open spec fn spec_same_lineage(&self, other: &Self) -> bool {
        self.spec_run_id().spec_bytes()@ == other.spec_run_id().spec_bytes()@
            && self.spec_workspace_id().spec_bytes()@ == other.spec_workspace_id().spec_bytes()@
    }

    /// Every observed candidate axis agrees within the same lineage.
    pub open spec fn spec_same_candidate(&self, other: &Self) -> bool {
        self.spec_same_lineage(other)
            && self.spec_content_digest().spec_bytes()@
                == other.spec_content_digest().spec_bytes()@
            && self.spec_repository_digest().spec_bytes()@
                == other.spec_repository_digest().spec_bytes()@
            && execution_equal_spec(
                self.spec_execution_digest(), other.spec_execution_digest())
            && self.spec_requirements_revision() == other.spec_requirements_revision()
    }

    /// Whether source, requirements, and any declared execution context agree.
    /// Execution-dependent evidence requires two present observations; two absent values never
    /// establish execution equivalence.
    pub open spec fn spec_matches_evidence_axes(
        &self,
        other: &Self,
        execution: bool,
    ) -> bool {
        self.spec_same_lineage(other)
            && self.spec_content_digest().spec_bytes()@
                == other.spec_content_digest().spec_bytes()@
            && self.spec_requirements_revision() == other.spec_requirements_revision()
            && (!execution || execution_observed_equal_spec(
                self.spec_execution_digest(), other.spec_execution_digest()))
    }

    /// Creates an identity from separately observed host facts.
    ///
    /// # Errors
    ///
    /// Returns [`SettlementErrorKind::ZeroCheckpointSequence`] for sequence zero.
    pub const fn new(
        run_id: RunId,
        workspace_id: WorkspaceId,
        content_digest: Sha256Digest,
        repository_digest: Sha256Digest,
        execution_digest: Option<Sha256Digest>,
        requirements_revision: u64,
        checkpoint_sequence: u64,
    ) -> (result: Result<Self, SettlementError>)
        ensures
            result.is_ok() == (checkpoint_sequence > 0),
            match result {
                Ok(value) => value.spec_run_id() == run_id
                    && value.spec_workspace_id() == workspace_id
                    && value.spec_content_digest() == content_digest
                    && value.spec_repository_digest() == repository_digest
                    && value.spec_execution_digest() == execution_digest
                    && value.spec_requirements_revision() == requirements_revision
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
                content_digest,
                repository_digest,
                execution_digest,
                requirements_revision,
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

    /// Source-content digest independent of repository history.
    #[must_use]
    pub const fn content_digest(&self) -> (value: Sha256Digest)
        ensures value == self.spec_content_digest(),
    { self.content_digest }

    /// Exact repository snapshot used to fence handoff operations.
    #[must_use]
    pub const fn repository_digest(&self) -> (value: Sha256Digest)
        ensures value == self.spec_repository_digest(),
    { self.repository_digest }

    /// Host-observed execution context, absent until acquired in this process.
    #[must_use]
    pub const fn execution_digest(&self) -> (value: Option<Sha256Digest>)
        ensures value == self.spec_execution_digest(),
    { self.execution_digest }

    /// Public-requirements revision incorporated by the candidate.
    #[must_use]
    pub const fn requirements_revision(&self) -> (value: u64)
        ensures value == self.spec_requirements_revision(),
    { self.requirements_revision }

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
            && bytes_equal(self.content_digest.as_bytes(), other.content_digest.as_bytes())
            && bytes_equal(self.repository_digest.as_bytes(), other.repository_digest.as_bytes())
            && execution_equal(self.execution_digest, other.execution_digest)
            && self.requirements_revision == other.requirements_revision
    }

    /// Whether source content and public requirements agree within this lineage.
    #[must_use]
    pub fn same_content_and_requirements(&self, other: &Self) -> (same: bool)
        ensures same == self.spec_matches_evidence_axes(other, false),
    {
        self.matches_evidence_axes(other, false)
    }

    /// Compares source, requirements, and any declared execution context.
    #[must_use]
    pub fn matches_evidence_axes(
        &self,
        other: &Self,
        execution: bool,
    ) -> (same: bool)
        ensures same == self.spec_matches_evidence_axes(other, execution),
    {
        self.same_lineage(other)
            && bytes_equal(self.content_digest.as_bytes(), other.content_digest.as_bytes())
            && self.requirements_revision == other.requirements_revision
            && (!execution
                || execution_observed_equal(self.execution_digest, other.execution_digest))
    }
}

pub open spec fn execution_equal_spec(
    left: Option<Sha256Digest>,
    right: Option<Sha256Digest>,
) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => left.spec_bytes()@ == right.spec_bytes()@,
        (None, None) => true,
        _ => false,
    }
}

fn execution_equal(
    left: Option<Sha256Digest>,
    right: Option<Sha256Digest>,
) -> (equal: bool)
    ensures equal == execution_equal_spec(left, right),
{
    match (left, right) {
        (Some(left), Some(right)) => bytes_equal(left.as_bytes(), right.as_bytes()),
        (None, None) => true,
        _ => false,
    }
}

pub open spec fn execution_observed_equal_spec(
    left: Option<Sha256Digest>,
    right: Option<Sha256Digest>,
) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => left.spec_bytes()@ == right.spec_bytes()@,
        _ => false,
    }
}

fn execution_observed_equal(
    left: Option<Sha256Digest>,
    right: Option<Sha256Digest>,
) -> (equal: bool)
    ensures equal == execution_observed_equal_spec(left, right),
{
    match (left, right) {
        (Some(left), Some(right)) => bytes_equal(left.as_bytes(), right.as_bytes()),
        _ => false,
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
