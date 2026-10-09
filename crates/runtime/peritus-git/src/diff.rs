//! Bounded structured diff between two immutable commits.

use std::ffi::OsString;

use peritus_types::Sha256Digest;

use crate::{
    CommitId, ErrorKind, GitError, GitRepository, Operation, RecoveryClass, RegisteredWorktree,
    command::CommandAccess,
};

/// Maximum structured paths in one diff observation.
pub const MAX_DIFF_ENTRIES: u32 = 100_000;
/// Maximum retained textual patch bytes.
pub const MAX_DIFF_BYTES: u64 = 8 * 1_024 * 1_024;

/// Closed name-status change vocabulary.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DiffChange {
    /// Added path.
    Added,
    /// Modified path.
    Modified,
    /// Deleted path.
    Deleted,
    /// File type changed.
    TypeChanged,
    /// Unmerged path.
    Unmerged,
}

/// One UTF-8 repository-relative changed path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffEntry {
    path: String,
    change: DiffChange,
}

impl DiffEntry {
    /// Returns the exact unquoted repository-relative path.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns the reported change class.
    #[must_use]
    pub const fn change(&self) -> DiffChange {
        self.change
    }
}

/// Exact immutable commit pair and caller-selected output bounds.
#[derive(Clone, Copy, Debug)]
pub struct DiffRequest<'a> {
    worktree: &'a RegisteredWorktree,
    base: CommitId,
    target: CommitId,
    maximum_entries: u32,
    maximum_patch_bytes: u64,
}

impl<'a> DiffRequest<'a> {
    /// Creates one structured immutable diff request.
    ///
    /// # Errors
    /// Rejects zero or excessive entry and patch bounds.
    pub fn new(
        worktree: &'a RegisteredWorktree,
        base: CommitId,
        target: CommitId,
        maximum_entries: u32,
        maximum_patch_bytes: u64,
    ) -> Result<Self, GitError> {
        if maximum_entries == 0
            || maximum_entries > MAX_DIFF_ENTRIES
            || maximum_patch_bytes == 0
            || maximum_patch_bytes > MAX_DIFF_BYTES
        {
            return Err(input_error("Git diff bounds are zero or exceed their hard maximum"));
        }
        Ok(Self { worktree, base, target, maximum_entries, maximum_patch_bytes })
    }
}

/// Complete bounded diff observation with exact source identities.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitDiffObservation {
    repository_digest: Sha256Digest,
    base: CommitId,
    target: CommitId,
    entries: Vec<DiffEntry>,
    patch: Vec<u8>,
    digest: Sha256Digest,
}

impl GitDiffObservation {
    /// Returns the repository binding.
    #[must_use]
    pub const fn repository_digest(&self) -> Sha256Digest {
        self.repository_digest
    }
    /// Returns the exact base commit.
    #[must_use]
    pub const fn base(&self) -> CommitId {
        self.base
    }
    /// Returns the exact target commit.
    #[must_use]
    pub const fn target(&self) -> CommitId {
        self.target
    }
    /// Returns path-sorted structured changes.
    #[must_use]
    pub fn entries(&self) -> &[DiffEntry] {
        &self.entries
    }
    /// Returns the exact bounded Git patch bytes.
    #[must_use]
    pub fn patch(&self) -> &[u8] {
        &self.patch
    }
    /// Returns the canonical observation digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

impl GitRepository {
    /// Observes one immutable commit diff through fixed read-only Git commands.
    ///
    /// # Errors
    /// Returns typed registration, command, UTF-8 path, output-bound, or protocol failure.
    pub fn diff(&self, request: DiffRequest<'_>) -> Result<GitDiffObservation, GitError> {
        self.validate_registration(request.worktree, Operation::Diff)?;
        self.reject_external_filters(
            Operation::Diff,
            Self::worktree_location(request.worktree.root(), request.worktree.git_dir()),
        )?;
        let pair = format!("{}..{}", request.base, request.target);
        let mut names = crate::repository::strings(&[
            "diff",
            "--name-status",
            "-z",
            "--no-renames",
            "--no-ext-diff",
            "--no-textconv",
        ]);
        names.push(OsString::from(&pair));
        names.push(OsString::from("--"));
        let location =
            Some(Self::worktree_location(request.worktree.root(), request.worktree.git_dir()));
        let names = self.runner.checked(
            request.worktree.root(),
            location,
            CommandAccess::Read,
            Operation::Diff,
            &names,
            None,
        )?;
        let entries = parse_names(&names.stdout, request.maximum_entries)?;
        let mut arguments = crate::repository::strings(&[
            "diff",
            "--patch",
            "--no-renames",
            "--no-ext-diff",
            "--no-textconv",
            "--src-prefix=a/",
            "--dst-prefix=b/",
        ]);
        arguments.push(OsString::from(pair));
        arguments.push(OsString::from("--"));
        let patch = self
            .runner
            .checked(
                request.worktree.root(),
                location,
                CommandAccess::Read,
                Operation::Diff,
                &arguments,
                None,
            )?
            .stdout;
        if patch.len() as u64 > request.maximum_patch_bytes {
            return Err(input_error("Git diff patch exceeds the requested byte bound"));
        }
        let digest =
            diff_digest(self.identity.digest(), request.base, request.target, &entries, &patch);
        Ok(GitDiffObservation {
            repository_digest: self.identity.digest(),
            base: request.base,
            target: request.target,
            entries,
            patch,
            digest,
        })
    }
}

fn parse_names(bytes: &[u8], maximum: u32) -> Result<Vec<DiffEntry>, GitError> {
    let fields =
        bytes.split(|byte| *byte == 0).filter(|field| !field.is_empty()).collect::<Vec<_>>();
    if fields.len() % 2 != 0 || fields.len() / 2 > maximum as usize {
        return Err(protocol("Git diff name-status output is malformed or exceeds its bound"));
    }
    let mut entries = Vec::with_capacity(fields.len() / 2);
    for pair in fields.chunks_exact(2) {
        let change = match pair[0] {
            b"A" => DiffChange::Added,
            b"M" => DiffChange::Modified,
            b"D" => DiffChange::Deleted,
            b"T" => DiffChange::TypeChanged,
            b"U" => DiffChange::Unmerged,
            _ => return Err(protocol("Git diff reported an unsupported change code")),
        };
        let path = std::str::from_utf8(pair[1])
            .map_err(|_| protocol("Git diff path is not UTF-8"))?
            .to_owned();
        if path.is_empty() || path.len() > crate::status::MAX_STATUS_PATH_BYTES {
            return Err(protocol("Git diff path is empty or exceeds its bound"));
        }
        entries.push(DiffEntry { path, change });
    }
    Ok(entries)
}

fn diff_digest(
    repository: Sha256Digest,
    base: CommitId,
    target: CommitId,
    entries: &[DiffEntry],
    patch: &[u8],
) -> Sha256Digest {
    let mut bytes = b"PERITUS-GIT-DIFF-V1\0".to_vec();
    bytes.extend_from_slice(repository.as_bytes());
    bytes.extend_from_slice(base.object_id().as_bytes());
    bytes.extend_from_slice(target.object_id().as_bytes());
    bytes.extend_from_slice(&(entries.len() as u64).to_be_bytes());
    for entry in entries {
        bytes.push(match entry.change {
            DiffChange::Added => 1,
            DiffChange::Modified => 2,
            DiffChange::Deleted => 3,
            DiffChange::TypeChanged => 4,
            DiffChange::Unmerged => 5,
        });
        crate::status::put_bytes(&mut bytes, entry.path.as_bytes());
    }
    crate::status::put_bytes(&mut bytes, patch);
    peritus_codec::sha256(&bytes)
}

fn input_error(detail: &'static str) -> GitError {
    GitError::new(ErrorKind::InvalidInput, Operation::Diff, RecoveryClass::CorrectRequest, detail)
}

fn protocol(detail: &'static str) -> GitError {
    GitError::new(ErrorKind::GitProtocol, Operation::Diff, RecoveryClass::Reobserve, detail)
}
