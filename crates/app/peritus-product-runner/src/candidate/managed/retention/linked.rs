//! Preserve linked worktree identity without copying or relocating its shared Git database.

use super::{Entry, ManagedBaseline, Manifest, Store, failure, git, metadata, owner, text};
use crate::{ProductRunnerError, file_metadata};
use std::{collections::BTreeMap, fs, path::Path};

pub(super) const MARKER: &str = "git-file";

pub(super) fn capture(
    store: &Store,
    root: &Path,
    baseline: &ManagedBaseline,
) -> Result<String, ProductRunnerError> {
    let directory = owner(root)?;
    let marker = root.join(".git");
    let permissions =
        file_metadata::permission_fingerprint(&fs::symlink_metadata(&marker).map_err(failure)?);
    let object = text(git(
        &store.root,
        &["-c", "core.fsync=all", "hash-object", "-w", "--stdin"],
        Some(&fs::read(&marker).map_err(failure)?),
    )?)?;
    let manifest = Manifest {
        linked: Some(directory),
        source_tree: baseline.tree.clone(),
        source_index: baseline.index.clone(),
        roots: store.fetch_repository(root)?,
        entries: BTreeMap::from([(
            MARKER.to_owned(),
            Entry { object, mode: "100644".into(), permissions },
        )]),
        directories: BTreeMap::new(),
        root_permissions: file_metadata::permission_fingerprint(
            &fs::metadata(root).map_err(failure)?,
        ),
        git_permissions: 0,
    };
    validate(&manifest)?;
    metadata::persist(store, &manifest)
}

pub(super) fn validate(manifest: &Manifest) -> Result<(), ProductRunnerError> {
    if let Some(directory) = &manifest.linked
        && (!directory.is_absolute()
            || directory.to_str().is_none_or(|path| path.contains(['\n', '\r']))
            || !manifest.directories.is_empty()
            || manifest.entries.len() != 1
            || manifest.entries.get(MARKER).is_none_or(|entry| entry.mode != "100644"))
    {
        return Err(failure("invalid retained linked repository metadata"));
    }
    Ok(())
}

pub(super) fn initialize(destination: &Path, directory: &Path) -> Result<(), ProductRunnerError> {
    for ancestor in directory.ancestors() {
        if !fs::symlink_metadata(ancestor).map_err(failure)?.is_dir() {
            return Err(failure("linked repository database was replaced; no restore started"));
        }
    }
    let path = directory.to_str().ok_or_else(|| failure("linked Git directory is not UTF-8"))?;
    fs::write(destination.join(".git"), format!("gitdir: {path}\n")).map_err(failure)?;
    fs::File::open(destination.join(".git")).and_then(|file| file.sync_all()).map_err(failure)
}
