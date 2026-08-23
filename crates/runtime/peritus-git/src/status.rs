//! Bounded typed observations of Git porcelain-v2 status.

mod porcelain;

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use peritus_types::Sha256Digest;

use crate::command::CommandAccess;
use crate::repository::strings;
use crate::{CommitId, GitError, GitRepository, RegisteredWorktree, TreeId};

pub const MAX_STATUS_ENTRIES: usize = 100_000;
pub const MAX_STATUS_PATH_BYTES: usize = 4_096;
pub const MAX_STATUS_BYTES: usize = 64 * 1024 * 1024;

/// One porcelain-v2 index or worktree change code.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ChangeCode {
    /// No change in this column.
    Unmodified,
    /// Added content.
    Added,
    /// Modified content.
    Modified,
    /// Deleted content.
    Deleted,
    /// Renamed content.
    Renamed,
    /// Copied content.
    Copied,
    /// File type changed.
    TypeChanged,
    /// Unmerged content.
    Unmerged,
}

/// Parsed porcelain-v2 submodule state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[allow(clippy::struct_excessive_bools)] // Four independent porcelain-v2 facts, not states.
pub struct SubmoduleState {
    is_submodule: bool,
    commit_changed: bool,
    modified_content: bool,
    untracked_content: bool,
}

impl SubmoduleState {
    /// Returns whether the entry is a submodule.
    #[must_use]
    pub const fn is_submodule(self) -> bool {
        self.is_submodule
    }

    /// Returns whether the submodule commit differs.
    #[must_use]
    pub const fn commit_changed(self) -> bool {
        self.commit_changed
    }

    /// Returns whether tracked submodule content is modified.
    #[must_use]
    pub const fn modified_content(self) -> bool {
        self.modified_content
    }

    /// Returns whether the submodule contains untracked content.
    #[must_use]
    pub const fn untracked_content(self) -> bool {
        self.untracked_content
    }
}

/// Exact head, index, and worktree file modes reported for an entry.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EntryModes {
    head: u32,
    index: u32,
    worktree: u32,
}

impl EntryModes {
    /// Returns the HEAD-side mode.
    #[must_use]
    pub const fn head(self) -> u32 {
        self.head
    }

    /// Returns the index-side mode.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.index
    }

    /// Returns the worktree-side mode.
    #[must_use]
    pub const fn worktree(self) -> u32 {
        self.worktree
    }
}

/// Exact porcelain-v2 record class and record-specific metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StatusKind {
    /// An ordinary tracked entry.
    Ordinary {
        /// Index-side change.
        index: ChangeCode,
        /// Worktree-side change.
        worktree: ChangeCode,
        /// Submodule dirtiness state.
        submodule: SubmoduleState,
        /// Reported file modes.
        modes: EntryModes,
    },
    /// A tracked rename or copy record.
    Renamed {
        /// Index-side change.
        index: ChangeCode,
        /// Worktree-side change.
        worktree: ChangeCode,
        /// Submodule dirtiness state.
        submodule: SubmoduleState,
        /// Reported file modes.
        modes: EntryModes,
        /// Similarity percentage from Git.
        score: u8,
        /// Original path paired with the entry's destination path.
        original_path: String,
    },
    /// An unmerged entry retaining every stage mode.
    Unmerged {
        /// Submodule dirtiness state.
        submodule: SubmoduleState,
        /// Common-ancestor stage mode.
        ancestor_mode: u32,
        /// Ours stage mode.
        ours_mode: u32,
        /// Theirs stage mode.
        theirs_mode: u32,
        /// Worktree mode.
        worktree_mode: u32,
    },
    /// An untracked path.
    Untracked,
    /// An ignored path.
    Ignored,
}

/// One validated repository-relative status entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StatusEntry {
    path: String,
    kind: StatusKind,
}

impl StatusEntry {
    /// Returns the exact unquoted repository-relative UTF-8 path.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns the typed porcelain record.
    #[must_use]
    pub const fn kind(&self) -> &StatusKind {
        &self.kind
    }
}

/// Repository-bound status observation with a canonical digest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StatusObservation {
    repository_digest: Sha256Digest,
    worktree_root: PathBuf,
    head: CommitId,
    detached: bool,
    index_tree: Option<TreeId>,
    digest: Sha256Digest,
    entries: Vec<StatusEntry>,
}

impl StatusObservation {
    /// Returns the repository identity observed with this status.
    #[must_use]
    pub const fn repository_digest(&self) -> Sha256Digest {
        self.repository_digest
    }

    /// Returns the exact canonical worktree root.
    #[must_use]
    pub fn worktree_root(&self) -> &Path {
        &self.worktree_root
    }

    /// Returns the exact current HEAD commit.
    #[must_use]
    pub const fn head(&self) -> CommitId {
        self.head
    }

    /// Returns whether the observed HEAD has detached topology.
    #[must_use]
    pub const fn is_detached(&self) -> bool {
        self.detached
    }

    /// Returns the current index tree, or `None` when conflicts prevent writing one.
    #[must_use]
    pub const fn index_tree(&self) -> Option<TreeId> {
        self.index_tree
    }

    /// Returns the canonical digest binding identity, HEAD, index, and entries.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    /// Returns typed entries in Git's deterministic porcelain order.
    #[must_use]
    pub fn entries(&self) -> &[StatusEntry] {
        &self.entries
    }

    /// Returns whether there are no tracked, untracked, ignored, or conflicted entries.
    #[must_use]
    pub const fn is_clean(&self) -> bool {
        self.entries.is_empty()
    }

    pub(crate) fn has_unmerged(&self) -> bool {
        self.entries.iter().any(|entry| matches!(&entry.kind, StatusKind::Unmerged { .. }))
    }

    pub(crate) fn has_untracked(&self) -> bool {
        self.entries.iter().any(|entry| matches!(&entry.kind, StatusKind::Untracked))
    }

    pub(crate) fn has_ignored(&self) -> bool {
        self.entries.iter().any(|entry| matches!(&entry.kind, StatusKind::Ignored))
    }

    pub(crate) fn has_worktree_change(&self) -> bool {
        self.entries.iter().any(|entry| match &entry.kind {
            StatusKind::Ordinary { worktree, .. } | StatusKind::Renamed { worktree, .. } => {
                *worktree != ChangeCode::Unmodified
            }
            StatusKind::Unmerged { .. } => true,
            StatusKind::Untracked | StatusKind::Ignored => false,
        })
    }
}

impl GitRepository {
    /// Observes a registered worktree with bounded NUL-delimited porcelain-v2 output.
    ///
    /// # Errors
    ///
    /// Returns a typed registration, command, object, or porcelain protocol failure.
    pub fn status(&self, worktree: &RegisteredWorktree) -> Result<StatusObservation, GitError> {
        self.validate_registration(worktree, crate::Operation::Status)?;
        self.reject_external_filters(
            crate::Operation::Status,
            Self::worktree_location(worktree.root(), worktree.git_dir()),
        )?;
        let arguments = strings(&[
            "status",
            "--porcelain=v2",
            "-z",
            "--branch",
            "--untracked-files=all",
            "--ignored=matching",
            "--ignore-submodules=none",
        ]);
        let output = self.runner.checked(
            worktree.root(),
            Some(Self::worktree_location(worktree.root(), worktree.git_dir())),
            CommandAccess::Read,
            crate::Operation::Status,
            &arguments,
            None,
        )?;
        let parsed = porcelain::parse(&output.stdout, self.identity.object_format())?;
        let index_output = self.runner.observe(
            worktree.root(),
            Some(Self::worktree_location(worktree.root(), worktree.git_dir())),
            CommandAccess::Read,
            crate::Operation::Status,
            &[OsString::from("write-tree")],
            None,
        )?;
        let index_tree = if index_output.status.success() {
            Some(TreeId::checked(crate::ObjectId::parse(
                self.identity.object_format(),
                crate::command::one_line(&index_output.stdout, crate::Operation::Status)?,
                crate::Operation::Status,
            )?))
        } else {
            None
        };
        let digest = status_digest(
            self.identity.digest(),
            worktree.root(),
            parsed.head,
            parsed.detached,
            index_tree,
            &output.stdout,
        )?;
        Ok(StatusObservation {
            repository_digest: self.identity.digest(),
            worktree_root: worktree.root().to_owned(),
            head: parsed.head,
            detached: parsed.detached,
            index_tree,
            digest,
            entries: parsed.entries,
        })
    }
}

fn status_digest(
    repository: Sha256Digest,
    root: &Path,
    head: CommitId,
    detached: bool,
    index: Option<TreeId>,
    porcelain: &[u8],
) -> Result<Sha256Digest, GitError> {
    let root = root.to_str().ok_or_else(|| {
        GitError::new(
            crate::ErrorKind::UnsupportedRepository,
            crate::Operation::Status,
            crate::RecoveryClass::CorrectRequest,
            "canonical worktree path is not UTF-8",
        )
    })?;
    let mut bytes = b"PERITUS-GIT-STATUS-V1\0".to_vec();
    bytes.extend_from_slice(repository.as_bytes());
    put_bytes(&mut bytes, root.as_bytes());
    put_bytes(&mut bytes, head.object_id().as_bytes());
    bytes.push(u8::from(detached));
    match index {
        Some(tree) => {
            bytes.push(1);
            put_bytes(&mut bytes, tree.object_id().as_bytes());
        }
        None => bytes.push(0),
    }
    put_bytes(&mut bytes, porcelain);
    Ok(peritus_codec::sha256(&bytes))
}

pub fn put_bytes(output: &mut Vec<u8>, value: &[u8]) {
    output.extend_from_slice(&(value.len() as u64).to_be_bytes());
    output.extend_from_slice(value);
}
