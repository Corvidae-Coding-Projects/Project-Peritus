//! Inert filesystem mutation compilation.

use peritus_patch::{FinalFile, PatchOperation, PatchSet, Preimage};
use peritus_types::{Generation, RevisionNumber, WorkspaceId};

use crate::{
    CreateInput, FsToolError, FsToolErrorKind, FsToolOperation, PatchEdit, PatchInput,
    RecoveryClass, RemoveInput, ReplaceInput, WriteInput,
};

/// Exact C1 workspace version bound into an inert patch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkspaceVersion {
    workspace_id: WorkspaceId,
    generation: Generation,
    revision: RevisionNumber,
}

impl WorkspaceVersion {
    /// Creates an exact workspace version binding.
    #[must_use]
    pub const fn new(
        workspace_id: WorkspaceId,
        generation: Generation,
        revision: RevisionNumber,
    ) -> Self {
        Self { workspace_id, generation, revision }
    }
}

/// One checked inert mutation ready for the target-owned C1 gateway.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompiledMutation {
    operation: FsToolOperation,
    patch: PatchSet,
}

impl CompiledMutation {
    /// Compiles an exact create input.
    ///
    /// # Errors
    /// Returns a typed input or patch construction failure.
    pub fn create(version: WorkspaceVersion, input: CreateInput) -> Result<Self, FsToolError> {
        compile(version, FsToolOperation::Create, vec![create(input)?])
    }

    /// Compiles an explicit create-or-replace write input.
    ///
    /// # Errors
    /// Returns a typed input or patch construction failure.
    pub fn write(version: WorkspaceVersion, input: WriteInput) -> Result<Self, FsToolError> {
        let fields = input.0;
        let final_file = final_file(FsToolOperation::Write, &fields.final_input)?;
        let operation = match fields.preimage {
            Preimage::Absent => PatchOperation::create(fields.final_input.path, final_file),
            present @ Preimage::Present { .. } => {
                PatchOperation::replace(fields.final_input.path, present, final_file)
                    .map_err(|_| patch_error(FsToolOperation::Write))?
            }
        };
        compile(version, FsToolOperation::Write, vec![operation])
    }

    /// Compiles an exact deletion input.
    ///
    /// # Errors
    /// Returns a typed input or patch construction failure.
    pub fn remove(version: WorkspaceVersion, input: RemoveInput) -> Result<Self, FsToolError> {
        compile(version, FsToolOperation::Remove, vec![remove(input)?])
    }

    /// Compiles an exact replacement input.
    ///
    /// # Errors
    /// Returns a typed input or patch construction failure.
    pub fn replace(version: WorkspaceVersion, input: ReplaceInput) -> Result<Self, FsToolError> {
        compile(version, FsToolOperation::Replace, vec![replace(input)?])
    }

    /// Compiles a bounded atomic multi-file patch.
    ///
    /// # Errors
    /// Returns a typed duplicate, conflict, content, or bounds failure.
    pub fn patch(version: WorkspaceVersion, input: PatchInput) -> Result<Self, FsToolError> {
        let mut operations = Vec::with_capacity(input.edits.len());
        for edit in input.edits {
            operations.push(match edit {
                PatchEdit::Create(input) => create(input)?,
                PatchEdit::Replace(input) => replace(input)?,
                PatchEdit::Remove(input) => remove(input)?,
            });
        }
        compile(version, FsToolOperation::Patch, operations)
    }

    /// Returns the exact originating tool operation.
    #[must_use]
    pub const fn operation(&self) -> FsToolOperation {
        self.operation
    }

    /// Returns the canonical inert C1 patch.
    #[must_use]
    pub const fn patch_set(&self) -> &PatchSet {
        &self.patch
    }

    /// Consumes the compiler product into its canonical inert C1 patch.
    #[must_use]
    pub fn into_patch(self) -> PatchSet {
        self.patch
    }
}

fn compile(
    version: WorkspaceVersion,
    operation: FsToolOperation,
    operations: Vec<PatchOperation>,
) -> Result<CompiledMutation, FsToolError> {
    let patch =
        PatchSet::new(version.workspace_id, version.generation, version.revision, operations)
            .map_err(|_| patch_error(operation))?;
    Ok(CompiledMutation { operation, patch })
}

fn create(input: CreateInput) -> Result<PatchOperation, FsToolError> {
    let fields = input.0;
    let final_file = final_file(FsToolOperation::Create, &fields)?;
    Ok(PatchOperation::create(fields.path, final_file))
}

fn replace(input: ReplaceInput) -> Result<PatchOperation, FsToolError> {
    let fields = input.0;
    let final_file = FinalFile::new(fields.bytes, fields.mode, fields.line_endings)
        .map_err(|_| patch_error(FsToolOperation::Replace))?;
    PatchOperation::replace(fields.existing.path, fields.existing.preimage, final_file)
        .map_err(|_| patch_error(FsToolOperation::Replace))
}

fn remove(input: RemoveInput) -> Result<PatchOperation, FsToolError> {
    let fields = input.0;
    PatchOperation::delete(fields.path, fields.preimage)
        .map_err(|_| patch_error(FsToolOperation::Remove))
}

fn final_file(
    operation: FsToolOperation,
    input: &crate::input::FinalInput,
) -> Result<FinalFile, FsToolError> {
    FinalFile::new(input.bytes.clone(), input.mode, input.line_endings)
        .map_err(|_| patch_error(operation))
}

const fn patch_error(operation: FsToolOperation) -> FsToolError {
    FsToolError::new(
        FsToolErrorKind::Patch,
        operation,
        RecoveryClass::CorrectInput,
        "structured input could not be compiled into a canonical patch",
    )
}
