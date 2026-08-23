//! Bounded final file content with exact computed identity.

use peritus_types::Sha256Digest;

use crate::{
    ErrorCode, FileMode, LineEndingPolicy, PatchError, PatchOperationContext, RecoveryClass,
    RollbackStatus,
};

/// Complete final bytes, digest, size, mode, and line-ending intent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinalFile {
    bytes: Vec<u8>,
    digest: Sha256Digest,
    size: u64,
    mode: FileMode,
    line_endings: LineEndingPolicy,
}

impl FinalFile {
    /// Applies the requested line-ending policy, checks the per-file bound, and computes identity.
    ///
    /// # Errors
    ///
    /// Returns invalid-content or overflow for non-text normalization and oversized content.
    pub fn new(
        bytes: Vec<u8>,
        mode: FileMode,
        line_endings: LineEndingPolicy,
    ) -> Result<Self, PatchError> {
        let bytes = line_endings.transform(bytes)?;
        if bytes.len() > crate::set::MAX_FILE_BYTES {
            return Err(PatchError::message(
                ErrorCode::InvalidPatchBounds,
                RecoveryClass::CorrectPatch,
                PatchOperationContext::Plan,
                RollbackStatus::NotRequired,
                "final file exceeds the per-file byte bound",
            ));
        }
        let size = u64::try_from(bytes.len()).map_err(|_| {
            PatchError::message(
                ErrorCode::ArithmeticOverflow,
                RecoveryClass::CorrectPatch,
                PatchOperationContext::Plan,
                RollbackStatus::NotRequired,
                "final file length cannot be represented",
            )
        })?;
        let digest = peritus_codec::sha256(&bytes);
        Ok(Self { bytes, digest, size, mode, line_endings })
    }

    /// Borrows the exact transformed final bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// Returns SHA-256 over exact final bytes.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
    /// Returns exact final size.
    #[must_use]
    pub const fn size(&self) -> u64 {
        self.size
    }
    /// Returns requested portable mode.
    #[must_use]
    pub const fn mode(&self) -> FileMode {
        self.mode
    }
    /// Returns the applied line-ending policy.
    #[must_use]
    pub const fn line_endings(&self) -> LineEndingPolicy {
        self.line_endings
    }
}
