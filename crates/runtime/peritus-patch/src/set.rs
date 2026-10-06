//! Canonical patch authority independent of inline content and message capacity.

use peritus_codec::{CanonicalWriter, CodecLimits};
use peritus_types::{Generation, RevisionNumber, WorkspaceId};

use crate::{
    ErrorCode, PatchError, PatchIdentity, PatchOperation, PatchOperationContext, PatchPlan,
    Preimage, RecoveryClass, RollbackStatus,
};

// Historical encoding classification only. Larger inputs use metadata authority and paged
// transaction storage; these values are never admission limits for a new patch.
pub const LEGACY_FILE_BYTES: usize = 8 * 1024 * 1024;
const LEGACY_PATCH_BYTES: usize = 8 * 1024 * 1024;
pub const LEGACY_PATCH_OPERATIONS: usize = 1_024;

/// Historical schema-one inline file capacity, retained for source compatibility.
/// This is not an admission ceiling for [`crate::FinalFile`] or [`PatchSet`].
pub const MAX_FILE_BYTES: usize = LEGACY_FILE_BYTES;
/// Historical inline aggregate capacity, retained for source compatibility.
/// This is not an admission ceiling for [`PatchSet`].
pub const MAX_PATCH_BYTES: usize = LEGACY_PATCH_BYTES;
/// Historical schema-one operation capacity, retained for source compatibility.
/// This is not an admission ceiling for [`PatchSet`].
pub const MAX_PATCH_OPERATIONS: usize = LEGACY_PATCH_OPERATIONS;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Representation {
    Inline,
    Snapshot,
    Paged,
}

/// Nonempty patch set in deterministic target-path order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PatchSet {
    workspace_id: WorkspaceId,
    expected_generation: Generation,
    expected_revision: RevisionNumber,
    operations: Vec<PatchOperation>,
    identity: PatchIdentity,
    representation: Representation,
}

impl PatchSet {
    /// Validates target conflicts, sorts by path, and computes stable authority.
    /// Owned streamed content is accepted alongside inline content. Size and operation count
    /// select a compatible representation rather than limit the authorized mutation.
    ///
    /// # Errors
    ///
    /// Returns a typed error for empty, duplicate, or ancestor-conflicting targets.
    pub fn new(
        workspace_id: WorkspaceId,
        expected_generation: Generation,
        expected_revision: RevisionNumber,
        operations: Vec<PatchOperation>,
    ) -> Result<Self, PatchError> {
        Self::checked(workspace_id, expected_generation, expected_revision, operations, false)
    }

    /// Plans restoration of streaming snapshots as one authorized, recoverable transaction.
    /// Content is identified by digest and size instead of encoded into command payloads.
    ///
    /// # Errors
    /// Rejects empty, duplicate, protected, conflicting or unencodable recovery metadata.
    pub fn from_snapshot(
        workspace_id: WorkspaceId,
        expected_generation: Generation,
        expected_revision: RevisionNumber,
        operations: Vec<PatchOperation>,
    ) -> Result<Self, PatchError> {
        Self::checked(workspace_id, expected_generation, expected_revision, operations, true)
    }

    fn checked(
        workspace_id: WorkspaceId,
        expected_generation: Generation,
        expected_revision: RevisionNumber,
        mut operations: Vec<PatchOperation>,
        snapshot: bool,
    ) -> Result<Self, PatchError> {
        if !crate::verified::patch_bounds_valid(operations.len()) {
            return Err(bounds_error());
        }
        operations.sort_unstable_by(|left, right| left.path().cmp(right.path()));
        for pair in operations.windows(2) {
            if pair[0].path() == pair[1].path() {
                return Err(PatchError::message(
                    ErrorCode::DuplicateTarget,
                    RecoveryClass::CorrectPatch,
                    PatchOperationContext::Plan,
                    RollbackStatus::NotRequired,
                    "patch contains duplicate target paths",
                )
                .at(pair[0].path().clone()));
            }
            if pair[0].path().is_ancestor_of(pair[1].path()) {
                return Err(PatchError::message(
                    ErrorCode::TargetShapeConflict,
                    RecoveryClass::CorrectPatch,
                    PatchOperationContext::Plan,
                    RollbackStatus::NotRequired,
                    "one patch target is an ancestor of another",
                )
                .at(pair[0].path().clone()));
            }
        }
        let legacy = if snapshot {
            None
        } else {
            legacy_inline_identity(
                workspace_id,
                expected_generation,
                expected_revision,
                &operations,
            )?
        };
        let mut patch = Self {
            workspace_id,
            expected_generation,
            expected_revision,
            identity: legacy.unwrap_or_else(|| {
                metadata_identity(
                    workspace_id,
                    expected_generation,
                    expected_revision,
                    &operations,
                    snapshot,
                )
            }),
            representation: if operations
                .iter()
                .any(|operation| !operation.path().is_legacy_portable())
            {
                Representation::Paged
            } else if snapshot {
                Representation::Snapshot
            } else if legacy.is_some() {
                Representation::Inline
            } else {
                Representation::Paged
            },
            operations,
        };
        if patch.representation == Representation::Inline
            && !crate::transaction::legacy_manifest_fits(&patch)?
        {
            patch.representation = Representation::Paged;
            patch.identity = metadata_identity(
                workspace_id,
                expected_generation,
                expected_revision,
                &patch.operations,
                false,
            );
        }
        Ok(patch)
    }

    pub(crate) const fn is_snapshot(&self) -> bool {
        matches!(self.representation, Representation::Snapshot)
    }

    pub(crate) const fn is_paged(&self) -> bool {
        matches!(self.representation, Representation::Paged)
    }

    pub(crate) fn has_extended_paths(&self) -> bool {
        self.operations.iter().any(|operation| !operation.path().is_legacy_portable())
    }

    /// Returns the bound workspace identity.
    #[must_use]
    pub const fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }

    /// Returns the required workspace generation.
    #[must_use]
    pub const fn expected_generation(&self) -> Generation {
        self.expected_generation
    }

    /// Returns the required workspace revision.
    #[must_use]
    pub const fn expected_revision(&self) -> RevisionNumber {
        self.expected_revision
    }

    /// Returns the stable identity over the complete canonical patch representation.
    #[must_use]
    pub const fn identity(&self) -> PatchIdentity {
        self.identity
    }

    /// Borrows canonical path-sorted operations.
    #[must_use]
    pub fn operations(&self) -> &[PatchOperation] {
        &self.operations
    }

    /// Converts this inert patch to an effect-capable plan after exact version validation.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorCode::StaleWorkspace`] unless identity, generation, and revision all match.
    pub fn plan(
        self,
        current_workspace_id: WorkspaceId,
        current_generation: Generation,
        current_revision: RevisionNumber,
    ) -> Result<PatchPlan, PatchError> {
        if !crate::verified::workspace_version_matches(
            self.workspace_id == current_workspace_id,
            self.expected_generation.get(),
            current_generation.get(),
            self.expected_revision.get(),
            current_revision.get(),
        ) {
            return Err(PatchError::message(
                ErrorCode::StaleWorkspace,
                RecoveryClass::Reauthorize,
                PatchOperationContext::Plan,
                RollbackStatus::NotRequired,
                "patch binding does not match the observed workspace version",
            ));
        }
        Ok(PatchPlan { patch: self })
    }
}

fn legacy_inline_identity(
    workspace_id: WorkspaceId,
    generation: Generation,
    revision: RevisionNumber,
    operations: &[PatchOperation],
) -> Result<Option<PatchIdentity>, PatchError> {
    let Some(total_bytes) = operations.iter().try_fold(0usize, |total, operation| {
        total.checked_add(operation.final_file().map_or(0, |file| file.bytes().len()))
    }) else {
        return Ok(None);
    };
    if operations.len() > LEGACY_PATCH_OPERATIONS
        || total_bytes > LEGACY_PATCH_BYTES
        || operations.iter().any(|operation| {
            !operation.path().is_legacy_portable()
                || operation.is_snapshot()
                || matches!(operation.preimage(), Preimage::Present { size, .. } if size > LEGACY_FILE_BYTES as u64)
                || operation.final_file().is_some_and(|file| file.bytes().len() > LEGACY_FILE_BYTES)
        })
    {
        return Ok(None);
    }
    let mut writer = CanonicalWriter::new(CodecLimits::LEGACY_V1);
    let encoded = (|| {
        let directories = operations.iter().any(PatchOperation::covers_directory);
        writer.write_fixed(if directories {
            b"peritus-patch-set-v3"
        } else {
            b"peritus-patch-set-v1"
        })?;
        writer.write_fixed(workspace_id.as_bytes())?;
        writer.write_u64(generation.get())?;
        writer.write_u64(revision.get())?;
        writer.write_collection_len(operations.len())?;
        for operation in operations {
            writer.write_u8(match operation.kind() {
                crate::PatchOperationKind::Create => 1,
                crate::PatchOperationKind::Replace => 2,
                crate::PatchOperationKind::Delete => 3,
                crate::PatchOperationKind::CreateDirectory => 4,
                crate::PatchOperationKind::DeleteDirectory => 5,
            })?;
            writer.write_str(operation.path().as_str())?;
            match operation.preimage() {
                Preimage::Absent => writer.write_u8(0)?,
                Preimage::EmptyDirectory { mode } => {
                    writer.write_u8(2)?;
                    writer.write_u16(mode.bits())?;
                }
                Preimage::Present { digest, size, mode } => {
                    writer.write_u8(1)?;
                    writer.write_fixed(digest.as_bytes())?;
                    writer.write_u64(size)?;
                    writer.write_u8(mode.tag())?;
                }
            }
            match operation.final_file() {
                None => match operation.postimage() {
                    Preimage::EmptyDirectory { mode } => {
                        writer.write_u8(2)?;
                        writer.write_u16(mode.bits())?;
                    }
                    _ => writer.write_u8(0)?,
                },
                Some(file) => {
                    writer.write_u8(1)?;
                    writer.write_fixed(file.digest().as_bytes())?;
                    writer.write_u64(file.size())?;
                    writer.write_u8(file.mode().tag())?;
                    writer.write_u8(file.line_endings().tag())?;
                    writer.write_bytes(file.bytes())?;
                }
            }
        }
        Ok::<(), peritus_codec::CodecError>(())
    })();
    // Historical contract overflow selects paged authority. Allocator pressure instead keeps
    // the same identity pending for retry; it never selects another representation.
    if !crate::error::codec_encoding_fits(encoded, PatchOperationContext::Plan)? {
        return Ok(None);
    }
    Ok(Some(PatchIdentity::new(peritus_codec::sha256(writer.as_slice()))))
}

fn metadata_identity(
    workspace_id: WorkspaceId,
    generation: Generation,
    revision: RevisionNumber,
    operations: &[PatchOperation],
    snapshot: bool,
) -> PatchIdentity {
    use sha2::{Digest as _, Sha256};

    let mut digest = Sha256::new();
    let directories = operations.iter().any(PatchOperation::covers_directory);
    let extended = operations.iter().any(|operation| !operation.path().is_legacy_portable());
    let domain: &[u8] = if extended {
        if snapshot { b"peritus-snapshot-v5\0" } else { b"peritus-patch-set-v5\0" }
    } else if !snapshot {
        b"peritus-patch-set-v4\0"
    } else if directories {
        b"peritus-snapshot-v3\0"
    } else {
        b"peritus-snapshot-v2\0"
    };
    digest.update(domain);
    if extended {
        digest.update([crate::path::native_platform_tag()]);
    }
    digest.update(workspace_id.as_bytes());
    digest.update(generation.get().to_be_bytes());
    digest.update(revision.get().to_be_bytes());
    digest.update((operations.len() as u64).to_be_bytes());
    for operation in operations {
        digest.update([match operation.kind() {
            crate::PatchOperationKind::Create => 1,
            crate::PatchOperationKind::Replace => 2,
            crate::PatchOperationKind::Delete => 3,
            crate::PatchOperationKind::CreateDirectory => 4,
            crate::PatchOperationKind::DeleteDirectory => 5,
        }]);
        digest.update((operation.path().as_str().len() as u64).to_be_bytes());
        digest.update(operation.path().as_str().as_bytes());
        for identity in [operation.preimage(), operation.postimage()] {
            match identity {
                Preimage::Absent => digest.update([0]),
                Preimage::EmptyDirectory { mode } => {
                    digest.update([2]);
                    digest.update(mode.bits().to_be_bytes());
                }
                Preimage::Present { digest: content, size, mode } => {
                    digest.update([1]);
                    digest.update(content.as_bytes());
                    digest.update(size.to_be_bytes());
                    digest.update([mode.tag()]);
                }
            }
        }
        if !snapshot {
            match operation.final_file() {
                Some(file) => digest.update([1, file.line_endings().tag()]),
                None => digest.update([0]),
            }
        }
    }
    PatchIdentity::new(peritus_types::Sha256Digest::new(digest.finalize().into()))
}

const fn bounds_error() -> PatchError {
    PatchError::message(
        ErrorCode::InvalidPatchBounds,
        RecoveryClass::CorrectPatch,
        PatchOperationContext::Plan,
        RollbackStatus::NotRequired,
        "patch must contain at least one operation",
    )
}

#[cfg(test)]
mod tests;
