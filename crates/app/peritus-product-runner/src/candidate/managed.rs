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
pub mod transaction;
pub use paths::parent_is_directory;
#[cfg(test)]
mod tests;

use super::repository;
use crate::ProductRunnerError;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
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

#[derive(Clone)]
pub(crate) struct EvidenceContent {
    pub(crate) repository: PathBuf,
    pub(crate) object: String,
    pub(crate) mode: String,
    pub(crate) permissions: u32,
}

pub(crate) struct EvidenceEntry {
    pub(crate) path: PathBuf,
    pub(crate) before: Option<EvidenceContent>,
    pub(crate) after: Option<EvidenceContent>,
}

pub(crate) struct EvidenceSnapshot {
    pub(crate) entries: Vec<EvidenceEntry>,
    pub(crate) exclusions: Vec<PathBuf>,
    pub(crate) content_digest: peritus_types::Sha256Digest,
    pub(crate) repository_digest: peritus_types::Sha256Digest,
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

    pub(crate) fn evidence_snapshot(
        &self,
        root: &Path,
    ) -> Result<EvidenceSnapshot, ProductRunnerError> {
        let current = Self::capture_with_baseline(root, false, Some(self))?;
        let checkpoint_head = capture::nested_head(root)?
            .ok_or_else(|| failure("managed candidate repository has no committed HEAD"))?;
        let content_digest = evidence_source_digest(&current, root)?;
        let repository_digest = evidence_repository_digest(&current, checkpoint_head)?;
        let owner = retention::owner(root)?;
        let mut entries = Vec::new();
        collect_evidence(
            Some(self),
            Some(&current),
            root,
            root,
            Path::new(""),
            &owner,
            &mut entries,
        )?;
        entries.sort_by(|left, right| left.path.cmp(&right.path));
        let mut exclusions = BTreeSet::new();
        collect_exclusions(&current, root, Path::new(""), &mut exclusions)?;
        Ok(EvidenceSnapshot {
            entries,
            exclusions: exclusions.into_iter().collect(),
            content_digest,
            repository_digest,
        })
    }

    pub(crate) fn candidate_content_digest(
        &self,
        root: &Path,
    ) -> Result<peritus_types::Sha256Digest, ProductRunnerError> {
        let current = Self::capture_with_baseline(root, false, Some(self))?;
        evidence_source_digest(&current, root)
    }

    pub(crate) fn candidate_checkpoint(
        &self,
        root: &Path,
    ) -> Result<crate::progress::WorkspaceCheckpoint, ProductRunnerError> {
        let current = Self::capture_with_baseline(root, false, Some(self))?;
        let checkpoint_head = capture::nested_head(root)?
            .ok_or_else(|| failure("managed candidate repository has no committed HEAD"))?;
        let fingerprint = repository_fingerprint(&current, Some(checkpoint_head.clone()))?;
        Ok(crate::progress::WorkspaceCheckpoint::managed_snapshot(
            checkpoint_head,
            fingerprint,
        ))
    }
}

#[allow(clippy::too_many_arguments, reason = "each recursive repository has two exact roots")]
fn collect_evidence(
    before: Option<&ManagedBaseline>,
    after: Option<&ManagedBaseline>,
    before_root: &Path,
    after_root: &Path,
    prefix: &Path,
    owner: &Path,
    output: &mut Vec<EvidenceEntry>,
) -> Result<(), ProductRunnerError> {
    let mut names = BTreeSet::new();
    if let Some(snapshot) = before {
        names.extend(snapshot.entries.keys().cloned());
    }
    if let Some(snapshot) = after {
        names.extend(snapshot.entries.keys().cloned());
    }
    for name in names {
        let old = before.and_then(|snapshot| snapshot.entries.get(&name));
        let new = after.and_then(|snapshot| snapshot.entries.get(&name));
        if old == new {
            continue;
        }
        output.push(EvidenceEntry {
            path: prefix.join(&name),
            before: old.map(|entry| evidence_content(before_root, entry)),
            after: new.map(|entry| evidence_content(after_root, entry)),
        });
    }

    let mut nested = BTreeSet::new();
    if let Some(snapshot) = before {
        nested.extend(snapshot.nested.keys().cloned());
    }
    if let Some(snapshot) = after {
        nested.extend(snapshot.nested.keys().cloned());
    }
    for name in nested {
        let old = before.and_then(|snapshot| snapshot.nested.get(&name));
        let new = after.and_then(|snapshot| snapshot.nested.get(&name));
        let old_root = match old {
            Some(snapshot) if snapshot.retained.is_some() => {
                retention::source_root(owner, snapshot)?
            }
            Some(_) => before_root.join(&name),
            None => before_root.join(&name),
        };
        let new_root = after_root.join(&name);
        collect_evidence(
            old,
            new,
            &old_root,
            &new_root,
            &prefix.join(&name),
            owner,
            output,
        )?;
    }
    Ok(())
}

fn evidence_content(root: &Path, entry: &Entry) -> EvidenceContent {
    EvidenceContent {
        repository: root.to_owned(),
        object: entry.object.clone(),
        mode: entry.mode.clone(),
        permissions: entry.permissions,
    }
}

fn evidence_source_digest(
    snapshot: &ManagedBaseline,
    root: &Path,
) -> Result<peritus_types::Sha256Digest, ProductRunnerError> {
    use sha2::{Digest as _, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(b"peritus-source-v1\0");
    hash_evidence_source(snapshot, root, &mut hasher)?;
    Ok(peritus_types::Sha256Digest::new(hasher.finalize().into()))
}

fn evidence_repository_digest(
    snapshot: &ManagedBaseline,
    checkpoint_head: String,
) -> Result<peritus_types::Sha256Digest, ProductRunnerError> {
    let fingerprint = repository_fingerprint(snapshot, Some(checkpoint_head.clone()))?;
    Ok(crate::progress::WorkspaceCheckpoint::managed_snapshot(
        checkpoint_head,
        fingerprint,
    )
    .digest())
}

fn repository_fingerprint(
    snapshot: &ManagedBaseline,
    head: Option<String>,
) -> Result<[u8; 32], ProductRunnerError> {
    use sha2::{Digest as _, Sha256};

    let bytes = serde_json::to_vec(&(head, snapshot)).map_err(failure)?;
    Ok(Sha256::digest(bytes).into())
}

fn hash_evidence_source(
    snapshot: &ManagedBaseline,
    root: &Path,
    hasher: &mut sha2::Sha256,
) -> Result<(), ProductRunnerError> {
    use sha2::Digest as _;

    let metadata = fs::metadata(root).map_err(failure)?;
    hasher.update(file_metadata::permission_fingerprint(&metadata).to_le_bytes());
    for (path, entry) in &snapshot.entries {
        if snapshot.nested.contains_key(path) {
            continue;
        }
        hasher.update([0]);
        let bytes = serde_json::to_vec(&(path, entry)).map_err(failure)?;
        hasher.update(u64::try_from(bytes.len()).map_err(failure)?.to_le_bytes());
        hasher.update(bytes);
    }
    for (path, child) in &snapshot.nested {
        hasher.update([1]);
        hasher.update(u64::try_from(path.len()).map_err(failure)?.to_le_bytes());
        hasher.update(path.as_bytes());
        hash_evidence_source(child, &root.join(path), hasher)?;
    }
    hasher.update([2]);
    Ok(())
}

fn collect_exclusions(
    snapshot: &ManagedBaseline,
    root: &Path,
    prefix: &Path,
    output: &mut BTreeSet<PathBuf>,
) -> Result<(), ProductRunnerError> {
    let untracked = git(root, &["ls-files", "-z", "--others", "--exclude-standard"], None)?;
    for encoded in untracked.split(|byte| *byte == 0).filter(|value| !value.is_empty()) {
        let path = std::str::from_utf8(encoded).map_err(failure)?.trim_end_matches('/');
        let relative = prefix.join(path);
        if crate::workspace_filter::generated(&relative) {
            output.insert(relative);
        }
    }
    for (name, child) in &snapshot.nested {
        collect_exclusions(child, &root.join(name), &prefix.join(name), output)?;
    }
    Ok(())
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
        .env("GIT_LITERAL_PATHSPECS", "1");
    let mut stdout = Vec::new();
    let completed = super::process::stream_current(
        command,
        input,
        &mut stdout,
        "observe retained task workspace with Git",
    )?;
    if !completed.status.success() {
        return Err(failure(String::from_utf8_lossy(&completed.stderr)));
    }
    Ok(stdout)
}

fn repository_command(root: &Path) -> Result<Command, ProductRunnerError> {
    let mut command = Command::new("git");
    // Rust preserves Windows filesystem identity with a verbatim path, while Git's process
    // boundary requires the ordinary drive/UNC spelling used for local repository operands.
    #[cfg(windows)]
    command.current_dir(PathBuf::from(git_path::local_path(root)?));
    #[cfg(not(windows))]
    command.current_dir(root);
    // A submodule's core.worktree still names its missing original directory while
    // recovery operates in a private stage. The explicit caller root owns this operation.
    if fs::symlink_metadata(root.join(".git")).is_ok_and(|metadata| metadata.is_file()) {
        #[cfg(windows)]
        command.env("GIT_WORK_TREE", git_path::local_path(root)?);
        #[cfg(not(windows))]
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
