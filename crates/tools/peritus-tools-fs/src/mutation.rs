//! Inert filesystem mutation compilation.

use std::io::Write as _;

use peritus_artifact_store::{ArtifactDigest, ArtifactStore};
use peritus_patch::{FinalFile, PatchOperation, PatchSet, Preimage, SnapshotFile};
use peritus_types::{Generation, RevisionNumber, WorkspaceId};

use crate::{
    CreateInput, FsToolError, FsToolErrorKind, FsToolOperation, MutationContent, PatchEdit,
    PatchInput, RecoveryClass, RemoveInput, ReplaceInput, WriteInput,
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
        Self::create_with_artifacts(version, input, None)
    }

    /// Compiles an explicit create-or-replace write input.
    ///
    /// # Errors
    /// Returns a typed input or patch construction failure.
    pub fn write(version: WorkspaceVersion, input: WriteInput) -> Result<Self, FsToolError> {
        Self::write_with_artifacts(version, input, None)
    }

    pub(crate) fn create_with_artifacts(
        version: WorkspaceVersion,
        input: CreateInput,
        artifacts: Option<&ArtifactStore>,
    ) -> Result<Self, FsToolError> {
        compile(version, FsToolOperation::Create, vec![create(input, artifacts)?])
    }

    pub(crate) fn write_with_artifacts(
        version: WorkspaceVersion,
        input: WriteInput,
        artifacts: Option<&ArtifactStore>,
    ) -> Result<Self, FsToolError> {
        let fields = input.0;
        let final_content = final_content(
            FsToolOperation::Write,
            fields.final_input.content,
            fields.final_input.mode,
            fields.final_input.line_endings,
            artifacts,
        )?;
        let operation = match fields.preimage {
            Preimage::Absent => final_content.create(fields.final_input.path),
            Preimage::EmptyDirectory { .. } => return Err(patch_error(FsToolOperation::Write)),
            present @ Preimage::Present { .. } => {
                final_content
                    .replace(fields.final_input.path, present)
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
        Self::replace_with_artifacts(version, input, None)
    }

    /// Compiles a bounded atomic multi-file patch.
    ///
    /// # Errors
    /// Returns a typed duplicate, conflict, content, or bounds failure.
    pub fn patch(version: WorkspaceVersion, input: PatchInput) -> Result<Self, FsToolError> {
        Self::patch_with_artifacts(version, input, None)
    }

    pub(crate) fn replace_with_artifacts(
        version: WorkspaceVersion,
        input: ReplaceInput,
        artifacts: Option<&ArtifactStore>,
    ) -> Result<Self, FsToolError> {
        compile(
            version,
            FsToolOperation::Replace,
            vec![replace(input, artifacts)?],
        )
    }

    pub(crate) fn patch_with_artifacts(
        version: WorkspaceVersion,
        input: PatchInput,
        artifacts: Option<&ArtifactStore>,
    ) -> Result<Self, FsToolError> {
        let mut operations = Vec::with_capacity(input.edits.len());
        for edit in input.edits {
            operations.push(match edit {
                PatchEdit::Create(input) => create(input, artifacts)?,
                PatchEdit::Replace(input) => replace(input, artifacts)?,
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

fn create(
    input: CreateInput,
    artifacts: Option<&ArtifactStore>,
) -> Result<PatchOperation, FsToolError> {
    let fields = input.0;
    final_content(
        FsToolOperation::Create,
        fields.content,
        fields.mode,
        fields.line_endings,
        artifacts,
    )
    .map(|content| content.create(fields.path))
}

fn replace(
    input: ReplaceInput,
    artifacts: Option<&ArtifactStore>,
) -> Result<PatchOperation, FsToolError> {
    let fields = input.0;
    final_content(
        FsToolOperation::Replace,
        fields.content,
        fields.mode,
        fields.line_endings,
        artifacts,
    )?
    .replace(fields.existing.path, fields.existing.preimage)
        .map_err(|_| patch_error(FsToolOperation::Replace))
}

fn remove(input: RemoveInput) -> Result<PatchOperation, FsToolError> {
    let fields = input.0;
    PatchOperation::delete(fields.path, fields.preimage)
        .map_err(|_| patch_error(FsToolOperation::Remove))
}

enum FinalContent {
    Inline(FinalFile),
    Streamed(SnapshotFile),
}

impl FinalContent {
    fn create(self, path: peritus_patch::WorkspacePath) -> PatchOperation {
        match self {
            Self::Inline(file) => PatchOperation::create(path, file),
            Self::Streamed(file) => PatchOperation::create_snapshot(path, file),
        }
    }

    fn replace(
        self,
        path: peritus_patch::WorkspacePath,
        preimage: Preimage,
    ) -> Result<PatchOperation, peritus_patch::PatchError> {
        match self {
            Self::Inline(file) => PatchOperation::replace(path, preimage, file),
            Self::Streamed(file) => PatchOperation::replace_snapshot(path, preimage, file),
        }
    }
}

fn final_content(
    operation: FsToolOperation,
    content: MutationContent,
    mode: peritus_patch::FileMode,
    line_endings: peritus_patch::LineEndingPolicy,
    artifacts: Option<&ArtifactStore>,
) -> Result<FinalContent, FsToolError> {
    match content {
        MutationContent::Inline(bytes) => FinalFile::new(bytes, mode, line_endings)
            .map(FinalContent::Inline)
            .map_err(|_| patch_error(operation)),
        MutationContent::Artifact { digest, bytes } => {
            let store = artifacts.ok_or_else(|| artifact_error(operation))?;
            let mut reader = store
                .open_read(ArtifactDigest::from_sha256(digest))
                .map_err(|_| artifact_error(operation))?;
            if reader.metadata().size() != bytes
                || reader.metadata().digest() != ArtifactDigest::from_sha256(digest)
            {
                return Err(artifact_error(operation));
            }
            let mut file = tempfile::tempfile().map_err(|_| artifact_error(operation))?;
            let capacity = usize::try_from(bytes.min(64 * 1_024).max(1))
                .map_err(|_| artifact_error(operation))?;
            let mut written = 0_u64;
            while let Some(chunk) = reader
                .read_chunk(capacity)
                .map_err(|_| artifact_error(operation))?
            {
                if chunk.offset() != written {
                    return Err(artifact_error(operation));
                }
                file.write_all(chunk.bytes()).map_err(|_| artifact_error(operation))?;
                written = written
                    .checked_add(chunk.bytes().len() as u64)
                    .ok_or_else(|| artifact_error(operation))?;
            }
            if written != bytes {
                return Err(artifact_error(operation));
            }
            file.sync_all().map_err(|_| artifact_error(operation))?;
            Ok(FinalContent::Streamed(SnapshotFile::new(file, digest, bytes, mode)))
        }
    }
}

const fn patch_error(operation: FsToolOperation) -> FsToolError {
    FsToolError::new(
        FsToolErrorKind::Patch,
        operation,
        RecoveryClass::CorrectInput,
        "structured input could not be compiled into a canonical patch",
    )
}

const fn artifact_error(operation: FsToolOperation) -> FsToolError {
    FsToolError::new(
        FsToolErrorKind::Inspection,
        operation,
        RecoveryClass::Reobserve,
        "mutation artifact is unavailable or differs from its exact digest and size",
    )
}
