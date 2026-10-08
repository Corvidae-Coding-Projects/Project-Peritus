//! Clean immutable C1 snapshot binding for quality execution.

use core::fmt;
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use peritus_process::{ExecutionPlan, WorkingDirectory, WorkspaceAccess};
use peritus_types::{EnvironmentId, ResourceId, RevisionTuple, Sha256Digest};
use peritus_workspace::{FolderIdentity, FolderInspection, ReadOnlyWorkspace};
use sha2::{Digest, Sha256};

use crate::{CheckDefinition, QualityError, QualityErrorKind};

/// Revalidated clean C1 snapshot observation bound to one exact D1 revision and target.
#[derive(Clone, Eq, PartialEq)]
pub struct CleanQualitySnapshot {
    revision: RevisionTuple,
    environment_id: EnvironmentId,
    resource_id: ResourceId,
    canonical_root: PathBuf,
    root_identity: FolderIdentity,
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
        let root_identity = FolderIdentity::observe(workspace.root()).map_err(|error| {
            QualityError::new(
                QualityErrorKind::Workspace,
                format!("immutable quality snapshot root could not be identified: {error}"),
            )
        })?;
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
            canonical_root: root_identity.root().to_owned(),
            root_identity,
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

    /// Resolves and opens the check's declared directory under this exact immutable snapshot.
    ///
    /// # Errors
    /// Rejects an unavailable, linked, non-directory, replaced, or escaping declaration.
    pub fn resolve_working_directory(
        &self,
        definition: &CheckDefinition,
    ) -> Result<WorkingDirectory, QualityError> {
        let resolved = self.resolve_declared_directory(definition)?;
        let directory = WorkingDirectory::open(
            &resolved,
            self.revision.workspace_id(),
            self.resource_id,
            self.environment_id,
            self.revision.workspace_generation(),
            self.revision.workspace_revision(),
            WorkspaceAccess::ReadOnly,
        )?;
        if directory.path() != resolved {
            return Err(invalid(
                "C2 canonicalized the declared quality directory to another native path",
            ));
        }
        Ok(directory)
    }

    pub(crate) fn validate_plan_target(&self, plan: &ExecutionPlan) -> Result<(), QualityError> {
        let directory = plan.working_directory();
        let identity = plan.identity();
        if directory.access() != WorkspaceAccess::ReadOnly
            || !directory.path().starts_with(&self.canonical_root)
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

    pub(crate) fn validate_plan(
        &self,
        plan: &ExecutionPlan,
        definition: &CheckDefinition,
    ) -> Result<(), QualityError> {
        self.validate_plan_target(plan)?;
        if plan.working_directory().path() != self.resolve_declared_directory(definition)? {
            return Err(invalid(
                "C2 quality plan working directory differs from the declared snapshot directory",
            ));
        }
        Ok(())
    }

    pub(crate) fn validate_write_roots(
        &self,
        process_root: &Path,
        artifact_root: &Path,
    ) -> Result<(), QualityError> {
        validate_write_roots(&self.canonical_root, process_root, artifact_root)
    }

    pub(crate) fn execution_binding(
        &self,
        definition: &CheckDefinition,
        plan: &ExecutionPlan,
    ) -> Result<SnapshotExecutionBinding, QualityError> {
        self.validate_plan(plan, definition)?;
        Ok(SnapshotExecutionBinding {
            snapshot_digest: self.binding_digest,
            root: self.canonical_root.clone(),
            working_directory: plan.working_directory().path().to_owned(),
        })
    }

    fn resolve_declared_directory(
        &self,
        definition: &CheckDefinition,
    ) -> Result<PathBuf, QualityError> {
        FolderInspection::open(&self.root_identity)?
            .resolve_directory(definition.working_directory())
            .map_err(Into::into)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SnapshotExecutionBinding {
    snapshot_digest: Sha256Digest,
    root: PathBuf,
    working_directory: PathBuf,
}

impl SnapshotExecutionBinding {
    pub(crate) fn restore(
        snapshot_digest: Sha256Digest,
        root: PathBuf,
        working_directory: PathBuf,
        definition: &CheckDefinition,
    ) -> Result<Self, QualityError> {
        let expected = declared_directory(&root, definition);
        if !normal_absolute(&root)
            || !normal_absolute(&working_directory)
            || working_directory != expected
            || !working_directory.starts_with(&root)
        {
            return Err(invalid(
                "checkpointed quality directory differs from its snapshot-relative declaration",
            ));
        }
        Ok(Self { snapshot_digest, root, working_directory })
    }

    pub(crate) const fn snapshot_digest(&self) -> Sha256Digest {
        self.snapshot_digest
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn working_directory(&self) -> &Path {
        &self.working_directory
    }

    pub(crate) fn validate_plan(
        &self,
        plan: &ExecutionPlan,
        definition: &CheckDefinition,
    ) -> Result<(), QualityError> {
        if plan.working_directory().access() != WorkspaceAccess::ReadOnly
            || plan.working_directory().path() != self.working_directory
            || declared_directory(&self.root, definition) != self.working_directory
        {
            return Err(invalid(
                "retained C2 plan differs from the checkpointed snapshot directory binding",
            ));
        }
        Ok(())
    }

    pub(crate) fn validate_write_roots(
        &self,
        process_root: &Path,
        artifact_root: &Path,
    ) -> Result<(), QualityError> {
        validate_write_roots(&self.root, process_root, artifact_root)
    }
}

impl fmt::Debug for CleanQualitySnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CleanQualitySnapshot")
            .field("revision", &self.revision)
            .field("environment_id", &self.environment_id)
            .field("resource_id", &self.resource_id)
            .field("canonical_root", &self.canonical_root)
            .field("status_digest", &self.status_digest)
            .field("binding_digest", &self.binding_digest)
            .finish_non_exhaustive()
    }
}

fn declared_directory(root: &Path, definition: &CheckDefinition) -> PathBuf {
    definition
        .working_directory()
        .map_or_else(|| root.to_owned(), |path| root.join(path.as_str()))
}

fn normal_absolute(path: &Path) -> bool {
    path.is_absolute()
        && path.components().all(|component| {
            matches!(component, Component::Prefix(_) | Component::RootDir | Component::Normal(_))
        })
}

fn validate_write_roots(
    snapshot_root: &Path,
    process_root: &Path,
    artifact_root: &Path,
) -> Result<(), QualityError> {
    for write_root in [process_root, artifact_root] {
        let write_root = fs::canonicalize(write_root).map_err(|error| {
            QualityError::new(
                QualityErrorKind::InvalidInput,
                format!("quality write root cannot be canonicalized: {error}"),
            )
        })?;
        if write_root.starts_with(snapshot_root) || snapshot_root.starts_with(&write_root) {
            return Err(invalid(
                "quality process, checkpoint, and artifact roots must be outside the snapshot",
            ));
        }
    }
    Ok(())
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
