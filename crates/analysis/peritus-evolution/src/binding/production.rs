//! Exact E1 production-harness and installed-snapshot bindings.

use crate::{
    EvolutionError, EvolutionErrorKind, EvolutionOperation, EvolutionRecovery,
    identity::digest_parts,
};
use peritus_harness::{GoverningHarnessBinding, domain::HarnessRevisionIdentity};
use peritus_types::{Generation, RevisionNumber, RevisionTuple, Sha256Digest, WorkspaceId};

/// Compact exact identity of the C1 snapshot installed by E1.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InstalledSnapshotBinding {
    workspace_id: WorkspaceId,
    generation: Generation,
    revision: RevisionNumber,
    commit_format: u8,
    commit: [u8; 32],
    tree_format: u8,
    tree: [u8; 32],
    digest: Sha256Digest,
}

impl InstalledSnapshotBinding {
    fn capture(binding: &GoverningHarnessBinding) -> Self {
        let snapshot = binding.installed_snapshot();
        Self::from_replay_parts(
            snapshot.workspace_id(),
            snapshot.generation(),
            snapshot.revision(),
            object(snapshot.commit().as_bytes()),
            object(snapshot.tree().as_bytes()),
        )
    }

    pub(crate) fn from_replay_parts(
        workspace_id: WorkspaceId,
        generation: Generation,
        revision: RevisionNumber,
        commit: (u8, [u8; 32]),
        tree: (u8, [u8; 32]),
    ) -> Self {
        let generation_bytes = generation.get().to_be_bytes();
        let revision_bytes = revision.get().to_be_bytes();
        let digest = digest_parts(
            b"peritus.f0.installed-snapshot.v1\0",
            &[
                workspace_id.as_bytes(),
                &generation_bytes,
                &revision_bytes,
                &[commit.0],
                &commit.1,
                &[tree.0],
                &tree.1,
            ],
        );
        Self {
            workspace_id,
            generation,
            revision,
            commit_format: commit.0,
            commit: commit.1,
            tree_format: tree.0,
            tree: tree.1,
            digest,
        }
    }

    /// Returns the workspace lineage.
    #[must_use]
    pub const fn workspace_id(self) -> WorkspaceId {
        self.workspace_id
    }
    /// Returns the exact workspace generation.
    #[must_use]
    pub const fn generation(self) -> Generation {
        self.generation
    }
    /// Returns the exact workspace revision.
    #[must_use]
    pub const fn revision(self) -> RevisionNumber {
        self.revision
    }
    /// Returns the Git object-format tag and padded exact installed commit bytes.
    #[must_use]
    pub const fn commit(self) -> (u8, [u8; 32]) {
        (self.commit_format, self.commit)
    }
    /// Returns the Git object-format tag and padded exact installed tree bytes.
    #[must_use]
    pub const fn tree(self) -> (u8, [u8; 32]) {
        (self.tree_format, self.tree)
    }
    /// Returns the digest of the complete installed snapshot identity.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.digest
    }
}

fn object(value: &[u8]) -> (u8, [u8; 32]) {
    let mut bytes = [0_u8; 32];
    bytes[..value.len()].copy_from_slice(value);
    (if value.len() == 20 { 1 } else { 2 }, bytes)
}

/// Exact immutable production harness, receipt, and installed snapshot identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProductionHarnessBinding {
    revision: RevisionTuple,
    harness_revision: HarnessRevisionIdentity,
    materialization_receipt_digest: Sha256Digest,
    installed_snapshot: InstalledSnapshotBinding,
    digest: Sha256Digest,
}

impl ProductionHarnessBinding {
    /// Captures an already cross-checked E1 governing binding.
    ///
    /// # Errors
    /// Rejects a binding whose shared tuple and installed snapshot have drifted.
    pub fn capture(binding: &GoverningHarnessBinding) -> Result<Self, EvolutionError> {
        let snapshot = InstalledSnapshotBinding::capture(binding);
        let revision = binding.revision();
        if revision.workspace_id() != snapshot.workspace_id()
            || revision.workspace_generation() != snapshot.generation()
            || revision.workspace_revision() != snapshot.revision()
        {
            return Err(EvolutionError::new(
                EvolutionErrorKind::BindingDrift,
                EvolutionOperation::BindProductionHarness,
                EvolutionRecovery::CorrectInput,
                "governing harness and installed snapshot differ",
            ));
        }
        Ok(Self::from_exact_parts(
            revision,
            binding.harness_revision(),
            binding.materialization().digest(),
            snapshot,
        ))
    }

    pub(crate) fn from_exact_parts(
        revision: RevisionTuple,
        harness_revision: HarnessRevisionIdentity,
        materialization_receipt_digest: Sha256Digest,
        installed_snapshot: InstalledSnapshotBinding,
    ) -> Self {
        let harness_number = harness_revision.number().get().to_be_bytes();
        let digest = digest_parts(
            b"peritus.f0.production-harness-binding.v1\0",
            &[
                peritus_evidence::revision_digest(&revision).as_bytes(),
                harness_revision.harness_id().as_bytes(),
                &harness_number,
                harness_revision.digest().as_bytes(),
                materialization_receipt_digest.as_bytes(),
                installed_snapshot.digest().as_bytes(),
            ],
        );
        Self {
            revision,
            harness_revision,
            materialization_receipt_digest,
            installed_snapshot,
            digest,
        }
    }

    /// Returns the unchanged shared authority/evidence tuple.
    #[must_use]
    pub const fn revision(self) -> RevisionTuple {
        self.revision
    }
    /// Returns the full branch-distinguishing E1 revision identity.
    #[must_use]
    pub const fn harness_revision(self) -> HarnessRevisionIdentity {
        self.harness_revision
    }
    /// Returns the exact E1 materialization receipt digest.
    #[must_use]
    pub const fn materialization_receipt_digest(self) -> Sha256Digest {
        self.materialization_receipt_digest
    }
    /// Returns the installed C1 snapshot identity.
    #[must_use]
    pub const fn installed_snapshot(self) -> InstalledSnapshotBinding {
        self.installed_snapshot
    }
    /// Returns the digest of every binding field.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.digest
    }
}
