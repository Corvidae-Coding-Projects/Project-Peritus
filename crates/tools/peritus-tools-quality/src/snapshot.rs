//! Clean immutable C1 snapshot binding for quality execution.

use core::fmt;
use std::path::PathBuf;

use peritus_process::{ExecutionPlan, WorkspaceAccess};
use peritus_types::{EnvironmentId, ResourceId, RevisionTuple, Sha256Digest};
use peritus_workspace::ReadOnlyWorkspace;
use sha2::{Digest, Sha256};

use crate::{QualityError, QualityErrorKind};

/// Revalidated clean C1 snapshot observation bound to one exact D1 revision and target.
#[derive(Clone, Eq, PartialEq)]
pub struct CleanQualitySnapshot {
    revision: RevisionTuple,
    environment_id: EnvironmentId,
    resource_id: ResourceId,
    canonical_root: PathBuf,
    status_digest: Sha256Digest,
    binding_digest: Sha256Digest,
}

impl CleanQualitySnapshot {
    /// Revalidates a physically distinct immutable snapshot for quality execution.
    ///
    /// # Errors
    /// Returns a typed failure unless C1 reports the exact target, detached snapshot commit and
    /// tree, an empty status, and revision counters matching `revision`.
    pub fn inspect(
        workspace: &ReadOnlyWorkspace,
        revision: RevisionTuple,
        environment_id: EnvironmentId,
        resource_id: ResourceId,
    ) -> Result<Self, QualityError> {
        let snapshot = workspace.snapshot();
        let target = workspace.target_binding().ok_or_else(|| {
            invalid("immutable quality snapshot has no validated C4 target binding")
        })?;
        let target_mismatches = [
            snapshot.workspace_id() != revision.workspace_id(),
            snapshot.generation() != revision.workspace_generation(),
            snapshot.revision() != revision.workspace_revision(),
            target.workspace_id() != revision.workspace_id(),
            target.environment_id() != environment_id,
            target.resource_id() != resource_id,
        ];
        if target_mismatches.into_iter().any(core::convert::identity) {
            return Err(invalid(
                "immutable quality snapshot differs from the exact revision or target",
            ));
        }
        let status = workspace.inspect().map_err(|error| {
            QualityError::new(
                QualityErrorKind::Workspace,
                format!("immutable quality snapshot could not be revalidated: {error}"),
            )
        })?;
        let status_mismatches = [
            !status.is_clean(),
            !status.is_detached(),
            status.head() != snapshot.commit(),
            status.index_tree() != Some(snapshot.tree()),
            status.worktree_root() != workspace.root(),
        ];
        if status_mismatches.into_iter().any(core::convert::identity) {
            return Err(invalid(
                "quality execution requires a clean detached snapshot at the exact commit and tree",
            ));
        }
        let status_digest = status.digest();
        let binding_digest = snapshot_digest(
            revision,
            environment_id,
            resource_id,
            snapshot.commit().object_id().format().as_str(),
            snapshot.commit().object_id().as_bytes(),
            snapshot.tree().object_id().as_bytes(),
            status_digest,
        );
        Ok(Self {
            revision,
            environment_id,
            resource_id,
            canonical_root: workspace.root().to_owned(),
            status_digest,
            binding_digest,
        })
    }

    /// Returns the complete exact revision binding.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }

    /// Returns the clean C1 status digest observed before execution.
    #[must_use]
    pub const fn status_digest(&self) -> Sha256Digest {
        self.status_digest
    }

    /// Returns the canonical snapshot/target/status binding digest.
    #[must_use]
    pub const fn binding_digest(&self) -> Sha256Digest {
        self.binding_digest
    }

    pub(crate) fn validate_plan(&self, plan: &ExecutionPlan) -> Result<(), QualityError> {
        let directory = plan.working_directory();
        let identity = plan.identity();
        if directory.path() != self.canonical_root
            || directory.access() != WorkspaceAccess::ReadOnly
            || directory.workspace_id() != self.revision.workspace_id()
            || directory.generation() != self.revision.workspace_generation()
            || directory.revision() != self.revision.workspace_revision()
            || directory.environment_id() != self.environment_id
            || directory.resource_id() != self.resource_id
            || identity.revision() != self.revision
            || identity.environment_id() != self.environment_id
            || identity.resource_id() != self.resource_id
        {
            return Err(invalid(
                "C2 quality plan does not target the validated clean immutable snapshot",
            ));
        }
        Ok(())
    }
}

impl fmt::Debug for CleanQualitySnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CleanQualitySnapshot")
            .field("revision", &self.revision)
            .field("environment_id", &self.environment_id)
            .field("resource_id", &self.resource_id)
            .field("status_digest", &self.status_digest)
            .field("binding_digest", &self.binding_digest)
            .finish_non_exhaustive()
    }
}

#[allow(clippy::too_many_arguments)]
fn snapshot_digest(
    revision: RevisionTuple,
    environment_id: EnvironmentId,
    resource_id: ResourceId,
    object_format: &str,
    commit: &[u8],
    tree: &[u8],
    status: Sha256Digest,
) -> Sha256Digest {
    let mut hash = Sha256::new();
    hash.update(b"peritus-c4-quality-clean-snapshot-v1\0");
    hash.update(revision.acceptance_spec_id().as_bytes());
    hash.update(revision.harness_id().as_bytes());
    hash.update(revision.workspace_id().as_bytes());
    hash.update(revision.workspace_generation().get().to_be_bytes());
    hash.update(revision.workspace_revision().get().to_be_bytes());
    hash.update(revision.policy_id().as_bytes());
    hash.update(revision.provider_profile_id().as_bytes());
    hash.update(environment_id.as_bytes());
    hash.update(resource_id.as_bytes());
    put_bytes(&mut hash, object_format.as_bytes());
    put_bytes(&mut hash, commit);
    put_bytes(&mut hash, tree);
    hash.update(status.as_bytes());
    Sha256Digest::new(hash.finalize().into())
}

fn put_bytes(hash: &mut Sha256, value: &[u8]) {
    hash.update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_be_bytes());
    hash.update(value);
}

fn invalid(detail: &'static str) -> QualityError {
    QualityError::new(QualityErrorKind::InvalidInput, detail)
}
