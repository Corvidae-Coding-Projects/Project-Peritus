//! Caller-rooted repository ownership, Git isolation, and guarded cleanup.

use super::environment::{GitCommandContext, isolated_git_command};
use super::filesystem::{
    create_safe_directories, create_symlink, guarded_cleanup, reject_existing_path,
    reject_existing_symlink, validate_new_root, write_owner_marker,
};
use super::{GitCommandOutput, GitObjectId, TempRepositoryError, TempRepositoryErrorKind};
use crate::FixturePath;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};

/// The target type for an explicitly adversarial fixture symlink.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FixtureSymlinkKind {
    /// A link interpreted as a file on platforms that require a distinction.
    File,
    /// A link interpreted as a directory on platforms that require a distinction.
    Directory,
}

/// Builder for one exclusively owned, caller-rooted Git repository.
#[derive(Clone, Debug)]
pub struct TemporaryRepositoryBuilder {
    owned_root: PathBuf,
    initial_branch: String,
    bare: bool,
    git_program: OsString,
}

impl TemporaryRepositoryBuilder {
    /// Selects an explicit root that does not yet exist.
    ///
    /// The final component must begin with `peritus-test-`; this guards recursive cleanup from
    /// broad or accidentally user-owned targets.
    #[must_use]
    pub fn new(owned_root: impl Into<PathBuf>) -> Self {
        Self {
            owned_root: owned_root.into(),
            initial_branch: "main".to_owned(),
            bare: false,
            git_program: OsString::from("git"),
        }
    }

    /// Sets the exact initial branch passed to Git for validation.
    #[must_use]
    pub fn initial_branch(mut self, initial_branch: impl Into<String>) -> Self {
        self.initial_branch = initial_branch.into();
        self
    }

    /// Selects a bare or worktree repository.
    #[must_use]
    pub const fn bare(mut self, bare: bool) -> Self {
        self.bare = bare;
        self
    }

    /// Selects the Git executable without invoking a shell.
    #[must_use]
    pub fn git_program(mut self, git_program: impl Into<OsString>) -> Self {
        self.git_program = git_program.into();
        self
    }

    /// Exclusively creates the owned root, isolated Git configuration, and repository.
    ///
    /// # Errors
    ///
    /// Returns [`TempRepositoryError`] when ownership cannot be proven or initialization fails.
    pub fn build(self) -> Result<TemporaryRepository, TempRepositoryError> {
        validate_new_root(&self.owned_root)?;
        fs::create_dir(&self.owned_root).map_err(|source| {
            TempRepositoryError::sourced(
                TempRepositoryErrorKind::Io,
                &self.owned_root,
                "could not exclusively create owned test root",
                source,
            )
        })?;
        let mut creation_guard = CreationGuard::new(self.owned_root.clone());
        write_owner_marker(&self.owned_root).map_err(|source| {
            TempRepositoryError::sourced(
                TempRepositoryErrorKind::Io,
                &self.owned_root,
                "could not establish repository ownership marker",
                source,
            )
        })?;
        let repository_root = self.owned_root.join("repository");
        let hooks_root = self.owned_root.join("disabled-hooks");
        let global_config = self.owned_root.join("isolated-gitconfig");
        let process_temp = self.owned_root.join("process-temp");
        fs::create_dir(&repository_root)
            .and_then(|()| fs::create_dir(&hooks_root))
            .and_then(|()| fs::create_dir(&process_temp))
            .and_then(|()| fs::write(&global_config, []))
            .map_err(|source| {
                TempRepositoryError::sourced(
                    TempRepositoryErrorKind::Io,
                    &self.owned_root,
                    "could not establish isolated repository files",
                    source,
                )
            })?;
        let repository = TemporaryRepository {
            owned_root: self.owned_root,
            repository_root,
            hooks_root,
            global_config,
            process_temp,
            git_program: self.git_program,
            bare: self.bare,
            cleanup_armed: true,
        };
        creation_guard.disarm();
        let mut args = vec![OsString::from("init")];
        if repository.bare {
            args.push(OsString::from("--bare"));
        }
        args.push(OsString::from(format!("--initial-branch={}", self.initial_branch)));
        repository.git_success(args)?;
        Ok(repository)
    }
}

#[derive(Debug)]
struct CreationGuard {
    root: PathBuf,
    armed: bool,
}

impl CreationGuard {
    const fn new(root: PathBuf) -> Self {
        Self { root, armed: true }
    }

    const fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for CreationGuard {
    fn drop(&mut self) {
        if self.armed {
            let _cleanup = super::filesystem::guarded_cleanup_partial(&self.root);
        }
    }
}

/// An owned Git repository under an explicit guarded temporary root.
#[derive(Debug)]
pub struct TemporaryRepository {
    owned_root: PathBuf,
    repository_root: PathBuf,
    hooks_root: PathBuf,
    global_config: PathBuf,
    process_temp: PathBuf,
    git_program: OsString,
    bare: bool,
    cleanup_armed: bool,
}

impl TemporaryRepository {
    /// Returns the Git repository root, excluding support-owned isolation files.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.repository_root
    }

    /// Returns whether this is a bare repository.
    #[must_use]
    pub const fn is_bare(&self) -> bool {
        self.bare
    }

    /// Writes exact bytes to a contained, non-symlink worktree path.
    ///
    /// # Errors
    ///
    /// Returns a typed bare-repository, unsafe-path, or I/O failure.
    pub fn write(&mut self, path: &FixturePath, bytes: &[u8]) -> Result<(), TempRepositoryError> {
        self.require_worktree()?;
        let target = self.prepare_parent(path)?;
        reject_existing_symlink(&target)?;
        fs::write(&target, bytes).map_err(|source| {
            TempRepositoryError::sourced(
                TempRepositoryErrorKind::Io,
                target,
                "could not write repository fixture bytes",
                source,
            )
        })
    }

    /// Writes exact UTF-8 bytes without line-ending normalization.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::write`].
    pub fn write_text(
        &mut self,
        path: &FixturePath,
        text: &str,
    ) -> Result<(), TempRepositoryError> {
        self.write(path, text.as_bytes())
    }

    /// Creates a contained directory and all safe missing ancestors.
    ///
    /// # Errors
    ///
    /// Returns a typed bare-repository, unsafe-path, or I/O failure.
    pub fn create_dir(&mut self, path: &FixturePath) -> Result<(), TempRepositoryError> {
        self.require_worktree()?;
        create_safe_directories(&self.repository_root, path)
    }

    /// Creates an explicitly adversarial symlink without later allowing writes through it.
    ///
    /// # Errors
    ///
    /// Returns a typed platform, unsafe-path, existing-target, or I/O failure.
    pub fn create_adversarial_symlink(
        &mut self,
        link: &FixturePath,
        target: impl AsRef<Path>,
        kind: FixtureSymlinkKind,
    ) -> Result<(), TempRepositoryError> {
        self.require_worktree()?;
        let link_path = self.prepare_parent(link)?;
        reject_existing_path(&link_path)?;
        create_symlink(target.as_ref(), &link_path, kind)
    }

    /// Runs Git with isolated configuration and returns complete output for every exit status.
    ///
    /// # Errors
    ///
    /// Returns [`TempRepositoryErrorKind::GitSpawn`] only when the process cannot be launched.
    pub fn run_git<I, S>(&self, args: I) -> Result<GitCommandOutput, TempRepositoryError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = isolated_git_command(
            &GitCommandContext {
                git_program: &self.git_program,
                repository_root: &self.repository_root,
                hooks_root: &self.hooks_root,
                global_config: &self.global_config,
                process_temp: &self.process_temp,
                bare: self.bare,
            },
            args,
            std::env::vars_os(),
        );
        let output = command.output().map_err(|source| {
            TempRepositoryError::sourced(
                TempRepositoryErrorKind::GitSpawn,
                &self.git_program,
                "could not launch isolated Git",
                source,
            )
        })?;
        Ok(GitCommandOutput::new(output.status, output.stdout, output.stderr))
    }

    /// Runs Git and requires a successful exit while preserving failed output.
    ///
    /// # Errors
    ///
    /// Returns a spawn error or [`TempRepositoryErrorKind::GitFailed`] with complete output.
    pub fn git_success<I, S>(&self, args: I) -> Result<GitCommandOutput, TempRepositoryError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let output = self.run_git(args)?;
        if output.success() { Ok(output) } else { Err(TempRepositoryError::git_failed(output)) }
    }

    /// Stages the worktree, creates one deterministic commit, and returns its opaque object ID.
    ///
    /// # Errors
    ///
    /// Returns a typed bare-repository, Git, UTF-8, or object-ID failure.
    pub fn commit_all(&self, message: &str) -> Result<GitObjectId, TempRepositoryError> {
        self.require_worktree()?;
        self.git_success(["add", "--all"])?;
        self.git_success(["commit", "--message", message])?;
        let output = self.git_success(["rev-parse", "--verify", "HEAD"])?;
        let value = std::str::from_utf8(output.stdout()).map_err(|source| {
            TempRepositoryError::sourced(
                TempRepositoryErrorKind::NonUtf8ObjectId,
                &self.repository_root,
                "Git object identifier was not UTF-8",
                source,
            )
        })?;
        GitObjectId::new(value.trim_end().to_owned())
    }

    /// Performs guarded recursive cleanup and reports any failure.
    ///
    /// # Errors
    ///
    /// Returns [`TempRepositoryErrorKind::Cleanup`] if ownership cannot be re-proved or removal
    /// fails. Drop otherwise attempts the same guarded cleanup on a best-effort basis.
    pub fn close(mut self) -> Result<(), TempRepositoryError> {
        guarded_cleanup(&self.owned_root)?;
        self.cleanup_armed = false;
        Ok(())
    }

    fn require_worktree(&self) -> Result<(), TempRepositoryError> {
        if self.bare {
            Err(TempRepositoryError::at(
                TempRepositoryErrorKind::BareRepository,
                &self.repository_root,
                "operation requires a Git worktree",
            ))
        } else {
            Ok(())
        }
    }

    fn prepare_parent(&self, path: &FixturePath) -> Result<PathBuf, TempRepositoryError> {
        let segments: Vec<_> = path.as_str().split('/').collect();
        if segments.len() > 1 {
            let parent =
                FixturePath::new(segments[..segments.len() - 1].join("/")).map_err(|error| {
                    TempRepositoryError::new(TempRepositoryErrorKind::UnsafePath, error.to_string())
                })?;
            create_safe_directories(&self.repository_root, &parent)?;
        }
        Ok(self.repository_root.join(path.as_path()))
    }
}

impl Drop for TemporaryRepository {
    fn drop(&mut self) {
        if self.cleanup_armed && guarded_cleanup(&self.owned_root).is_ok() {
            self.cleanup_armed = false;
        }
    }
}
