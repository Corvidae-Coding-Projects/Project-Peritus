//! Recoverable staged-final and backup transaction adapter.

mod apply;
mod filesystem;
mod manifest;
mod recover;
mod recovery_observation;
mod roots;
mod storage;

#[cfg(test)]
mod tests;

use peritus_types::{Generation, RevisionNumber, Sha256Digest, WorkspaceId};

use crate::{
    ErrorCode, PatchError, PatchIdentity, PatchOperationContext, PatchSet, RecoveryClass,
    RollbackStatus,
};

pub use apply::apply_patch;
pub use manifest::TransactionPhase;
pub use recover::recover_transaction;

pub fn validate_patch_manifest_capacity(patch: &PatchSet) -> Result<(), PatchError> {
    manifest::validate_patch_capacity(patch).map_err(|_| {
        PatchError::message(
            ErrorCode::InvalidPatchBounds,
            RecoveryClass::CorrectPatch,
            PatchOperationContext::Plan,
            RollbackStatus::NotRequired,
            "patch recovery manifest exceeds a configured resource bound",
        )
    })
}

/// Stable named effect boundaries used by fault-injection tests and diagnostics.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum TransactionFaultPoint {
    /// Before creating transaction state.
    BeforePrepare,
    /// After one final file has been staged and synchronized.
    AfterStageFinal,
    /// After the prepared manifest is durable.
    AfterPreparedManifest,
    /// After the installing phase is durable and before target mutation.
    AfterInstallingManifest,
    /// After one previously absent workspace directory has been created and synchronized.
    AfterCreateDirectory,
    /// After an original has moved to backup storage.
    AfterBackupOriginal,
    /// After a final target mutation.
    AfterInstallFinal,
    /// Immediately before synchronizing an affected directory.
    BeforeDirectorySync,
    /// Before final postimage verification.
    BeforeVerifyResult,
    /// Before ordinary failure rollback.
    BeforeRollback,
    /// Before completed transaction cleanup.
    BeforeCleanup,
}

impl TransactionFaultPoint {
    /// Returns the stable diagnostic boundary name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BeforePrepare => "before_prepare",
            Self::AfterStageFinal => "after_stage_final",
            Self::AfterPreparedManifest => "after_prepared_manifest",
            Self::AfterInstallingManifest => "after_installing_manifest",
            Self::AfterCreateDirectory => "after_create_directory",
            Self::AfterBackupOriginal => "after_backup_original",
            Self::AfterInstallFinal => "after_install_final",
            Self::BeforeDirectorySync => "before_directory_sync",
            Self::BeforeVerifyResult => "before_verify_result",
            Self::BeforeRollback => "before_rollback",
            Self::BeforeCleanup => "before_cleanup",
        }
    }
}

trait FaultInjector {
    fn check(&self, point: TransactionFaultPoint) -> std::io::Result<()>;
}

struct NoFaults;

impl FaultInjector for NoFaults {
    fn check(&self, _point: TransactionFaultPoint) -> std::io::Result<()> {
        Ok(())
    }
}

/// Durable successful patch evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppliedPatch {
    identity: PatchIdentity,
    installed_manifest: Vec<u8>,
    manifest_digest: Sha256Digest,
    cleanup_pending: bool,
}

impl AppliedPatch {
    pub(crate) fn new(
        identity: PatchIdentity,
        installed_manifest: Vec<u8>,
        cleanup_pending: bool,
    ) -> Self {
        let manifest_digest = peritus_codec::sha256(&installed_manifest);
        Self { identity, installed_manifest, manifest_digest, cleanup_pending }
    }

    /// Returns the applied patch identity.
    #[must_use]
    pub const fn identity(&self) -> PatchIdentity {
        self.identity
    }
    /// Borrows exact installed-manifest bytes.
    #[must_use]
    pub fn installed_manifest(&self) -> &[u8] {
        &self.installed_manifest
    }
    /// Returns the SHA-256 digest of installed-manifest bytes.
    #[must_use]
    pub const fn manifest_digest(&self) -> Sha256Digest {
        self.manifest_digest
    }
    /// Reports that success is durable but transaction metadata needs later cleanup.
    #[must_use]
    pub const fn cleanup_pending(&self) -> bool {
        self.cleanup_pending
    }
}

/// Deterministic interrupted-transaction classification.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RecoveryState {
    /// Every postimage was present and verified.
    AlreadyApplied,
    /// Every preimage was restored and verified.
    RolledBackCleanly,
    /// A safe regular target matched neither declared preimage nor postimage.
    Dirty,
    /// Corruption or unsafe filesystem state prevented a conclusive classification.
    Indeterminate,
}

/// Exact workspace version that a restart-visible transaction belongs to.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RecoveryBinding {
    workspace_id: WorkspaceId,
    generation: Generation,
    revision: RevisionNumber,
}

impl RecoveryBinding {
    /// Creates an exact recovery binding from durable workspace state.
    #[must_use]
    pub const fn new(
        workspace_id: WorkspaceId,
        generation: Generation,
        revision: RevisionNumber,
    ) -> Self {
        Self { workspace_id, generation, revision }
    }

    /// Returns the owning workspace identity.
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
}

/// Result of inspecting and, when safe, resolving one restart-visible transaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryOutcome {
    state: RecoveryState,
    binding: Option<RecoveryBinding>,
    identity: Option<PatchIdentity>,
    quarantined: bool,
    cleanup_pending: bool,
}

impl RecoveryOutcome {
    pub(crate) const fn new(
        state: RecoveryState,
        binding: Option<RecoveryBinding>,
        identity: Option<PatchIdentity>,
        quarantined: bool,
        cleanup_pending: bool,
    ) -> Self {
        Self { state, binding, identity, quarantined, cleanup_pending }
    }

    /// Returns the exact recovery classification.
    #[must_use]
    pub const fn state(&self) -> RecoveryState {
        self.state
    }
    /// Returns the manifest's exact workspace binding when it decoded safely.
    #[must_use]
    pub const fn binding(&self) -> Option<RecoveryBinding> {
        self.binding
    }
    /// Returns the manifest patch identity when it decoded safely.
    #[must_use]
    pub const fn identity(&self) -> Option<PatchIdentity> {
        self.identity
    }
    /// Reports whether corrupt transaction metadata was moved out of the active namespace.
    #[must_use]
    pub const fn quarantined(&self) -> bool {
        self.quarantined
    }
    /// Reports that a conclusive result is durable but transaction cleanup did not finish.
    #[must_use]
    pub const fn cleanup_pending(&self) -> bool {
        self.cleanup_pending
    }
}
