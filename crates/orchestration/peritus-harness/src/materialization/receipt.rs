//! Exact successful materialization receipt model and C1 validation.

use peritus_patch::{FileMode, PatchIdentity, Preimage, WorkspacePath};
use peritus_types::{ActionId, EventId, HarnessId, Sha256Digest, SnapshotId};
use peritus_workspace::{CandidateOutcome, MutationOutcome};

use crate::domain::RevisionDigest;

use super::{
    MaterializationError, MaterializationErrorKind, MaterializationPlan, MaterializationPlanId,
    MaterializationRecovery, PlannedFileOperation, WorkspaceSnapshot,
};

/// Stable compact identity derived from a complete receipt digest.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MaterializationReceiptId([u8; 16]);

impl MaterializationReceiptId {
    /// Returns exact identity bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    pub(super) fn from_digest(digest: Sha256Digest) -> Self {
        let mut bytes = [0_u8; 16];
        bytes.copy_from_slice(&digest.as_bytes()[..16]);
        bytes[0] |= 0x40;
        Self(bytes)
    }

    pub(crate) fn decode(bytes: [u8; 16]) -> Result<Self, MaterializationError> {
        if bytes == [0; 16] {
            return Err(invalid("materialization receipt identity is zero"));
        }
        Ok(Self(bytes))
    }
}

/// One exact installed regular file retained by a receipt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptFile {
    pub(super) path: WorkspacePath,
    pub(super) digest: Sha256Digest,
    pub(super) byte_length: u64,
    pub(super) mode: FileMode,
}

impl ReceiptFile {
    /// Constructs a checked output-file observation.
    #[must_use]
    pub const fn new(
        path: WorkspacePath,
        digest: Sha256Digest,
        byte_length: u64,
        mode: FileMode,
    ) -> Self {
        Self { path, digest, byte_length, mode }
    }

    /// Returns the canonical target path.
    #[must_use]
    pub const fn path(&self) -> &WorkspacePath {
        &self.path
    }
    /// Returns the exact output content digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
    /// Returns the exact output byte length.
    #[must_use]
    pub const fn byte_length(&self) -> u64 {
        self.byte_length
    }
    /// Returns the portable output mode.
    #[must_use]
    pub const fn mode(&self) -> FileMode {
        self.mode
    }
    /// Returns the exact present-file preimage this receipt proves.
    #[must_use]
    pub const fn preimage(&self) -> Preimage {
        Preimage::present(self.digest, self.byte_length, self.mode)
    }
}

/// Durable exact evidence of a successful C1 patch and candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterializationReceipt {
    pub(super) id: MaterializationReceiptId,
    pub(super) digest: Sha256Digest,
    pub(super) plan_id: MaterializationPlanId,
    pub(super) plan_digest: Sha256Digest,
    pub(super) harness_id: HarnessId,
    pub(super) revision_digest: RevisionDigest,
    pub(super) prior_receipt: Option<MaterializationReceiptId>,
    pub(super) patch_id: Sha256Digest,
    pub(super) patch_action_id: ActionId,
    pub(super) patch_authorization_digest: Sha256Digest,
    pub(super) candidate_action_id: ActionId,
    pub(super) candidate_authorization_digest: Sha256Digest,
    pub(super) before: WorkspaceSnapshot,
    pub(super) after: WorkspaceSnapshot,
    pub(super) snapshot_id: SnapshotId,
    pub(super) workspace_manifest_artifact: Sha256Digest,
    pub(super) files: Vec<ReceiptFile>,
    pub(super) started_at_millis: u64,
    pub(super) completed_at_millis: u64,
    pub(super) causal_event_id: EventId,
}

impl MaterializationReceipt {
    #[cfg(test)]
    pub(crate) fn test_fixture(
        revision: &crate::domain::HarnessRevision,
        installed: WorkspaceSnapshot,
    ) -> Self {
        let plan_digest = Sha256Digest::new([21; 32]);
        let mut receipt = Self {
            id: MaterializationReceiptId([0; 16]),
            digest: Sha256Digest::new([0; 32]),
            plan_id: MaterializationPlanId::from_digest(plan_digest),
            plan_digest,
            harness_id: revision.harness_id(),
            revision_digest: revision.digest(),
            prior_receipt: None,
            patch_id: Sha256Digest::new([22; 32]),
            patch_action_id: ActionId::new([23; 16]).expect("test patch action"),
            patch_authorization_digest: Sha256Digest::new([24; 32]),
            candidate_action_id: ActionId::new([25; 16]).expect("test candidate action"),
            candidate_authorization_digest: Sha256Digest::new([26; 32]),
            before: installed.clone(),
            after: installed,
            snapshot_id: SnapshotId::new([27; 16]).expect("test snapshot"),
            workspace_manifest_artifact: Sha256Digest::new([28; 32]),
            files: Vec::new(),
            started_at_millis: 10,
            completed_at_millis: 11,
            causal_event_id: EventId::new([29; 16]).expect("test causal event"),
        };
        receipt.digest = peritus_codec::sha256(
            &receipt.encode_without_identity().expect("test receipt must encode"),
        );
        receipt.id = MaterializationReceiptId::from_digest(receipt.digest);
        receipt
    }

    /// Constructs and cross-checks a complete receipt from exact C1 outcomes.
    ///
    /// # Errors
    /// Rejects mismatched plan, patch, workspace, action, snapshot, timing, or output inventory.
    #[allow(clippy::too_many_arguments, reason = "receipt evidence remains explicit and auditable")]
    pub fn from_c1(
        plan: &MaterializationPlan,
        mutation: &MutationOutcome,
        candidate: &CandidateOutcome,
        patch_action_id: ActionId,
        patch_authorization_digest: Sha256Digest,
        candidate_action_id: ActionId,
        candidate_authorization_digest: Sha256Digest,
        started_at_millis: u64,
        completed_at_millis: u64,
    ) -> Result<Self, MaterializationError> {
        if mutation.patch_identity() != candidate.patch_id()
            || mutation.action_id() != patch_action_id
            || candidate.action_id() != candidate_action_id
            || mutation.workspace_id() != plan.target().workspace_id()
            || mutation.generation() != plan.target().generation()
            || mutation.revision() != plan.target().revision()
            || candidate.identity().workspace_id() != plan.target().workspace_id()
            || candidate.identity().generation() != plan.target().generation()
            || candidate.identity().revision().get()
                != plan.target().revision().get().saturating_add(1)
            || completed_at_millis < started_at_millis
        {
            return Err(MaterializationError::new(
                MaterializationErrorKind::Receipt,
                MaterializationRecovery::Reconcile,
                "C1 outcomes do not exactly match the committed materialization plan",
            ));
        }
        let mut files = plan
            .operations()
            .iter()
            .filter_map(|operation| match operation {
                PlannedFileOperation::Install {
                    path, artifact_digest, byte_length, mode, ..
                } => Some(ReceiptFile::new(path.clone(), *artifact_digest, *byte_length, *mode)),
                PlannedFileOperation::Delete { .. } => None,
            })
            .collect::<Vec<_>>();
        files.sort_unstable_by(|left, right| left.path.cmp(&right.path));
        let mut receipt = Self {
            id: MaterializationReceiptId([0; 16]),
            digest: Sha256Digest::new([0; 32]),
            plan_id: plan.id(),
            plan_digest: plan.digest(),
            harness_id: plan.harness_id(),
            revision_digest: plan.revision_digest(),
            prior_receipt: plan.prior_receipt(),
            patch_id: patch_digest(mutation.patch_identity()),
            patch_action_id,
            patch_authorization_digest,
            candidate_action_id,
            candidate_authorization_digest,
            before: plan.target().clone(),
            after: WorkspaceSnapshot::from_c1(candidate.identity()),
            snapshot_id: candidate.snapshot().snapshot_id(),
            workspace_manifest_artifact: candidate.artifact_digest().sha256(),
            files,
            started_at_millis,
            completed_at_millis,
            causal_event_id: plan.causal_event_id(),
        };
        receipt.digest = peritus_codec::sha256(&receipt.encode_without_identity()?);
        receipt.id = MaterializationReceiptId::from_digest(receipt.digest);
        Ok(receipt)
    }

    /// Returns the receipt identity.
    #[must_use]
    pub const fn id(&self) -> MaterializationReceiptId {
        self.id
    }
    /// Returns its complete canonical digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
    /// Returns the committed plan identity.
    #[must_use]
    pub const fn plan_id(&self) -> MaterializationPlanId {
        self.plan_id
    }
    /// Returns the complete plan digest.
    #[must_use]
    pub const fn plan_digest(&self) -> Sha256Digest {
        self.plan_digest
    }
    /// Returns the harness lineage.
    #[must_use]
    pub const fn harness_id(&self) -> HarnessId {
        self.harness_id
    }
    /// Returns the source revision digest.
    #[must_use]
    pub const fn revision_digest(&self) -> RevisionDigest {
        self.revision_digest
    }
    /// Returns the prior receipt, when present.
    #[must_use]
    pub const fn prior_receipt(&self) -> Option<MaterializationReceiptId> {
        self.prior_receipt
    }
    /// Returns the applied C1 patch digest.
    #[must_use]
    pub const fn patch_id(&self) -> Sha256Digest {
        self.patch_id
    }
    /// Returns the patch authorization action.
    #[must_use]
    pub const fn patch_action_id(&self) -> ActionId {
        self.patch_action_id
    }
    /// Returns the candidate authorization action.
    #[must_use]
    pub const fn candidate_action_id(&self) -> ActionId {
        self.candidate_action_id
    }
    /// Returns the workspace state before mutation.
    #[must_use]
    pub const fn before(&self) -> &WorkspaceSnapshot {
        &self.before
    }
    /// Returns the clean immutable successor state.
    #[must_use]
    pub const fn after(&self) -> &WorkspaceSnapshot {
        &self.after
    }
    /// Returns the retained C1 snapshot identity.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }
    /// Returns the finalized C1 workspace-manifest artifact.
    #[must_use]
    pub const fn workspace_manifest_artifact(&self) -> Sha256Digest {
        self.workspace_manifest_artifact
    }
    /// Returns the exact path-sorted installed inventory.
    #[must_use]
    pub fn files(&self) -> &[ReceiptFile] {
        &self.files
    }
    /// Returns the causal event identity.
    #[must_use]
    pub const fn causal_event_id(&self) -> EventId {
        self.causal_event_id
    }

    /// Returns whether this receipt proves exact ownership of one observed path.
    #[must_use]
    pub fn owns_exact(&self, path: &WorkspacePath, preimage: Preimage) -> bool {
        self.files
            .binary_search_by(|file| file.path.cmp(path))
            .ok()
            .is_some_and(|index| self.files[index].preimage() == preimage)
    }
}

const fn patch_digest(value: PatchIdentity) -> Sha256Digest {
    value.digest()
}

fn invalid(detail: &'static str) -> MaterializationError {
    MaterializationError::new(
        MaterializationErrorKind::Receipt,
        MaterializationRecovery::Quarantine,
        detail,
    )
}
