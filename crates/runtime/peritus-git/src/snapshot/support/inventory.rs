//! Cancellable owned-path inspection and exact pre-restore cleanup inventories.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::command::CommandAccess;
use crate::repository::strings;
use crate::{ErrorKind, GitError, GitRepository, Operation, RecoveryClass, RegisteredWorktree};

pub fn inspect_nested_git_metadata(
    repository: &GitRepository,
    worktree: &RegisteredWorktree,
    nested: &[crate::snapshot::RegisteredNestedRepository],
    include_ignored: bool,
    target_tree: Option<crate::TreeId>,
    operation: Operation,
) -> Result<(), GitError> {
    crate::snapshot::nested::validate(repository, worktree, nested, operation)?;
    let root = worktree.root();
    let owned = owned_directories(repository, worktree, target_tree, operation)?;
    let mut directories = vec![root.to_owned()];
    while let Some(directory) = directories.pop() {
        crate::snapshot::nested::check_cancelled(repository, operation)?;
        for entry in std::fs::read_dir(&directory).map_err(|source| {
            GitError::io(operation, RecoveryClass::Reconcile, "scan worktree entries", source)
        })? {
            crate::snapshot::nested::check_cancelled(repository, operation)?;
            let entry = entry.map_err(|source| {
                GitError::io(operation, RecoveryClass::Reconcile, "read worktree entry", source)
            })?;
            let path = entry.path();
            let metadata = std::fs::symlink_metadata(&path).map_err(|source| {
                GitError::io(operation, RecoveryClass::Reconcile, "inspect worktree entry", source)
            })?;
            let is_root_git = directory == root && entry.file_name() == ".git";
            if entry.file_name() == ".git" && !is_root_git {
                return Err(GitError::new(
                    ErrorKind::WorktreeConflict,
                    operation,
                    RecoveryClass::CorrectRequest,
                    "nested Git metadata requires an explicit independent repository registration",
                ));
            }
            if metadata.is_dir()
                && !is_root_git
                && !nested.iter().any(|registration| registration.root() == path)
                && (include_ignored
                    || owned.contains(&path)
                    || !is_ignored(repository, worktree, root, &path, operation)?)
            {
                directories.push(path);
            }
        }
    }
    Ok(())
}

/// Records only presently nonignored untracked leaves before restoration can change ignore rules.
pub(in crate::snapshot) fn cleanup_inventory(
    repository: &GitRepository,
    worktree: &RegisteredWorktree,
) -> Result<Vec<PathBuf>, GitError> {
    let operation = Operation::RestoreSnapshot;
    let output = repository.runner.checked(
        worktree.root(),
        Some(GitRepository::worktree_location(worktree.root(), worktree.git_dir())),
        CommandAccess::Read,
        operation,
        &strings(&["ls-files", "--others", "--exclude-standard", "-z"]),
        None,
    )?;
    output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|bytes| {
            crate::snapshot::nested::check_cancelled(repository, operation)?;
            inventory_path(bytes, operation)
        })
        .collect()
}

pub(in crate::snapshot) fn clean_inventory(
    repository: &GitRepository,
    worktree: &RegisteredWorktree,
    paths: &[PathBuf],
) -> Result<(), GitError> {
    // A command batch bounds argv transport, never the number of files in the operation.
    for batch in paths.chunks(64) {
        let mut args = strings(&["clean", "-fd", "--"]);
        args.extend(batch.iter().map(|path| path.as_os_str().to_owned()));
        repository.runner.checked(
            worktree.root(),
            Some(GitRepository::worktree_location(worktree.root(), worktree.git_dir())),
            CommandAccess::Write,
            Operation::RestoreSnapshot,
            &args,
            None,
        )?;
    }
    Ok(())
}

fn owned_directories(
    repository: &GitRepository,
    worktree: &RegisteredWorktree,
    target_tree: Option<crate::TreeId>,
    operation: Operation,
) -> Result<BTreeSet<PathBuf>, GitError> {
    let mut arguments = vec![strings(&["ls-files", "--cached", "-z"])];
    if let Some(tree) = target_tree {
        arguments.push(vec![
            OsString::from("ls-tree"),
            OsString::from("--name-only"),
            OsString::from("-rz"),
            OsString::from(tree.to_string()),
        ]);
    }
    let mut directories = BTreeSet::new();
    for args in arguments {
        let output = repository.runner.checked(
            worktree.root(),
            Some(GitRepository::worktree_location(worktree.root(), worktree.git_dir())),
            CommandAccess::Read,
            operation,
            &args,
            None,
        )?;
        for bytes in output.stdout.split(|byte| *byte == 0).filter(|path| !path.is_empty()) {
            crate::snapshot::nested::check_cancelled(repository, operation)?;
            let path = inventory_path(bytes, operation)?;
            // Include the leaf because a gitlink itself names an owned directory boundary.
            for ancestor in path.ancestors() {
                directories.insert(worktree.root().join(ancestor));
            }
        }
    }
    Ok(directories)
}

pub(in crate::snapshot) fn inventory_path(
    bytes: &[u8],
    operation: Operation,
) -> Result<PathBuf, GitError> {
    #[cfg(unix)]
    let path = {
        use std::os::unix::ffi::OsStrExt as _;
        PathBuf::from(std::ffi::OsStr::from_bytes(bytes))
    };
    #[cfg(not(unix))]
    let path = std::str::from_utf8(bytes).map(PathBuf::from).map_err(|_| {
        GitError::new(
            ErrorKind::GitProtocol,
            operation,
            RecoveryClass::Reconcile,
            "Git inventory path is not UTF-8",
        )
    })?;
    if path.is_absolute()
        || path.components().any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err(GitError::new(
            ErrorKind::GitProtocol,
            operation,
            RecoveryClass::Reconcile,
            "Git inventory path escapes the worktree",
        ));
    }
    Ok(path)
}

fn is_ignored(
    repository: &GitRepository,
    worktree: &RegisteredWorktree,
    root: &Path,
    path: &Path,
    operation: Operation,
) -> Result<bool, GitError> {
    let relative = path.strip_prefix(root).map_err(|_| {
        GitError::new(
            ErrorKind::WorktreeConflict,
            operation,
            RecoveryClass::Reconcile,
            "worktree child escaped its registered root during candidate inspection",
        )
    })?;
    let arguments = vec![
        OsString::from("check-ignore"),
        OsString::from("--quiet"),
        OsString::from("--no-index"),
        OsString::from("--"),
        relative.as_os_str().to_owned(),
    ];
    let output = repository.runner.observe(
        root,
        Some(GitRepository::worktree_location(worktree.root(), worktree.git_dir())),
        CommandAccess::ReadWithoutLiteralPathspecs,
        operation,
        &arguments,
        None,
    )?;
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(GitError::new(
            ErrorKind::WorktreeConflict,
            operation,
            RecoveryClass::Reconcile,
            "Git could not classify candidate directory ownership",
        )),
    }
}
