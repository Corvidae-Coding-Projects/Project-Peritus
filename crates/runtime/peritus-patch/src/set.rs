//! Bounded canonical patch sets.

use peritus_codec::{CanonicalWriter, CodecLimits};
use peritus_types::{Generation, RevisionNumber, WorkspaceId};

use crate::{
    ErrorCode, PatchError, PatchIdentity, PatchOperation, PatchOperationContext, PatchPlan,
    Preimage, RecoveryClass, RollbackStatus,
};

/// Maximum exact final bytes carried by one file.
pub const MAX_FILE_BYTES: usize = 8 * 1024 * 1024;
/// Maximum aggregate final bytes in one patch.
pub const MAX_PATCH_BYTES: usize = 8 * 1024 * 1024;
/// Maximum operations in one patch.
pub const MAX_PATCH_OPERATIONS: usize = 1_024;

/// Nonempty bounded patch set in deterministic target-path order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PatchSet {
    workspace_id: WorkspaceId,
    expected_generation: Generation,
    expected_revision: RevisionNumber,
    operations: Vec<PatchOperation>,
    identity: PatchIdentity,
}

impl PatchSet {
    /// Validates bounds and target conflicts, sorts by path, and computes stable identity.
    ///
    /// # Errors
    ///
    /// Returns a typed error for empty/oversized, duplicate, or ancestor-conflicting targets.
    pub fn new(
        workspace_id: WorkspaceId,
        expected_generation: Generation,
        expected_revision: RevisionNumber,
        mut operations: Vec<PatchOperation>,
    ) -> Result<Self, PatchError> {
        let total_bytes = operations.iter().try_fold(0usize, |total, operation| {
            total
                .checked_add(operation.final_file().map_or(0, |file| file.bytes().len()))
                .ok_or_else(arithmetic_error)
        })?;
        if !crate::verified::patch_bounds_valid(
            operations.len(),
            total_bytes,
            MAX_PATCH_OPERATIONS,
            MAX_PATCH_BYTES,
        ) {
            return Err(bounds_error());
        }
        if operations.iter().any(|operation| {
            matches!(
                operation.preimage(),
                Preimage::Present { size, .. } if size > MAX_FILE_BYTES as u64
            )
        }) {
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
        let identity =
            canonical_identity(workspace_id, expected_generation, expected_revision, &operations)?;
        let patch =
            Self { workspace_id, expected_generation, expected_revision, operations, identity };
        crate::transaction::validate_patch_manifest_capacity(&patch)?;
        Ok(patch)
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

fn canonical_identity(
    workspace_id: WorkspaceId,
    generation: Generation,
    revision: RevisionNumber,
    operations: &[PatchOperation],
) -> Result<PatchIdentity, PatchError> {
    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    let encoded = (|| {
        writer.write_fixed(b"peritus-patch-set-v1")?;
        writer.write_fixed(workspace_id.as_bytes())?;
        writer.write_u64(generation.get())?;
        writer.write_u64(revision.get())?;
        writer.write_collection_len(operations.len())?;
        for operation in operations {
            writer.write_u8(match operation.kind() {
                crate::PatchOperationKind::Create => 1,
                crate::PatchOperationKind::Replace => 2,
                crate::PatchOperationKind::Delete => 3,
            })?;
            writer.write_str(operation.path().as_str())?;
            match operation.preimage() {
                Preimage::Absent => writer.write_u8(0)?,
                Preimage::Present { digest, size, mode } => {
                    writer.write_u8(1)?;
                    writer.write_fixed(digest.as_bytes())?;
                    writer.write_u64(size)?;
                    writer.write_u8(mode.tag())?;
                }
            }
            match operation.final_file() {
                None => writer.write_u8(0)?,
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
    encoded.map_err(|_| bounds_error())?;
    Ok(PatchIdentity::new(peritus_codec::sha256(writer.as_slice())))
}

const fn bounds_error() -> PatchError {
    PatchError::message(
        ErrorCode::InvalidPatchBounds,
        RecoveryClass::CorrectPatch,
        PatchOperationContext::Plan,
        RollbackStatus::NotRequired,
        "patch is empty or exceeds a configured resource bound",
    )
}

const fn arithmetic_error() -> PatchError {
    PatchError::message(
        ErrorCode::ArithmeticOverflow,
        RecoveryClass::CorrectPatch,
        PatchOperationContext::Plan,
        RollbackStatus::NotRequired,
        "patch aggregate byte count overflowed",
    )
}

#[cfg(test)]
mod tests {
    use peritus_types::{Generation, RevisionNumber, WorkspaceId};

    use crate::{FileMode, FinalFile, LineEndingPolicy, PatchOperation, WorkspacePath};

    use super::PatchSet;

    fn workspace() -> WorkspaceId {
        WorkspaceId::new([7; 16]).expect("nonzero")
    }

    #[test]
    fn canonical_identity_does_not_depend_on_input_order() {
        let file = |byte| {
            FinalFile::new(vec![byte], FileMode::Regular, LineEndingPolicy::Preserve).expect("file")
        };
        let a = PatchOperation::create(WorkspacePath::new("a").expect("path"), file(1));
        let b = PatchOperation::create(WorkspacePath::new("b").expect("path"), file(2));
        let make = |operations| {
            PatchSet::new(workspace(), Generation::first(), RevisionNumber::first(), operations)
                .expect("patch")
        };
        assert_eq!(make(vec![a.clone(), b.clone()]).identity(), make(vec![b, a]).identity());
    }
}
