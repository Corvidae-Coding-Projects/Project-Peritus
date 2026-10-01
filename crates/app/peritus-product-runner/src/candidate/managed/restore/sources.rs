//! Materialize all source preimages before source, index, HEAD, or archive publication.

use super::super::transaction::{Journal, Kind, own_directory};
use super::super::{Entry, ManagedBaseline, failure, git, recovery, retention::Replacements};
use crate::ProductRunnerError;
use std::{
    collections::BTreeMap,
    fs,
    io::Write as _,
    path::{Path, PathBuf},
};

pub(super) struct Sources(BTreeMap<PathBuf, tempfile::TempDir>);

impl Sources {
    pub(super) fn prepare(
        baseline: &ManagedBaseline,
        root: &Path,
        paths: &[PathBuf],
        replacements: &Replacements,
        journal: Option<&mut Journal>,
    ) -> Result<Self, ProductRunnerError> {
        let mut sources = Self(BTreeMap::new());
        sources.prepare_repository(baseline, root, paths, replacements, journal)?;
        Ok(sources)
    }

    fn prepare_repository(
        &mut self,
        baseline: &ManagedBaseline,
        root: &Path,
        paths: &[PathBuf],
        replacements: &Replacements,
        mut journal: Option<&mut Journal>,
    ) -> Result<(), ProductRunnerError> {
        for path in paths {
            if baseline.nested.keys().any(|prefix| path.starts_with(prefix)) {
                continue;
            }
            let name = super::super::git_path::tree_name(path)?;
            if let Some(entry) = baseline.entries.get(&name)
                && entry.mode != "160000"
            {
                let bytes = git(root, &["cat-file", "blob", &entry.object], None)?;
                let parent = staging_parent(root, path, paths)?;
                let directory = prepare_source(&parent, entry, &bytes, journal.as_deref_mut())?;
                self.0.insert(root.join(path), directory);
            }
        }
        for (prefix, child) in &baseline.nested {
            if !paths.iter().any(|path| path.starts_with(prefix)) {
                continue;
            }
            let child_path = root.join(prefix);
            let child_root = replacements.root(&child_path);
            let children = paths
                .iter()
                .filter_map(|path| path.strip_prefix(prefix).ok())
                .filter(|path| !path.as_os_str().is_empty())
                .map(Path::to_path_buf)
                .collect();
            let children = replacements.children(child_root, child, children)?;
            self.prepare_repository(
                child,
                child_root,
                &children,
                replacements,
                journal.as_deref_mut(),
            )?;
        }
        Ok(())
    }

    pub(super) fn publish(
        &mut self,
        path: &Path,
        journal: Option<&mut Journal>,
    ) -> Result<(), ProductRunnerError> {
        let directory = self.0.remove(path).ok_or_else(|| failure("source was not prepared"))?;
        let parent = path.parent().ok_or_else(|| failure("restore has no parent"))?;
        create_parents(parent)?;
        super::publish_file(&directory.path().join("source"), path)?;
        recovery::sync_directory(parent)?;
        if let Some(journal) = journal {
            journal.cleanup_directory(directory.path())?;
        }
        #[cfg(test)]
        super::super::transaction::fault::pause(super::super::transaction::fault::Stage::Source);
        Ok(())
    }
}

fn prepare_source(
    parent: &Path,
    entry: &Entry,
    bytes: &[u8],
    mut journal: Option<&mut Journal>,
) -> Result<tempfile::TempDir, ProductRunnerError> {
    let mut directory =
        tempfile::Builder::new().prefix(".peritus-restore-").tempdir_in(parent).map_err(failure)?;
    own_directory(directory.path(), Kind::Source, journal.as_deref_mut())?;
    // The journal owns cleanup once a nonce is retained. TempDir's recursive Drop
    // would otherwise remove foreign entries added during an interrupted operation.
    directory.disable_cleanup(journal.is_some());
    let source = directory.path().join("source");
    if entry.mode == "120000" {
        super::create_link(&source, bytes)?;
    } else {
        let mut file = fs::File::create(&source).map_err(failure)?;
        file.write_all(bytes).map_err(failure)?;
        super::set_permissions(&source, entry.permissions)?;
        file.sync_all().map_err(failure)?;
    }
    recovery::sync_directory(directory.path())?;
    if let Some(journal) = journal {
        journal.seal_directory(directory.path())?;
    }
    Ok(directory)
}

/// Stay on the destination filesystem, outside every path that discard may remove.
/// Walk from the trusted root so no intermediate source symlink is followed.
fn staging_parent(
    root: &Path,
    path: &Path,
    paths: &[PathBuf],
) -> Result<PathBuf, ProductRunnerError> {
    let mut relative = PathBuf::new();
    let mut parent = root.to_path_buf();
    for component in path.parent().ok_or_else(|| failure("restore has no parent"))?.components() {
        relative.push(component);
        match fs::symlink_metadata(root.join(&relative)) {
            Ok(metadata) if metadata.is_dir() => {
                if !paths.iter().any(|path| relative.starts_with(path)) {
                    parent = root.join(&relative);
                }
            }
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(failure(error)),
        }
    }
    Ok(parent)
}

fn create_parents(path: &Path) -> Result<(), ProductRunnerError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(failure("restore parent is no longer a directory")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            create_parents(path.parent().ok_or_else(|| failure("restore has no parent"))?)?;
            recovery::create_directory(path)
        }
        Err(error) => Err(failure(error)),
    }
}
