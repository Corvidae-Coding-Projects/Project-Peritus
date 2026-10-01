//! Reconstruct deleted original repositories privately before discard touches current source.

use super::{ManagedBaseline, Store, failure, metadata, owner};
use crate::ProductRunnerError;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub(in super::super) struct Replacements {
    workspace: PathBuf,
    owner: PathBuf,
    staged: BTreeMap<PathBuf, Replacement>,
    preserve_preparations: bool,
}

struct Replacement {
    directory: tempfile::TempDir,
    existing: bool,
    links: Vec<RestoredLink>,
}

struct RestoredLink {
    root: PathBuf,
    entry: super::Entry,
    length: usize,
}

impl Replacements {
    pub(in super::super) fn prepare(
        baseline: &ManagedBaseline,
        root: &Path,
        paths: &[PathBuf],
        preserve_preparations: bool,
    ) -> Result<Self, ProductRunnerError> {
        let owner = owner(root)?;
        let mut replacements = Self {
            workspace: root.to_path_buf(),
            owner: owner.clone(),
            staged: BTreeMap::new(),
            preserve_preparations,
        };
        replacements.prepare_nested(baseline, root, paths, &owner)?;
        Ok(replacements)
    }

    fn prepare_nested(
        &mut self,
        baseline: &ManagedBaseline,
        root: &Path,
        paths: &[PathBuf],
        owner: &Path,
    ) -> Result<(), ProductRunnerError> {
        for (prefix, child) in &baseline.nested {
            if !paths.iter().any(|path| path.starts_with(prefix)) {
                continue;
            }
            let destination = root.join(prefix);
            if !super::super::paths::parent_is_directory(root, Path::new(prefix))? {
                return Err(failure("nested repository has a replaced parent; no restore started"));
            }
            match fs::symlink_metadata(&destination) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    self.stage(owner, destination, child, false)?;
                }
                Ok(metadata) if metadata.is_dir() => {
                    match fs::symlink_metadata(destination.join(".git")) {
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                            self.stage(owner, destination, child, true)?;
                            continue;
                        }
                        Err(error) => return Err(failure(error)),
                        Ok(_) => {}
                    }
                    let children = paths
                        .iter()
                        .filter_map(|path| path.strip_prefix(prefix).ok())
                        .filter(|path| !path.as_os_str().is_empty())
                        .map(Path::to_path_buf)
                        .collect::<Vec<_>>();
                    self.prepare_nested(child, &destination, &children, owner)?;
                }
                Ok(_) => return Err(failure("nested repository was replaced; no restore started")),
                Err(error) => return Err(failure(error)),
            }
        }
        Ok(())
    }

    fn stage(
        &mut self,
        owner: &Path,
        destination: PathBuf,
        child: &ManagedBaseline,
        existing: bool,
    ) -> Result<(), ProductRunnerError> {
        let staging = owner.join("peritus");
        super::super::recovery::create_directory(&staging)?;
        let staging = staging.join("restoring");
        super::super::recovery::create_directory(&staging)?;
        verify_mount(&staging, &destination)?;
        let mut directory =
            tempfile::Builder::new().prefix("repository-").tempdir_in(&staging).map_err(failure)?;
        // Incomplete reconstruction can contain unrecognized state after a crash.
        // Retain it beside the independent backup instead of recursively deleting it.
        directory.disable_cleanup(self.preserve_preparations);
        let mut links = Vec::new();
        let original = self
            .workspace
            .canonicalize()
            .map_err(failure)?
            .join(destination.strip_prefix(&self.workspace).map_err(failure)?);
        restore_tree(owner, directory.path(), child, &mut links, (&original, directory.path()))?;
        self.staged.insert(destination, Replacement { directory, existing, links });
        Ok(())
    }

    pub(in super::super) fn root<'a>(&'a self, root: &'a Path) -> &'a Path {
        self.staged.get(root).map_or(root, |replacement| replacement.directory.path())
    }

    pub(in super::super) fn restored_root(
        &self,
        root: &Path,
    ) -> Result<PathBuf, ProductRunnerError> {
        for (destination, replacement) in &self.staged {
            if let Ok(relative) = root.strip_prefix(replacement.directory.path()) {
                let destination = destination.strip_prefix(&self.workspace).map_err(failure)?;
                let mut restored =
                    self.workspace.canonicalize().map_err(failure)?.join(destination);
                restored.extend(relative.components());
                return Ok(restored);
            }
        }
        root.canonicalize().map_err(failure)
    }

    pub(in super::super) fn children(
        &self,
        root: &Path,
        baseline: &ManagedBaseline,
        paths: Vec<PathBuf>,
    ) -> Result<Vec<PathBuf>, ProductRunnerError> {
        if !self.staged.values().any(|replacement| root.starts_with(replacement.directory.path())) {
            return Ok(paths);
        }
        let mut paths = baseline
            .entries
            .keys()
            .chain(baseline.nested.keys())
            .map(PathBuf::from)
            .collect::<std::collections::BTreeSet<_>>();
        for path in super::git(root, &["ls-files", "--cached", "-z"], None)?
            .split(|byte| *byte == 0)
            .filter(|path| !path.is_empty())
        {
            let path = PathBuf::from(std::str::from_utf8(path).map_err(failure)?);
            super::super::validate_path(&path)?;
            paths.insert(path);
        }
        Ok(paths.into_iter().collect())
    }

    pub(in super::super) fn publish(
        self,
        mut journal: Option<&mut super::super::transaction::Journal>,
    ) -> Result<Vec<PathBuf>, ProductRunnerError> {
        let mut recovered = Vec::new();
        for (destination, replacement) in self.staged {
            for link in &replacement.links {
                let store = Store::existing(&self.owner, link.length)?;
                metadata::write_entry(&store, &link.root, Path::new(".git"), &link.entry)?;
            }
            let relative = destination.strip_prefix(&self.workspace).map_err(failure)?;
            if !super::super::paths::parent_is_directory(&self.workspace, relative)? {
                return Err(failure(
                    "repository parent changed during discard; retained backup is intact",
                ));
            }
            match fs::symlink_metadata(&destination) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Ok(metadata) if metadata.is_dir() && replacement.existing => {
                    match fs::symlink_metadata(destination.join(".git")) {
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                        Err(error) => return Err(failure(error)),
                        Ok(_) => {
                            return Err(failure(
                                "Git metadata was recreated during discard; current directory was preserved",
                            ));
                        }
                    }
                    recovered.push(super::super::recovery::archive_observed(
                        &self.workspace,
                        relative,
                        journal.as_deref_mut(),
                    )?);
                }
                Ok(_) => {
                    return Err(failure(
                        "repository was recreated during discard; current directory was preserved",
                    ));
                }
                Err(error) => return Err(failure(error)),
            }
            let parent = destination.parent().ok_or_else(|| failure("repository has no parent"))?;
            fs::create_dir_all(parent).map_err(failure)?;
            let prepared = replacement.directory.keep();
            fs::rename(&prepared, &destination).map_err(|error| {
                failure(format!(
                    "cannot publish retained repository from {} to {}: {error}",
                    prepared.display(),
                    destination.display()
                ))
            })?;
            super::super::recovery::sync_directory(parent)?;
        }
        Ok(recovered)
    }
}

#[allow(
    clippy::missing_const_for_fn,
    clippy::unnecessary_wraps,
    reason = "shared fallible preflight interface; Unix device metadata is platform-specific"
)]
fn verify_mount(staging: &Path, destination: &Path) -> Result<(), ProductRunnerError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        let mut parent = destination.parent();
        while let Some(path) = parent {
            match fs::symlink_metadata(path) {
                Ok(metadata) => {
                    if metadata.dev() != fs::metadata(staging).map_err(failure)?.dev() {
                        return Err(failure(
                            "repository recovery crosses a filesystem boundary; no restore started",
                        ));
                    }
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    parent = path.parent();
                }
                Err(error) => return Err(failure(error)),
            }
        }
    }
    #[cfg(not(unix))]
    let _ = (staging, destination);
    Ok(())
}

fn restore_tree(
    owner: &Path,
    destination: &Path,
    baseline: &ManagedBaseline,
    links: &mut Vec<RestoredLink>,
    location: (&Path, &Path),
) -> Result<(), ProductRunnerError> {
    let store = Store::existing(owner, baseline.tree.len())?;
    let manifest = store.load(baseline)?;
    if let Some(directory) = &manifest.linked {
        let directory = directory
            .strip_prefix(location.0)
            .map_or_else(|_| directory.clone(), |relative| location.1.join(relative));
        super::linked::initialize(destination, &directory)?;
    } else {
        store.initialize(destination, &manifest)?;
    }
    metadata::materialize(&store, destination, &manifest)?;
    if manifest.linked.is_some() {
        let entry = manifest
            .entries
            .get(super::linked::MARKER)
            .ok_or_else(|| failure("linked repository marker is missing"))?
            .clone();
        links.push(RestoredLink {
            root: destination.to_path_buf(),
            entry,
            length: baseline.tree.len(),
        });
    }
    for (path, entry) in &baseline.entries {
        if entry.mode != "160000" {
            metadata::write_entry(&store, destination, Path::new(path), entry)?;
        }
    }
    for (path, child) in &baseline.nested {
        if !super::super::paths::parent_is_directory(destination, Path::new(path))? {
            return Err(failure("retained repository has a non-directory parent"));
        }
        let directory = destination.join(path);
        if fs::symlink_metadata(&directory).is_ok_and(|metadata| !metadata.is_dir()) {
            return Err(failure("retained repository would overwrite a non-directory"));
        }
        fs::create_dir_all(&directory).map_err(failure)?;
        restore_tree(owner, &directory, child, links, location)?;
    }
    super::super::restore::set_permissions(destination, manifest.root_permissions)?;
    super::super::recovery::sync_directory(destination)
}
