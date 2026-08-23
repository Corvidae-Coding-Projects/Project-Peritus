//! Repository discovery and stable identity binding.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use peritus_types::Sha256Digest;

use crate::command::{
    CommandAccess, DEFAULT_OUTPUT_LIMIT, GitRunner, MAX_OUTPUT_LIMIT, RepositoryLocation, one_line,
};
use crate::{ErrorKind, GitError, ObjectFormat, Operation, RecoveryClass};

/// Configuration for opening one existing repository.
#[derive(Clone, Debug)]
pub struct RepositoryOptions {
    start: PathBuf,
    git_program: OsString,
    max_output_bytes: usize,
    require_exact_root: bool,
}

impl RepositoryOptions {
    /// Selects the directory from which Git discovery starts.
    #[must_use]
    pub fn new(start: impl Into<PathBuf>) -> Self {
        Self {
            start: start.into(),
            git_program: OsString::from("git"),
            max_output_bytes: DEFAULT_OUTPUT_LIMIT,
            require_exact_root: true,
        }
    }

    /// Selects the exact Git executable without invoking a shell.
    #[must_use]
    pub fn git_program(mut self, program: impl Into<OsString>) -> Self {
        self.git_program = program.into();
        self
    }

    /// Sets the maximum stdout or stderr bytes accepted from one Git command.
    #[must_use]
    pub const fn max_output_bytes(mut self, value: usize) -> Self {
        self.max_output_bytes = value;
        self
    }

    /// Allows `start` to be a descendant of the discovered repository root.
    #[must_use]
    pub const fn allow_discovery_from_descendant(mut self, allow: bool) -> Self {
        self.require_exact_root = !allow;
        self
    }
}

/// Canonical repository identity observed from Git and the filesystem.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepositoryIdentity {
    repository_root: PathBuf,
    git_dir: PathBuf,
    common_dir: PathBuf,
    object_format: ObjectFormat,
    bare: bool,
    digest: Sha256Digest,
}

impl RepositoryIdentity {
    /// Returns the canonical repository or primary-worktree root.
    #[must_use]
    pub fn repository_root(&self) -> &Path {
        &self.repository_root
    }

    /// Returns the canonical Git directory selected during discovery.
    #[must_use]
    pub fn git_dir(&self) -> &Path {
        &self.git_dir
    }

    /// Returns the canonical common Git directory shared by linked worktrees.
    #[must_use]
    pub fn common_dir(&self) -> &Path {
        &self.common_dir
    }

    /// Returns the repository's exact object format.
    #[must_use]
    pub const fn object_format(&self) -> ObjectFormat {
        self.object_format
    }

    /// Returns whether the opened repository has no primary worktree.
    #[must_use]
    pub const fn is_bare(&self) -> bool {
        self.bare
    }

    /// Returns the canonical SHA-256 binding over all repository identity fields.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

/// Open structured Git repository adapter.
#[derive(Clone, Debug)]
pub struct GitRepository {
    pub(crate) identity: RepositoryIdentity,
    pub(crate) runner: GitRunner,
}

impl GitRepository {
    /// Opens and validates an existing repository.
    ///
    /// # Errors
    ///
    /// Returns a typed discovery, path, object-format, or Git protocol failure.
    pub fn open(options: RepositoryOptions) -> Result<Self, GitError> {
        if options.max_output_bytes == 0 || options.max_output_bytes > MAX_OUTPUT_LIMIT {
            return Err(GitError::new(
                ErrorKind::InvalidInput,
                Operation::Discover,
                RecoveryClass::CorrectRequest,
                "Git output limit must be between one byte and the hard 64 MiB bound",
            ));
        }
        let start = std::fs::canonicalize(&options.start).map_err(|source| {
            GitError::io(
                Operation::Discover,
                RecoveryClass::CorrectRequest,
                "canonicalize repository discovery root",
                source,
            )
        })?;
        if !start.is_dir() {
            return Err(GitError::new(
                ErrorKind::InvalidRepository,
                Operation::Discover,
                RecoveryClass::CorrectRequest,
                "repository discovery root is not a directory",
            ));
        }
        let runner = GitRunner::new(options.git_program, options.max_output_bytes);
        let bare = scalar(&runner, &start, &["rev-parse", "--is-bare-repository"])? == "true";
        let git_dir = canonical_git_path(
            &runner,
            &start,
            &["rev-parse", "--path-format=absolute", "--absolute-git-dir"],
        )?;
        let common_dir = canonical_git_path(
            &runner,
            &start,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )?;
        let repository_root = if bare {
            git_dir.clone()
        } else {
            canonical_git_path(
                &runner,
                &start,
                &["rev-parse", "--path-format=absolute", "--show-toplevel"],
            )?
        };
        if options.require_exact_root && repository_root != start {
            return Err(GitError::new(
                ErrorKind::InvalidRepository,
                Operation::Discover,
                RecoveryClass::CorrectRequest,
                "discovery root is not the exact repository root",
            ));
        }
        let format_text = scalar(&runner, &start, &["rev-parse", "--show-object-format=storage"])?;
        let object_format = ObjectFormat::parse(&format_text, Operation::Discover)?;
        let digest = identity_digest(&repository_root, &git_dir, &common_dir, object_format, bare)?;
        Ok(Self {
            identity: RepositoryIdentity {
                repository_root,
                git_dir,
                common_dir,
                object_format,
                bare,
                digest,
            },
            runner,
        })
    }

    /// Returns the exact canonical repository identity.
    #[must_use]
    pub const fn identity(&self) -> &RepositoryIdentity {
        &self.identity
    }

    pub(crate) fn control_cwd(&self) -> &Path {
        &self.identity.repository_root
    }

    pub(crate) fn common_location(&self) -> RepositoryLocation<'_> {
        RepositoryLocation { git_dir: &self.identity.common_dir, work_tree: None }
    }

    pub(crate) const fn worktree_location<'a>(
        root: &'a Path,
        git_dir: &'a Path,
    ) -> RepositoryLocation<'a> {
        RepositoryLocation { git_dir, work_tree: Some(root) }
    }

    pub(crate) fn checked_repo_command(
        &self,
        operation: Operation,
        access: CommandAccess,
        arguments: &[OsString],
        stdin: Option<&[u8]>,
    ) -> Result<crate::command::CommandOutput, GitError> {
        self.runner.checked(
            self.control_cwd(),
            Some(self.common_location()),
            access,
            operation,
            arguments,
            stdin,
        )
    }

    pub(crate) fn reject_external_filters(
        &self,
        operation: Operation,
        location: RepositoryLocation<'_>,
    ) -> Result<(), GitError> {
        let cwd = location.work_tree.unwrap_or_else(|| self.control_cwd());
        let output = self.runner.checked(
            cwd,
            Some(location),
            CommandAccess::Read,
            operation,
            &strings(&["config", "--null", "--name-only", "--list"]),
            None,
        )?;
        for name in output.stdout.split(|byte| *byte == 0).filter(|name| !name.is_empty()) {
            let normalized: Vec<_> = name.iter().map(u8::to_ascii_lowercase).collect();
            let is_filter = normalized.starts_with(b"filter.");
            let is_driver = normalized.ends_with(b".clean")
                || normalized.ends_with(b".smudge")
                || normalized.ends_with(b".process");
            if is_filter && is_driver {
                return Err(GitError::new(
                    ErrorKind::UnsupportedRepository,
                    operation,
                    RecoveryClass::CorrectRequest,
                    "external Git clean, smudge, and process filters are unsupported",
                ));
            }
        }
        Ok(())
    }
}

fn scalar(runner: &GitRunner, cwd: &Path, arguments: &[&str]) -> Result<String, GitError> {
    let arguments: Vec<_> = arguments.iter().map(OsString::from).collect();
    let output =
        runner.checked(cwd, None, CommandAccess::Read, Operation::Discover, &arguments, None)?;
    Ok(one_line(&output.stdout, Operation::Discover)?.to_owned())
}

fn canonical_git_path(
    runner: &GitRunner,
    cwd: &Path,
    arguments: &[&str],
) -> Result<PathBuf, GitError> {
    let value = scalar(runner, cwd, arguments)?;
    let path = PathBuf::from(value);
    std::fs::canonicalize(path).map_err(|source| {
        GitError::io(
            Operation::Discover,
            RecoveryClass::Quarantine,
            "canonicalize Git-reported repository path",
            source,
        )
    })
}

fn identity_digest(
    repository_root: &Path,
    git_dir: &Path,
    common_dir: &Path,
    format: ObjectFormat,
    bare: bool,
) -> Result<Sha256Digest, GitError> {
    let mut bytes = b"PERITUS-GIT-REPOSITORY-IDENTITY-V1\0".to_vec();
    put_path(&mut bytes, repository_root)?;
    put_path(&mut bytes, git_dir)?;
    put_path(&mut bytes, common_dir)?;
    bytes.push(match format {
        ObjectFormat::Sha1 => 1,
        ObjectFormat::Sha256 => 2,
    });
    bytes.push(u8::from(bare));
    Ok(peritus_codec::sha256(&bytes))
}

fn put_path(bytes: &mut Vec<u8>, path: &Path) -> Result<(), GitError> {
    let value = path.to_str().ok_or_else(|| {
        GitError::new(
            ErrorKind::UnsupportedRepository,
            Operation::Discover,
            RecoveryClass::CorrectRequest,
            "canonical repository paths must be UTF-8",
        )
    })?;
    let length = u64::try_from(value.len()).map_err(|_| {
        GitError::new(
            ErrorKind::UnsupportedRepository,
            Operation::Discover,
            RecoveryClass::CorrectRequest,
            "canonical repository path length exceeds u64",
        )
    })?;
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes.extend_from_slice(value.as_bytes());
    Ok(())
}

pub fn strings(values: &[&str]) -> Vec<OsString> {
    values.iter().map(|value| OsString::from(*value)).collect()
}

pub fn os(value: impl AsRef<OsStr>) -> OsString {
    value.as_ref().to_owned()
}
