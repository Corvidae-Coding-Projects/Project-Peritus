//! Task-owned preimages, independent of HEAD and the user's index.

mod capture;
mod export;
mod git_path;
mod head;
mod index;
mod paths;
mod recovery;
mod restore;
mod retention;
mod source_digest;
pub use paths::parent_is_directory;
#[cfg(test)]
mod tests;

use super::repository;
use crate::ProductRunnerError;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::Write as _,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedBaseline {
    tree: String,
    index: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    staged: Option<Vec<index::StagedEntry>>,
    entries: BTreeMap<String, Entry>,
    nested: BTreeMap<String, Self>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    retained: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    object: String,
    mode: String,
    permissions: u32,
}

impl ManagedBaseline {
    pub(crate) fn repository_fingerprint(root: &Path) -> Result<[u8; 32], ProductRunnerError> {
        use sha2::{Digest as _, Sha256};
        let snapshot = Self::capture(root, false)?;
        let head = capture::nested_head(root)?;
        let bytes = serde_json::to_vec(&(head, snapshot)).map_err(failure)?;
        Ok(Sha256::digest(bytes).into())
    }

    pub(crate) fn save(&self, path: &Path) -> Result<(), ProductRunnerError> {
        let parent = path.parent().ok_or_else(|| failure("baseline has no parent"))?;
        fs::create_dir_all(parent).map_err(failure)?;
        let mut file = tempfile::NamedTempFile::new_in(parent).map_err(failure)?;
        serde_json::to_writer(file.as_file_mut(), self).map_err(failure)?;
        file.as_file().sync_all().map_err(failure)?;
        file.persist(path).map_err(failure)?;
        Ok(())
    }

    pub(crate) fn load(path: &Path) -> Result<Option<Self>, ProductRunnerError> {
        match fs::read(path) {
            Ok(bytes) => {
                let value: Self = serde_json::from_slice(&bytes).map_err(failure)?;
                value.validate()?;
                Ok(Some(value))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(failure(error)),
        }
    }

    pub(crate) fn validate(&self) -> Result<(), ProductRunnerError> {
        self.validate_index()?;
        if let Some(object) = &self.retained {
            retention::validate_object(object, self.tree.len())?;
        }
        for object in
            [&self.tree, &self.index].into_iter().chain(self.entries.values().map(|e| &e.object))
        {
            if !matches!(object.len(), 40 | 64) || !object.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(failure("invalid baseline Git object"));
            }
        }
        for (path, entry) in &self.entries {
            validate_path(Path::new(path))?;
            if !matches!(entry.mode.as_str(), "100644" | "100755" | "120000" | "160000") {
                return Err(failure("invalid baseline file mode"));
            }
        }
        for (path, child) in &self.nested {
            validate_path(Path::new(path))?;
            child.validate()?;
        }
        Ok(())
    }

    pub(crate) fn changed_paths(&self, root: &Path) -> Result<Vec<PathBuf>, ProductRunnerError> {
        let current = Self::capture_with_baseline(root, false, Some(self))?;
        let mut paths = std::collections::BTreeSet::new();
        self.compare(&current, Path::new(""), &mut paths);
        Ok(paths.into_iter().collect())
    }

    fn compare(
        &self,
        current: &Self,
        prefix: &Path,
        paths: &mut std::collections::BTreeSet<PathBuf>,
    ) {
        for path in self.entries.keys().chain(current.entries.keys()) {
            if self.entries.get(path) != current.entries.get(path) {
                paths.insert(prefix.join(path));
            }
        }
        for (path, child) in &self.nested {
            if let Some(now) = current.nested.get(path) {
                child.compare(now, &prefix.join(path), paths);
            } else {
                paths.insert(prefix.join(path));
            }
        }
        for (path, child) in &current.nested {
            if !self.nested.contains_key(path) {
                paths.insert(prefix.join(path));
                child.append_paths(&prefix.join(path), paths);
            }
        }
    }

    fn append_paths(&self, prefix: &Path, paths: &mut std::collections::BTreeSet<PathBuf>) {
        paths.extend(self.entries.keys().map(|file| prefix.join(file)));
        for (path, child) in &self.nested {
            child.append_paths(&prefix.join(path), paths);
        }
    }

    pub(crate) fn patch(&self, root: &Path) -> Result<Vec<u8>, ProductRunnerError> {
        let current = Self::capture_with_baseline(root, false, Some(self))?;
        self.patch_against(root, &current, Path::new(""), &retention::owner(root)?)
    }
}

fn validate_path(path: &Path) -> Result<(), ProductRunnerError> {
    if path.as_os_str().is_empty()
        || path.components().any(|part| !matches!(part, std::path::Component::Normal(_)))
        || path.components().any(|part| part.as_os_str().eq_ignore_ascii_case(".git"))
    {
        return Err(failure("baseline path is not relative"));
    }
    Ok(())
}

fn git(
    root: &Path,
    arguments: &[&str],
    input: Option<&[u8]>,
) -> Result<Vec<u8>, ProductRunnerError> {
    let mut command = repository_command(root)?;
    command
        .args(arguments)
        .env("GIT_LITERAL_PATHSPECS", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if input.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = command.spawn().map_err(failure)?;
    if let Some(bytes) = input {
        child
            .stdin
            .take()
            .ok_or_else(|| failure("missing Git input"))?
            .write_all(bytes)
            .map_err(failure)?;
    }
    let output = child.wait_with_output().map_err(failure)?;
    if !output.status.success() {
        return Err(failure(String::from_utf8_lossy(&output.stderr)));
    }
    Ok(output.stdout)
}

fn repository_command(root: &Path) -> Result<Command, ProductRunnerError> {
    let mut command = Command::new("git");
    command.current_dir(root);
    // A submodule's core.worktree still names its missing original directory while
    // recovery operates in a private stage. The explicit caller root owns this operation.
    if fs::symlink_metadata(root.join(".git")).is_ok_and(|metadata| metadata.is_file()) {
        command.env("GIT_WORK_TREE", root.canonicalize().map_err(failure)?);
    }
    Ok(command)
}

fn text(bytes: Vec<u8>) -> Result<String, ProductRunnerError> {
    String::from_utf8(bytes).map(|s| s.trim().to_owned()).map_err(failure)
}

fn failure(error: impl std::fmt::Display) -> ProductRunnerError {
    repository("retain task workspace baseline", error.to_string())
}
