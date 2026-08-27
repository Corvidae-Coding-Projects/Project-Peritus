//! Canonical stable inputs for reopening one configured writable workspace.

use std::path::{Component, Path, PathBuf};

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecLimits};
use peritus_git::{
    GitRepository, RegisteredWorktree, WorktreeAccess, WorktreeRegistrationManifest,
};
use peritus_journal::{
    ApplicationWorkspace, MAX_APPLICATION_WORKSPACE_REGISTRATION_BYTES, NewApplicationWorkspace,
};
use peritus_types::{EnvironmentId, ResourceId, Sha256Digest, WorkspaceId};

use crate::{ErrorCode, RecoveryClass, WorkspaceBinding, WorkspaceError, WorkspaceOperation};

const REGISTRATION_DOMAIN: &str = "peritus.workspace.registration";
const REGISTRATION_SCHEMA: u16 = 1;
const MAX_REGISTRATION_PATH_BYTES: usize = 32_768;
const MAX_WORKTREE_MANIFEST_BYTES: usize = 262_144;

/// Maximum complete canonical workspace registration accepted by C1 and C0.
pub const MAX_WORKSPACE_REGISTRATION_BYTES: usize = MAX_APPLICATION_WORKSPACE_REGISTRATION_BYTES;

const REGISTRATION_LIMITS: CodecLimits = CodecLimits::new(
    MAX_WORKSPACE_REGISTRATION_BYTES,
    MAX_WORKSPACE_REGISTRATION_BYTES,
    8,
    MAX_REGISTRATION_PATH_BYTES,
    MAX_WORKTREE_MANIFEST_BYTES,
    4,
);

/// Stable config-bound inputs required to recover one C1 writable workspace.
///
/// Dynamic generation, revision, holder, snapshot, and condition remain in their authoritative
/// projections. This value retains only immutable registration facts and cannot manufacture a
/// clean [`crate::WorkspaceState`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceRegistration {
    bytes: Vec<u8>,
    digest: Sha256Digest,
    workspace_id: WorkspaceId,
    resource_id: ResourceId,
    environment_id: EnvironmentId,
    repository_root: PathBuf,
    worktree: WorktreeRegistrationManifest,
    transaction_root: PathBuf,
}

impl WorkspaceRegistration {
    /// Captures exact checked repository, writable-worktree, and workspace binding facts.
    ///
    /// # Errors
    ///
    /// Rejects cross-repository bindings, non-writable registrations, noncanonical or overlapping
    /// roots, non-UTF-8 paths, and any canonical encoding failure.
    pub fn new(
        binding: &WorkspaceBinding,
        repository: &GitRepository,
        worktree: &RegisteredWorktree,
        transaction_root: PathBuf,
    ) -> Result<Self, WorkspaceError> {
        if repository.identity().digest() != worktree.repository_digest()
            || worktree.access() != WorktreeAccess::Writable
            || binding.root() != worktree.root()
            || binding.baseline_commit() != worktree.baseline().commit()
            || binding.baseline_tree() != worktree.baseline().tree()
        {
            return Err(invalid("workspace registration facts disagree"));
        }
        let manifest = worktree
            .registration_manifest()
            .map_err(|_| invalid("writable worktree registration cannot be encoded canonically"))?;
        Self::from_parts(
            binding.workspace_id(),
            binding.resource_id(),
            binding.environment_id(),
            repository.identity().repository_root().to_owned(),
            manifest,
            transaction_root,
        )
    }

    /// Decodes and validates one complete schema-v1 registration.
    ///
    /// # Errors
    ///
    /// Rejects oversized, malformed, noncanonical, trailing, cross-field inconsistent, or
    /// unsupported registration bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, WorkspaceError> {
        if bytes.is_empty() || bytes.len() > MAX_WORKSPACE_REGISTRATION_BYTES {
            return Err(invalid("workspace registration size is outside the production bound"));
        }
        let mut reader = CanonicalReader::new(bytes, REGISTRATION_LIMITS);
        if reader.read_str().map_err(codec)? != REGISTRATION_DOMAIN
            || reader.read_u16().map_err(codec)? != REGISTRATION_SCHEMA
        {
            return Err(invalid("workspace registration schema is unsupported"));
        }
        let workspace_id = WorkspaceId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| invalid("workspace registration identity is invalid"))?;
        let resource_id = ResourceId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| invalid("workspace registration resource is invalid"))?;
        let environment_id = EnvironmentId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| invalid("workspace registration environment is invalid"))?;
        let repository_root = PathBuf::from(reader.read_str().map_err(codec)?);
        let worktree_bytes = reader.read_bytes().map_err(codec)?;
        let worktree = WorktreeRegistrationManifest::decode(worktree_bytes)
            .map_err(|_| invalid("nested worktree registration is invalid"))?;
        let transaction_root = PathBuf::from(reader.read_str().map_err(codec)?);
        reader.finish().map_err(codec)?;
        let registration = Self::from_parts(
            workspace_id,
            resource_id,
            environment_id,
            repository_root,
            worktree,
            transaction_root,
        )?;
        if registration.bytes != bytes {
            return Err(invalid("workspace registration bytes are not canonical"));
        }
        Ok(registration)
    }

    /// Validates and decodes one C0 workspace-catalog row for C1 recovery.
    ///
    /// # Errors
    ///
    /// Rejects any mismatch between the row identity/digest and its inner canonical registration.
    pub fn from_application_workspace(
        workspace: &ApplicationWorkspace,
    ) -> Result<Self, WorkspaceError> {
        let registration = Self::decode(workspace.registration_bytes())?;
        if registration.workspace_id != workspace.workspace_id()
            || registration.digest != workspace.registration_digest()
        {
            return Err(invalid("durable workspace row differs from its canonical registration"));
        }
        Ok(registration)
    }

    /// Creates the exact C0 catalog insertion value for this canonical registration.
    ///
    /// # Errors
    ///
    /// Returns a journal input error if C0's compiled catalog bound rejects the canonical bytes.
    pub fn durable_registration(
        &self,
    ) -> Result<NewApplicationWorkspace, peritus_journal::JournalError> {
        NewApplicationWorkspace::new(self.workspace_id, self.bytes.clone(), self.digest)
    }

    /// Returns the complete canonical schema-v1 bytes.
    #[must_use]
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the SHA-256 digest of the complete canonical bytes.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    /// Returns the workspace lineage identity.
    #[must_use]
    pub const fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }

    /// Returns the exact authorized resource identity.
    #[must_use]
    pub const fn resource_id(&self) -> ResourceId {
        self.resource_id
    }

    /// Returns the exact environment identity.
    #[must_use]
    pub const fn environment_id(&self) -> EnvironmentId {
        self.environment_id
    }

    /// Returns the canonical repository root from which C1 reopens Git.
    #[must_use]
    pub fn repository_root(&self) -> &Path {
        &self.repository_root
    }

    /// Returns the exact checked writable-worktree registration manifest.
    #[must_use]
    pub const fn worktree_manifest(&self) -> &WorktreeRegistrationManifest {
        &self.worktree
    }

    /// Returns the target-owned transaction root used for restart reconciliation.
    #[must_use]
    pub fn transaction_root(&self) -> &Path {
        &self.transaction_root
    }

    /// Reconstructs the immutable C1 binding without creating dynamic workspace state.
    ///
    /// # Errors
    ///
    /// Returns an input error if retained registration facts cannot form a C1 binding.
    pub fn workspace_binding(&self) -> Result<WorkspaceBinding, WorkspaceError> {
        WorkspaceBinding::new(
            self.workspace_id,
            self.resource_id,
            self.environment_id,
            self.worktree.root().to_owned(),
            self.worktree.baseline().commit(),
            self.worktree.baseline().tree(),
        )
    }

    fn from_parts(
        workspace_id: WorkspaceId,
        resource_id: ResourceId,
        environment_id: EnvironmentId,
        repository_root: PathBuf,
        worktree: WorktreeRegistrationManifest,
        transaction_root: PathBuf,
    ) -> Result<Self, WorkspaceError> {
        validate_path(&repository_root)?;
        validate_path(worktree.root())?;
        validate_path(&transaction_root)?;
        if worktree.access() != WorktreeAccess::Writable {
            return Err(invalid("workspace registration does not name a writable worktree"));
        }
        if overlaps(&transaction_root, worktree.root())
            || overlaps(&transaction_root, &repository_root)
        {
            return Err(invalid("workspace transaction root overlaps a registered Git path"));
        }
        if worktree.bytes().len() > MAX_WORKTREE_MANIFEST_BYTES {
            return Err(invalid("nested worktree registration exceeds its production bound"));
        }
        let repository = path_text(&repository_root)?;
        let transaction = path_text(&transaction_root)?;
        let mut writer = CanonicalWriter::new(REGISTRATION_LIMITS);
        writer.write_str(REGISTRATION_DOMAIN).map_err(codec)?;
        writer.write_u16(REGISTRATION_SCHEMA).map_err(codec)?;
        writer.write_fixed(workspace_id.as_bytes()).map_err(codec)?;
        writer.write_fixed(resource_id.as_bytes()).map_err(codec)?;
        writer.write_fixed(environment_id.as_bytes()).map_err(codec)?;
        writer.write_str(repository).map_err(codec)?;
        writer.write_bytes(worktree.bytes()).map_err(codec)?;
        writer.write_str(transaction).map_err(codec)?;
        let bytes = writer.into_bytes();
        if bytes.len() > MAX_WORKSPACE_REGISTRATION_BYTES {
            return Err(invalid("workspace registration exceeds its production bound"));
        }
        Ok(Self {
            digest: peritus_codec::sha256(&bytes),
            bytes,
            workspace_id,
            resource_id,
            environment_id,
            repository_root,
            worktree,
            transaction_root,
        })
    }
}

fn validate_path(path: &Path) -> Result<(), WorkspaceError> {
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return Err(invalid("workspace registration path is not canonical absolute input"));
    }
    let _ = path_text(path)?;
    Ok(())
}

fn path_text(path: &Path) -> Result<&str, WorkspaceError> {
    let value = path.to_str().ok_or_else(|| invalid("workspace registration path is not UTF-8"))?;
    if value.len() > MAX_REGISTRATION_PATH_BYTES {
        return Err(invalid("workspace registration path exceeds its production bound"));
    }
    Ok(value)
}

#[allow(clippy::missing_const_for_fn, reason = "Path::starts_with is not const")]
fn overlaps(left: &Path, right: &Path) -> bool {
    left.starts_with(right) || right.starts_with(left)
}

const fn codec(_error: peritus_codec::CodecError) -> WorkspaceError {
    invalid("workspace registration canonical encoding is invalid")
}

const fn invalid(detail: &'static str) -> WorkspaceError {
    WorkspaceError::new(
        ErrorCode::InvalidInput,
        WorkspaceOperation::Open,
        RecoveryClass::Quarantine,
        detail,
    )
}
