//! Preserve original Git metadata as immutable blobs, without following metadata symlinks.

use super::{Entry, ManagedBaseline, Manifest, Store, failure, git, text};
use crate::{ProductRunnerError, file_metadata};
use std::{collections::BTreeMap, fs, path::Path};

pub(super) fn capture(
    store: &Store,
    root: &Path,
    baseline: &ManagedBaseline,
) -> Result<String, ProductRunnerError> {
    let roots = store.fetch_repository(root)?;
    let directory = root.join(".git");
    let mut manifest = Manifest {
        linked: None,
        source_tree: baseline.tree.clone(),
        source_index: baseline.index.clone(),
        roots,
        entries: BTreeMap::new(),
        directories: BTreeMap::new(),
        root_permissions: file_metadata::permission_fingerprint(
            &fs::metadata(root).map_err(failure)?,
        ),
        git_permissions: file_metadata::permission_fingerprint(
            &fs::metadata(&directory).map_err(failure)?,
        ),
    };
    capture_directory(store, &directory, Path::new(""), &mut manifest)?;
    persist(store, &manifest)
}

pub(super) fn persist(store: &Store, manifest: &Manifest) -> Result<String, ProductRunnerError> {
    let tree = super::super::capture::build_tree(&store.root, &manifest.entries)?;
    store.pin(&tree)?;
    let object = text(git(
        &store.root,
        &["-c", "core.fsync=all", "hash-object", "-w", "--stdin"],
        Some(&serde_json::to_vec(manifest).map_err(failure)?),
    )?)?;
    store.pin(&object)?;
    Ok(object)
}

fn capture_directory(
    store: &Store,
    root: &Path,
    relative: &Path,
    manifest: &mut Manifest,
) -> Result<(), ProductRunnerError> {
    for entry in fs::read_dir(root.join(relative)).map_err(failure)? {
        let entry = entry.map_err(failure)?;
        let path = relative.join(entry.file_name());
        let name = path.to_str().ok_or_else(|| failure("Git metadata path is not UTF-8"))?;
        if path == Path::new("objects") || transient_lock(&path) {
            continue;
        }
        super::validate_metadata_path(name)?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(failure)?;
        let permissions = file_metadata::permission_fingerprint(&metadata);
        if metadata.is_dir() {
            manifest.directories.insert(name.to_owned(), permissions);
            capture_directory(store, root, &path, manifest)?;
            continue;
        }
        let (mode, object) = if metadata.file_type().is_symlink() {
            let target = fs::read_link(entry.path()).map_err(failure)?;
            (
                "120000",
                text(git(
                    &store.root,
                    &["-c", "core.fsync=all", "hash-object", "-w", "--stdin"],
                    Some(target.as_os_str().as_encoded_bytes()),
                )?)?,
            )
        } else if metadata.is_file() {
            let absolute = entry.path();
            let path =
                absolute.to_str().ok_or_else(|| failure("Git metadata path is not UTF-8"))?;
            (
                file_metadata::git_file_mode(&metadata),
                text(git(
                    &store.root,
                    &["-c", "core.fsync=all", "hash-object", "--no-filters", "-w", "--", path],
                    None,
                )?)?,
            )
        } else {
            return Err(failure(format!(
                "cannot retain non-file Git metadata {}",
                entry.path().display()
            )));
        };
        manifest.entries.insert(name.to_owned(), Entry { object, mode: mode.into(), permissions });
    }
    Ok(())
}

fn transient_lock(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name.to_string_lossy().ends_with(".lock"))
        && (path.parent() == Some(Path::new(""))
            || path.starts_with("refs")
            || path.starts_with("reftable"))
}

pub(super) fn materialize(
    store: &Store,
    root: &Path,
    manifest: &Manifest,
) -> Result<(), ProductRunnerError> {
    if manifest.linked.is_some() {
        return Ok(());
    }
    let git_directory = root.join(".git");
    for directory in manifest.directories.keys() {
        checked_parent(&git_directory, Path::new(directory))?;
        fs::create_dir_all(git_directory.join(directory)).map_err(failure)?;
    }
    for (path, entry) in &manifest.entries {
        write_entry(store, &git_directory, Path::new(path), entry)?;
    }
    let mut directories = manifest.directories.iter().collect::<Vec<_>>();
    directories.sort_by_key(|(path, _)| std::cmp::Reverse(Path::new(path).components().count()));
    for (directory, permissions) in directories {
        let directory = git_directory.join(directory);
        super::super::restore::set_permissions(&directory, *permissions)?;
        super::super::recovery::sync_directory(&directory)?;
    }
    super::super::restore::set_permissions(&git_directory, manifest.git_permissions)?;
    super::super::recovery::sync_directory(&git_directory)
}

pub(super) fn write_entry(
    store: &Store,
    root: &Path,
    path: &Path,
    entry: &Entry,
) -> Result<(), ProductRunnerError> {
    checked_parent(root, path)?;
    let destination = root.join(path);
    let parent = destination.parent().ok_or_else(|| failure("retained file has no parent"))?;
    fs::create_dir_all(parent).map_err(failure)?;
    if entry.mode == "120000" {
        let bytes = git(&store.root, &["cat-file", "blob", &entry.object], None)?;
        super::super::restore::restore_link(&destination, &bytes)?;
    } else {
        if fs::symlink_metadata(&destination).is_ok_and(|metadata| !metadata.is_file()) {
            return Err(failure("retained file would overwrite a non-regular target"));
        }
        store.write_blob(&entry.object, &destination)?;
        super::super::restore::set_permissions(&destination, entry.permissions)?;
        fs::File::open(&destination).and_then(|file| file.sync_all()).map_err(failure)?;
    }
    super::super::recovery::sync_directory(parent)
}

fn checked_parent(root: &Path, path: &Path) -> Result<(), ProductRunnerError> {
    if !super::super::paths::parent_is_directory(root, path)? {
        return Err(failure("retained file has a non-directory parent"));
    }
    Ok(())
}
