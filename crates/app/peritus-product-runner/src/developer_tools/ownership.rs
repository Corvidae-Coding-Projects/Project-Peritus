//! Workspace file ownership retained across one complete product run.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use peritus_agent::DeveloperLoopError;

use super::path::{protected_metadata, targets_default_exclusion, tool};

/// Distinguishes baseline and model-caused files from unrelated late external evidence.
#[derive(Clone)]
pub struct WorkspaceOwnership {
    direct: bool,
    baseline: BTreeSet<PathBuf>,
    directly_created: BTreeSet<PathBuf>,
    command_created: BTreeSet<PathBuf>,
    baseline_error: Option<String>,
}

impl WorkspaceOwnership {
    /// Captures regular files already present when the product run begins.
    #[must_use]
    pub fn capture(root: &Path) -> Self {
        Self::try_capture(root).unwrap_or_else(|error| Self {
            direct: false,
            baseline: BTreeSet::new(),
            directly_created: BTreeSet::new(),
            command_created: BTreeSet::new(),
            baseline_error: Some(error.to_string()),
        })
    }

    /// Captures a complete regular-file baseline or reports the exact inventory omission.
    ///
    /// # Errors
    /// Returns a tool error when any directory entry or metadata needed for ownership cannot be
    /// observed. An incomplete inventory must never silently confer ownership later in the run.
    pub(crate) fn try_capture(root: &Path) -> Result<Self, DeveloperLoopError> {
        Ok(Self {
            direct: false,
            baseline: regular_files(root).map_err(tool)?,
            directly_created: BTreeSet::new(),
            command_created: BTreeSet::new(),
            baseline_error: None,
        })
    }

    /// Tracks only explicitly inspected/created files in an in-place folder; never scans its tree.
    pub(crate) const fn direct() -> Self {
        Self {
            direct: true,
            baseline: BTreeSet::new(),
            directly_created: BTreeSet::new(),
            command_created: BTreeSet::new(),
            baseline_error: None,
        }
    }

    /// A direct-folder read establishes a bounded, exact-file target for subsequent requested work.
    pub(super) fn observe_file(&mut self, path: PathBuf) {
        if self.direct {
            self.baseline.insert(path);
        }
    }

    /// Captures files that existed outside the run's current ownership immediately before a
    /// structured command. A later comparison attributes only files newly produced by that
    /// command, preserving unrelated files that appeared through another actor.
    #[must_use]
    pub(super) fn unowned_files(
        &self,
        root: &Path,
    ) -> Result<BTreeSet<PathBuf>, DeveloperLoopError> {
        if self.direct {
            return Ok(BTreeSet::new());
        }
        if let Some(error) = &self.baseline_error {
            return Err(tool(format!(
                "workspace ownership baseline is incomplete: {error}",
            )));
        }
        let observed = match untracked_files(root).map_err(tool)? {
            Some(files) => files,
            None => regular_files(root).map_err(tool)?,
        };
        Ok(observed
            .into_iter()
            .filter(|path| {
                !self.baseline.contains(path)
                    && !self.directly_created.contains(path)
                    && !self.command_created.contains(path)
            })
            .collect())
    }

    /// Records regular files that appeared while one harness-owned command was executing.
    pub(super) fn record_command_creations(
        &mut self,
        root: &Path,
        unowned_before: &BTreeSet<PathBuf>,
    ) -> Result<(), DeveloperLoopError> {
        let unowned_after = self.unowned_files(root)?;
        for path in unowned_after.difference(unowned_before) {
            self.command_created.insert(path.clone());
        }
        Ok(())
    }

    /// Records a new file created through the explicit text-write tool.
    pub fn record_direct_creation(&mut self, path: &Path, existed_before: bool) {
        if !existed_before {
            self.directly_created.insert(path.to_path_buf());
        }
    }

    /// Returns whether source layout belongs to the starting workspace or was authored directly.
    ///
    /// Files produced later by compilers, generators, archive extraction, or other observed
    /// commands retain their upstream/generated structure instead of being misclassified as new
    /// first-party architecture. A file created through the explicit text-write tool remains
    /// first-party and remains subject to exact-target source checks.
    #[must_use]
    pub fn source_layout_applies(&self, path: &Path) -> bool {
        !targets_default_exclusion(Some(path))
            && (self.baseline.contains(path) || self.directly_created.contains(path))
    }

    /// Allows exact-file removal only when this product run has a defensible ownership claim.
    pub fn ensure_removable(&self, path: &Path) -> Result<(), DeveloperLoopError> {
        if self.baseline.contains(path)
            || self.directly_created.contains(path)
            || self.command_created.contains(path)
        {
            return Ok(());
        }
        if let Some(error) = &self.baseline_error {
            return Err(tool(format!(
                "refusing removal because the starting workspace ownership inventory was incomplete: {error}",
            )));
        }
        Err(tool(
            "refusing to remove a file that appeared after this product run began without exact current user-request authority for this path and preimage",
        ))
    }
}

fn untracked_files(root: &Path) -> Result<Option<BTreeSet<PathBuf>>, String> {
    let workspace = root
        .canonicalize()
        .map_err(|error| format!("canonicalize workspace ownership root: {error}"))?;
    let Ok(repository) = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(root)
        .output()
    else {
        return Ok(None);
    };
    if !repository.status.success() {
        return Ok(None);
    }
    let repository = trim_line_end(&repository.stdout);
    if repository.is_empty() {
        return Err("Git returned an empty repository root during ownership inventory".to_owned());
    }
    let repository = native_git_path(repository)?
        .canonicalize()
        .map_err(|error| format!("canonicalize Git ownership root: {error}"))?;
    if repository != workspace {
        return Ok(None);
    }
    let output = Command::new("git")
        .args(["ls-files", "--others", "-z"])
        .current_dir(root)
        .output()
        .map_err(|error| format!("run Git ownership inventory: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Git ownership inventory failed with status {}",
            output.status,
        ));
    }
    let mut files = BTreeSet::new();
    for encoded in output.stdout.split(|byte| *byte == 0).filter(|path| !path.is_empty()) {
        let relative = native_git_path(encoded)?;
        if relative.is_absolute()
            || relative.components().any(|component| {
                matches!(
                    component,
                    std::path::Component::ParentDir
                        | std::path::Component::RootDir
                        | std::path::Component::Prefix(_)
                )
            })
        {
            return Err("Git ownership inventory returned a path outside the workspace".to_owned());
        }
        let path = root.join(&relative);
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            format!("inspect Git ownership entry {}: {error}", relative.display())
        })?;
        if metadata.is_file() {
            files.insert(path);
        }
    }
    Ok(Some(files))
}

fn regular_files(root: &Path) -> Result<BTreeSet<PathBuf>, String> {
    let mut files = BTreeSet::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let children = fs::read_dir(&directory).map_err(|error| {
            format!("read ownership directory {}: {error}", directory.display())
        })?;
        for child in children {
            let child = child.map_err(|error| {
                format!("read ownership entry below {}: {error}", directory.display())
            })?;
            let path = child.path();
            let relative = path.strip_prefix(root).map_err(|_| {
                format!("ownership entry escaped workspace root: {}", path.display())
            })?;
            if protected_metadata(relative) {
                continue;
            }
            let kind = child.file_type().map_err(|error| {
                format!("inspect ownership entry {}: {error}", path.display())
            })?;
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file() {
                files.insert(path);
            }
        }
    }
    Ok(files)
}

fn trim_line_end(mut bytes: &[u8]) -> &[u8] {
    while bytes.last().is_some_and(|byte| matches!(*byte, b'\r' | b'\n')) {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
}

#[cfg(unix)]
fn native_git_path(encoded: &[u8]) -> Result<PathBuf, String> {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt as _};

    Ok(PathBuf::from(OsString::from_vec(encoded.to_vec())))
}

#[cfg(not(unix))]
fn native_git_path(encoded: &[u8]) -> Result<PathBuf, String> {
    String::from_utf8(encoded.to_vec())
        .map(PathBuf::from)
        .map_err(|_| "Git ownership inventory returned a path that is not native text".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn initialize_repository(root: &Path) {
        let status = Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(root)
            .status()
            .expect("git init");
        assert!(status.success());
    }

    #[test]
    fn late_external_file_is_not_owned_by_the_product_run() {
        let workspace = tempfile::tempdir().expect("workspace");
        let baseline = workspace.path().join("baseline.txt");
        fs::write(&baseline, "before").expect("baseline file");
        let mut ownership = WorkspaceOwnership::capture(workspace.path());

        let external = workspace.path().join("api_access.log");
        fs::write(&external, "/projects\n").expect("external evidence");
        assert!(ownership.ensure_removable(&baseline).is_ok());
        assert!(ownership.ensure_removable(&external).is_err());

        let direct = workspace.path().join("draft.txt");
        ownership.record_direct_creation(&direct, false);
        assert!(ownership.ensure_removable(&direct).is_ok());

        let unowned_before = ownership.unowned_files(workspace.path()).expect("pre-command scan");
        let command_output = workspace.path().join("generated-report.txt");
        fs::write(&command_output, "result\n").expect("command output");
        ownership
            .record_command_creations(workspace.path(), &unowned_before)
            .expect("post-command scan");
        assert!(ownership.ensure_removable(&command_output).is_ok());
        assert!(ownership.ensure_removable(&external).is_err());

        assert!(ownership.source_layout_applies(&baseline));
        assert!(ownership.source_layout_applies(&direct));
        assert!(!ownership.source_layout_applies(&external));
        assert!(!ownership.source_layout_applies(&command_output));
    }

    #[test]
    fn ignored_folder_does_not_inherit_parent_repository_ownership() {
        let parent = tempfile::tempdir().expect("parent repository");
        initialize_repository(parent.path());
        fs::write(parent.path().join(".gitignore"), "workspace/\n").expect("ignore workspace");
        let workspace = parent.path().join("workspace");
        fs::create_dir(&workspace).expect("workspace");

        let mut ownership = WorkspaceOwnership::capture(&workspace);
        let external = workspace.join("late-external.log");
        fs::write(&external, "preserve\n").expect("external evidence");
        let unowned_before = ownership.unowned_files(&workspace).expect("pre-command scan");
        let command_output = workspace.join("generated-report.txt");
        fs::write(&command_output, "result\n").expect("command output");
        ownership
            .record_command_creations(&workspace, &unowned_before)
            .expect("post-command scan");

        assert!(ownership.ensure_removable(&command_output).is_ok());
        assert!(ownership.ensure_removable(&external).is_err());
    }
}
