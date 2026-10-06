//! Stable patch failures and recovery guidance.

use std::{error::Error, fmt, io};

use crate::WorkspacePath;

/// Stable machine-readable patch failure category.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum ErrorCode {
    /// A workspace-relative path was malformed or outside supported bounds.
    InvalidPath,
    /// The path names protected Git, Peritus, or nested-repository metadata.
    ProtectedPath,
    /// A patch contained no operations; the historical textual code is retained.
    InvalidPatchBounds,
    /// More than one operation named the same target.
    DuplicateTarget,
    /// One target was an ancestor of another target.
    TargetShapeConflict,
    /// The current workspace identity, generation, or revision is stale.
    StaleWorkspace,
    /// Declared bytes, sizes, digests, or line-ending intent disagreed.
    InvalidContent,
    /// The current file was absent when a present preimage was required.
    PreimageMissing,
    /// The current file existed when absence was required.
    PreimageUnexpected,
    /// Current bytes, length, or mode did not match the preimage.
    PreimageMismatch,
    /// A target or ancestor was a symlink, special node, or nested repository.
    UnsafeFilesystemTarget,
    /// The workspace and transaction roots are not safe, separate directories.
    InvalidTransactionRoot,
    /// A transaction already exists and must be recovered first.
    InterruptedTransaction,
    /// A transaction manifest was malformed, unsupported, or inconsistent.
    CorruptManifest,
    /// A checked arithmetic operation overflowed.
    ArithmeticOverflow,
    /// A filesystem effect or canonical metadata allocation failed.
    Io,
    /// Rollback could not prove restoration of every original.
    Indeterminate,
}

impl ErrorCode {
    /// Returns the compatibility-stable textual code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidPath => "patch.invalid_path",
            Self::ProtectedPath => "patch.protected_path",
            Self::InvalidPatchBounds => "patch.invalid_bounds",
            Self::DuplicateTarget => "patch.duplicate_target",
            Self::TargetShapeConflict => "patch.target_shape_conflict",
            Self::StaleWorkspace => "patch.stale_workspace",
            Self::InvalidContent => "patch.invalid_content",
            Self::PreimageMissing => "patch.preimage_missing",
            Self::PreimageUnexpected => "patch.preimage_unexpected",
            Self::PreimageMismatch => "patch.preimage_mismatch",
            Self::UnsafeFilesystemTarget => "patch.unsafe_filesystem_target",
            Self::InvalidTransactionRoot => "patch.invalid_transaction_root",
            Self::InterruptedTransaction => "patch.interrupted_transaction",
            Self::CorruptManifest => "patch.corrupt_manifest",
            Self::ArithmeticOverflow => "patch.arithmetic_overflow",
            Self::Io => "patch.io",
            Self::Indeterminate => "patch.indeterminate",
        }
    }
}

/// Recommended caller response to a patch failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum RecoveryClass {
    /// Correct malformed or conflicting patch input.
    CorrectPatch,
    /// Reinspect the workspace and construct fresh preimages.
    ReinspectWorkspace,
    /// Obtain authority for the current workspace generation and revision.
    Reauthorize,
    /// Retry the same operation identity after a transient filesystem or capacity failure.
    Retry,
    /// Run transaction recovery before accepting more mutation.
    RecoverTransaction,
    /// Fence the workspace; automatic retry cannot establish safety.
    FenceWorkspace,
}

/// Narrow operation context retained for diagnostics.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum PatchOperationContext {
    /// Validate a caller-supplied path.
    ValidatePath,
    /// Validate and canonicalize a patch set.
    Plan,
    /// Inspect a current target.
    InspectPreimage,
    /// Prepare a protected transaction directory.
    Prepare,
    /// Write and synchronize a staged final file.
    StageFinal,
    /// Persist or replace the recovery manifest.
    PersistManifest,
    /// Move an original into protected backup storage.
    BackupOriginal,
    /// Install a staged final file or deletion.
    InstallFinal,
    /// Synchronize an affected directory.
    SynchronizeDirectory,
    /// Verify installed results.
    VerifyResult,
    /// Restore original targets after failure.
    Rollback,
    /// Reconcile an interrupted transaction after restart.
    Recover,
    /// Remove completed transaction data.
    Cleanup,
}

/// Whether an ordinary application error restored the pre-transaction state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RollbackStatus {
    /// No target mutation occurred.
    NotRequired,
    /// Every original was restored and verified.
    Restored,
    /// Restoration could not be proved; recovery data remains.
    Indeterminate,
}

/// Typed patch error with bounded static detail and an optional I/O or capacity source.
#[derive(Debug)]
pub struct PatchError {
    code: ErrorCode,
    recovery: RecoveryClass,
    operation: PatchOperationContext,
    rollback: RollbackStatus,
    path: Option<WorkspacePath>,
    detail: &'static str,
    source: Option<io::Error>,
}

impl PatchError {
    pub(crate) const fn message(
        code: ErrorCode,
        recovery: RecoveryClass,
        operation: PatchOperationContext,
        rollback: RollbackStatus,
        detail: &'static str,
    ) -> Self {
        Self { code, recovery, operation, rollback, path: None, detail, source: None }
    }

    pub(crate) fn at(mut self, path: WorkspacePath) -> Self {
        self.path = Some(path);
        self
    }

    pub(crate) fn with_io_source(mut self, source: io::Error) -> Self {
        self.source = Some(source);
        self
    }

    pub(crate) fn with_rollback(mut self, rollback: RollbackStatus) -> Self {
        self.rollback = rollback;
        if rollback == RollbackStatus::Indeterminate {
            self.recovery = RecoveryClass::FenceWorkspace;
        }
        self
    }

    pub(crate) fn io(
        operation: PatchOperationContext,
        rollback: RollbackStatus,
        source: io::Error,
    ) -> Self {
        Self {
            code: ErrorCode::Io,
            recovery: if rollback == RollbackStatus::Indeterminate {
                RecoveryClass::FenceWorkspace
            } else {
                RecoveryClass::Retry
            },
            operation,
            rollback,
            path: None,
            detail: "filesystem operation failed",
            source: Some(source),
        }
    }

    pub(crate) fn codec_allocation(
        operation: PatchOperationContext,
        source: peritus_codec::CodecError,
    ) -> Self {
        Self::message(
            ErrorCode::Io,
            RecoveryClass::Retry,
            operation,
            RollbackStatus::NotRequired,
            "canonical metadata allocation is temporarily unavailable",
        )
        .with_io_source(io::Error::new(io::ErrorKind::OutOfMemory, source))
    }

    pub(crate) const fn indeterminate(operation: PatchOperationContext) -> Self {
        Self::message(
            ErrorCode::Indeterminate,
            RecoveryClass::FenceWorkspace,
            operation,
            RollbackStatus::Indeterminate,
            "workspace restoration could not be proved",
        )
    }

    /// Returns the stable failure category.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        self.code
    }

    /// Returns recommended recovery guidance.
    #[must_use]
    pub const fn recovery_class(&self) -> RecoveryClass {
        self.recovery
    }

    /// Returns the failed operation boundary.
    #[must_use]
    pub const fn operation(&self) -> PatchOperationContext {
        self.operation
    }

    /// Returns the observed rollback status.
    #[must_use]
    pub const fn rollback_status(&self) -> RollbackStatus {
        self.rollback
    }

    /// Returns the checked workspace-relative path when one is relevant.
    #[must_use]
    pub const fn path(&self) -> Option<&WorkspacePath> {
        self.path.as_ref()
    }
}

impl fmt::Display for PatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code.as_str(), self.detail)?;
        if let Some(path) = &self.path {
            write!(formatter, " ({})", path.as_str())?;
        }
        Ok(())
    }
}

impl Error for PatchError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_ref().map(|source| source as &(dyn Error + 'static))
    }
}

/// Codec capacity never selects a different canonical identity or durable representation.
pub fn codec_encoding_fits(
    result: Result<(), peritus_codec::CodecError>,
    operation: PatchOperationContext,
) -> Result<bool, PatchError> {
    match result {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == peritus_codec::CodecErrorKind::AllocationUnavailable => {
            Err(PatchError::codec_allocation(operation, error))
        }
        Err(_) => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use peritus_codec::{CodecError, CodecErrorKind};

    #[test]
    fn allocation_pressure_does_not_select_a_different_representation() {
        let source = CodecError::at(CodecErrorKind::AllocationUnavailable, 19);
        let error = codec_encoding_fits(Err(source), PatchOperationContext::Plan).unwrap_err();
        assert_eq!(error.recovery_class(), RecoveryClass::Retry);
        assert_eq!(error.rollback_status(), RollbackStatus::NotRequired);
        let io = error.source().unwrap().downcast_ref::<io::Error>().unwrap();
        assert_eq!(io.kind(), io::ErrorKind::OutOfMemory);
        assert_eq!(io.get_ref().unwrap().downcast_ref::<CodecError>(), Some(&source));
        assert!(codec_encoding_fits(Ok(()), PatchOperationContext::Plan).unwrap());
        assert!(
            !codec_encoding_fits(
                Err(CodecError::at(CodecErrorKind::LimitExceeded, 19)),
                PatchOperationContext::Plan,
            )
            .unwrap()
        );
    }
}
