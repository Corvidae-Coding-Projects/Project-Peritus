//! Bounded structured diff between two immutable commits.

use std::ffi::OsString;

use peritus_types::Sha256Digest;

use crate::{
    CommitId, ErrorKind, GitError, GitRepository, Operation, RecoveryClass, RegisteredWorktree,
    command::CommandAccess,
};

/// Default structured-path transfer window; not a whole-diff admission limit.
pub const MAX_DIFF_ENTRIES: u32 = 100_000;
/// Default patch transfer window; not a whole-diff admission limit.
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

/// One native repository-relative changed path with a display projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffEntry {
    path: String,
    path_bytes: Vec<u8>,
    change: DiffChange,
}

impl DiffEntry {
    /// Returns a display path; use `path_bytes` for exact native identity.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns exact native repository-relative path bytes.
    #[must_use]
    pub fn path_bytes(&self) -> &[u8] {
        &self.path_bytes
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
    cursor: DiffCursor,
    expected_digest: Option<Sha256Digest>,
}

impl<'a> DiffRequest<'a> {
    /// Creates one structured immutable diff request.
    ///
    /// # Errors
    /// Rejects zero entry or patch page sizes.
    pub fn new(
        worktree: &'a RegisteredWorktree,
        base: CommitId,
        target: CommitId,
        maximum_entries: u32,
        maximum_patch_bytes: u64,
    ) -> Result<Self, GitError> {
        if maximum_entries == 0 || maximum_patch_bytes == 0 {
            return Err(input_error("Git diff page dimensions must be nonzero"));
        }
        Ok(Self {
            worktree,
            base,
            target,
            maximum_entries,
            maximum_patch_bytes,
            cursor: DiffCursor::default(),
            expected_digest: None,
        })
    }

    /// Binds a continuation to the previous complete immutable observation digest.
    #[must_use]
    pub const fn with_cursor(mut self, cursor: DiffCursor, digest: Option<Sha256Digest>) -> Self {
        self.cursor = cursor;
        self.expected_digest = digest;
        self
    }
}

/// Independent offsets into an immutable diff. Path bytes continue the first selected entry.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DiffCursor {
    /// Absolute changed-path index.
    pub entry_offset: u64,
    /// Absolute patch-byte offset.
    pub patch_offset: u64,
    /// Byte offset within the first selected native path.
    pub path_byte_offset: u64,
}

/// Bounded diff page with complete-source identity and explicit continuation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitDiffObservation {
    repository_digest: Sha256Digest,
    base: CommitId,
    target: CommitId,
    entries: Vec<DiffEntry>,
    patch: Vec<u8>,
    digest: Sha256Digest,
    cursor: DiffCursor,
    total_entries: u64,
    total_patch_bytes: u64,
}

impl GitDiffObservation {
    /// Requested immutable-source coordinates.
    #[must_use]
    pub const fn cursor(&self) -> DiffCursor {
        self.cursor
    }
    /// Number of paths in the complete diff.
    #[must_use]
    pub const fn total_entries(&self) -> u64 {
        self.total_entries
    }
    /// Number of bytes in the complete patch.
    #[must_use]
    pub const fn total_patch_bytes(&self) -> u64 {
        self.total_patch_bytes
    }
    /// Next path index after all entries in this runtime page.
    #[must_use]
    pub fn next_entry_offset(&self) -> Option<u64> {
        let end = self.cursor.entry_offset + self.entries.len() as u64;
        (end < self.total_entries).then_some(end)
    }
    /// Next byte offset after this runtime patch page.
    #[must_use]
    pub fn next_patch_offset(&self) -> Option<u64> {
        let end = self.cursor.patch_offset + self.patch.len() as u64;
        (end < self.total_patch_bytes).then_some(end)
    }

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
        let entries = parse_names(&names.stdout)?;
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
        let digest =
            diff_digest(self.identity.digest(), request.base, request.target, &entries, &patch);
        if request.expected_digest.is_some_and(|expected| expected != digest) {
            return Err(input_error("Git diff continuation identity changed"));
        }
        let total_entries = entries.len() as u64;
        let total_patch_bytes = patch.len() as u64;
        let entry_offset = usize::try_from(request.cursor.entry_offset)
            .map_err(|_| input_error("Git diff path offset is not representable"))?;
        let patch_offset = usize::try_from(request.cursor.patch_offset)
            .map_err(|_| input_error("Git diff patch offset is not representable"))?;
        if entry_offset > entries.len()
            || patch_offset > patch.len()
            || request.cursor.path_byte_offset
                > entries.get(entry_offset).map_or(0, |entry| entry.path_bytes.len() as u64)
        {
            return Err(input_error("Git diff continuation is outside its immutable source"));
        }
        let entries =
            entries.into_iter().skip(entry_offset).take(request.maximum_entries as usize).collect();
        let patch_end = patch_offset
            .saturating_add(usize::try_from(request.maximum_patch_bytes).unwrap_or(usize::MAX))
            .min(patch.len());
        let patch = patch[patch_offset..patch_end].to_vec();
        Ok(GitDiffObservation {
            repository_digest: self.identity.digest(),
            base: request.base,
            target: request.target,
            entries,
            patch,
            digest,
            cursor: request.cursor,
            total_entries,
            total_patch_bytes,
        })
    }
}

fn parse_names(bytes: &[u8]) -> Result<Vec<DiffEntry>, GitError> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    let bytes = bytes
        .strip_suffix(&[0])
        .ok_or_else(|| protocol("Git diff name-status output is not NUL terminated"))?;
    let fields = bytes.split(|byte| *byte == 0).collect::<Vec<_>>();
    if fields.len() % 2 != 0 {
        return Err(protocol("Git diff name-status output is malformed"));
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
        if pair[1].is_empty() {
            return Err(protocol("Git diff path is empty"));
        }
        let path_bytes = pair[1].to_vec();
        let path = String::from_utf8_lossy(&path_bytes).into_owned();
        entries.push(DiffEntry { path, path_bytes, change });
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
        crate::status::put_bytes(&mut bytes, &entry.path_bytes);
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
