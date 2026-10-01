//! Independently retained nested Git history and metadata in the enclosing workspace.

mod linked;
mod metadata;
mod restore;
mod store;

use super::{Entry, ManagedBaseline, failure, git, text};
use crate::ProductRunnerError;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub(super) use restore::Replacements;
use store::Store;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    linked: Option<PathBuf>,
    source_tree: String,
    source_index: String,
    roots: Vec<String>,
    entries: BTreeMap<String, Entry>,
    directories: BTreeMap<String, u32>,
    root_permissions: u32,
    git_permissions: u32,
}

pub(super) fn capture_children(
    root: &Path,
    snapshot: &mut ManagedBaseline,
) -> Result<(), ProductRunnerError> {
    let owner = owner(root)?;
    capture_nested(root, snapshot, &owner)
}

fn capture_nested(
    root: &Path,
    snapshot: &mut ManagedBaseline,
    owner: &Path,
) -> Result<(), ProductRunnerError> {
    for (path, child) in &mut snapshot.nested {
        let root = root.join(path);
        // Linked Git databases already live independently of the nested source root.
        // Never copy or relocate another worktree's administrative directory implicitly.
        let common = PathBuf::from(text(git(
            &root,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
            None,
        )?)?)
        .canonicalize()
        .map_err(failure)?;
        let marker = fs::symlink_metadata(root.join(".git")).map_err(failure)?;
        if marker.is_file() {
            let store = Store::prepare(owner, child.tree.len())?;
            child.retained = Some(linked::capture(&store, &root, child)?);
        } else if marker.is_dir() && common == self::owner(&root)? {
            let store = Store::prepare(owner, child.tree.len())?;
            child.retained = Some(metadata::capture(&store, &root, child)?);
        }
        capture_nested(&root, child, owner)?;
    }
    Ok(())
}

pub(super) fn owner(root: &Path) -> Result<PathBuf, ProductRunnerError> {
    PathBuf::from(text(git(root, &["rev-parse", "--absolute-git-dir"], None)?)?)
        .canonicalize()
        .map_err(failure)
}

pub(super) fn linked_owners(
    owner: &Path,
    baseline: &ManagedBaseline,
) -> Result<std::collections::BTreeSet<PathBuf>, ProductRunnerError> {
    let mut owners = std::collections::BTreeSet::new();
    for child in baseline.nested.values() {
        if child.retained.is_some()
            && let Some(directory) = Store::existing(owner, child.tree.len())?.load(child)?.linked
        {
            owners.insert(directory);
        }
        owners.extend(linked_owners(owner, child)?);
    }
    Ok(owners)
}

pub(super) fn source_root(
    owner: &Path,
    baseline: &ManagedBaseline,
) -> Result<PathBuf, ProductRunnerError> {
    let store = Store::existing(owner, baseline.tree.len())?;
    store.load(baseline)?;
    Ok(store.root)
}

pub(super) fn validate_object(object: &str, length: usize) -> Result<(), ProductRunnerError> {
    if !matches!(length, 40 | 64)
        || object.len() != length
        || !object.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(failure("invalid independent repository backup object"));
    }
    Ok(())
}

fn validate_metadata_path(path: &str) -> Result<(), ProductRunnerError> {
    if path.is_empty()
        || Path::new(path).components().any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err(failure("invalid independent repository metadata path"));
    }
    Ok(())
}
