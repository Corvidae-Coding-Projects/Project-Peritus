//! Registered-worktree manifest representation and restart recovery.

use std::path::PathBuf;

use peritus_types::Sha256Digest;

use super::{SCHEMA_VERSION, digest, encoded, finish, format_from_tag, format_tag, path_text};
use crate::{
    Baseline, CommitId, CreateWorktree, ErrorKind, GitError, GitRepository, Operation,
    RecoveryClass, RegisteredWorktree, TreeId, WorktreeAccess, WorktreeName,
};

const MAGIC: &str = "peritus-git-worktree-registration";

/// Durable schema-v1 representation of a checked worktree registration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorktreeRegistrationManifest {
    bytes: Vec<u8>,
    digest: Sha256Digest,
    repository_digest: Sha256Digest,
    name: WorktreeName,
    root: PathBuf,
    git_dir: PathBuf,
    baseline: Baseline,
    access: WorktreeAccess,
}

impl WorktreeRegistrationManifest {
    pub(super) fn new(worktree: &RegisteredWorktree) -> Result<Self, GitError> {
        let root = path_text(worktree.root(), Operation::ReopenWorktree)?;
        let git_dir = path_text(worktree.git_dir(), Operation::ReopenWorktree)?;
        let mut writer = super::writer();
        let result = (|| {
            writer.write_str(MAGIC)?;
            writer.write_u16(SCHEMA_VERSION)?;
            writer.write_fixed(worktree.repository_digest().as_bytes())?;
            writer.write_str(worktree.name().as_str())?;
            writer.write_str(root)?;
            writer.write_str(git_dir)?;
            writer.write_u8(format_tag(worktree.baseline().commit().object_id().format()))?;
            super::write_object(&mut writer, worktree.baseline().commit().object_id())?;
            super::write_object(&mut writer, worktree.baseline().tree().object_id())?;
            writer.write_u8(match worktree.access() {
                WorktreeAccess::Writable => 1,
                WorktreeAccess::ReadOnly => 2,
            })
        })();
        let bytes = encoded(result, writer)?;
        Ok(Self {
            digest: digest(&bytes),
            bytes,
            repository_digest: worktree.repository_digest(),
            name: worktree.name().clone(),
            root: worktree.root().to_owned(),
            git_dir: worktree.git_dir().to_owned(),
            baseline: worktree.baseline(),
            access: worktree.access(),
        })
    }

    /// Decodes one complete schema-v1 registration manifest.
    ///
    /// # Errors
    ///
    /// Rejects unknown schemas, malformed fields, oversized input, and trailing bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, GitError> {
        let mut reader = super::reader(bytes)?;
        if reader.read_str().map_err(|_| super::invalid_manifest())? != MAGIC
            || reader.read_u16().map_err(|_| super::invalid_manifest())? != SCHEMA_VERSION
        {
            return Err(super::invalid_manifest());
        }
        let repository_digest =
            Sha256Digest::new(reader.read_fixed::<32>().map_err(|_| super::invalid_manifest())?);
        let name = WorktreeName::new(reader.read_str().map_err(|_| super::invalid_manifest())?)
            .map_err(|_| super::invalid_manifest())?;
        let root = PathBuf::from(reader.read_str().map_err(|_| super::invalid_manifest())?);
        let git_dir = PathBuf::from(reader.read_str().map_err(|_| super::invalid_manifest())?);
        if !root.is_absolute() || !git_dir.is_absolute() {
            return Err(super::invalid_manifest());
        }
        let format = format_from_tag(reader.read_u8().map_err(|_| super::invalid_manifest())?)
            .ok_or_else(super::invalid_manifest)?;
        let commit = CommitId::checked(super::read_object(&mut reader, format)?);
        let tree = TreeId::checked(super::read_object(&mut reader, format)?);
        let access = match reader.read_u8().map_err(|_| super::invalid_manifest())? {
            1 => WorktreeAccess::Writable,
            2 => WorktreeAccess::ReadOnly,
            _ => return Err(super::invalid_manifest()),
        };
        finish(reader)?;
        Ok(Self {
            bytes: bytes.to_vec(),
            digest: digest(bytes),
            repository_digest,
            name,
            root,
            git_dir,
            baseline: Baseline::checked(commit, tree),
            access,
        })
    }

    /// Returns exact canonical schema-v1 bytes suitable for durable storage.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the SHA-256 digest of the complete manifest bytes.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

impl RegisteredWorktree {
    /// Encodes this checked registration for bounded durable restart recovery.
    ///
    /// # Errors
    ///
    /// Returns an unsupported-repository error if canonical paths are not UTF-8.
    pub fn registration_manifest(&self) -> Result<WorktreeRegistrationManifest, GitError> {
        super::worktree_manifest(self)
    }
}

impl GitRepository {
    /// Reopens a persisted registration after revalidating repository, objects, and filesystem.
    ///
    /// # Errors
    ///
    /// Rejects repository identity, baseline, path, Git metadata, HEAD, or detached-state drift.
    pub fn reopen_worktree(
        &self,
        manifest: &WorktreeRegistrationManifest,
    ) -> Result<RegisteredWorktree, GitError> {
        if manifest.repository_digest != self.identity().digest() {
            return Err(reopen_mismatch("worktree manifest belongs to another repository"));
        }
        let expected = self.resolve_baseline(&manifest.baseline.commit().to_string())?;
        if expected != manifest.baseline {
            return Err(reopen_mismatch("worktree manifest baseline no longer resolves exactly"));
        }
        let worktree = RegisteredWorktree::checked(
            manifest.repository_digest,
            manifest.name.clone(),
            manifest.root.clone(),
            manifest.git_dir.clone(),
            manifest.baseline,
            manifest.access,
        );
        let observed = self.inspect_worktree(&worktree)?;
        if observed.head() != worktree.baseline().commit() || !observed.is_detached() {
            return Err(reopen_mismatch("persisted worktree is not at its detached baseline"));
        }
        Ok(worktree)
    }

    /// Recovers an already-created linked worktree when creation completed before its handle was
    /// returned or persisted.
    ///
    /// # Errors
    ///
    /// Rejects missing paths, protected locations, mismatched names, repository ownership, HEAD,
    /// and attached topology.
    pub fn recover_existing_worktree(
        &self,
        request: CreateWorktree,
    ) -> Result<RegisteredWorktree, GitError> {
        crate::worktree::recover_existing(self, request)
    }
}

fn reopen_mismatch(detail: &'static str) -> GitError {
    GitError::new(
        ErrorKind::WorktreeConflict,
        Operation::ReopenWorktree,
        RecoveryClass::Reconcile,
        detail,
    )
}
