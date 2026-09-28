//! Restore task preimages without changing unrelated staged or unstaged files.

use super::{ManagedBaseline, failure, git, retention::Replacements};
use crate::ProductRunnerError;
use std::{
    fs,
    path::{Path, PathBuf},
};

mod prepared;
mod sources;
#[cfg(all(test, unix))]
mod tests;

impl ManagedBaseline {
    pub(crate) fn discard(
        &self,
        root: &Path,
        paths: &[PathBuf],
    ) -> Result<Vec<PathBuf>, ProductRunnerError> {
        self.validate()?;
        let replacements = Replacements::prepare(self, root, paths)?;
        self.preflight_restore(root, paths, &replacements)?;
        let mut indexes = prepared::Indexes::prepare(self, root, paths, &replacements)?;
        let mut heads = prepared::Heads::prepare(self, root, paths, &replacements)?;
        let mut sources = sources::Sources::prepare(self, root, paths, &replacements)?;
        let mut recovered = self.discard_prepared(
            root,
            paths,
            &mut indexes,
            &mut heads,
            &mut sources,
            &replacements,
        )?;
        recovered.extend(replacements.publish()?);
        Ok(recovered)
    }

    fn discard_prepared(
        &self,
        root: &Path,
        paths: &[PathBuf],
        indexes: &mut prepared::Indexes,
        heads: &mut prepared::Heads,
        sources: &mut sources::Sources,
        replacements: &Replacements,
    ) -> Result<Vec<PathBuf>, ProductRunnerError> {
        let mut recovered = self.archive_new_repositories(root, paths)?;
        self.prepare_restore_paths(root, paths)?;
        for path in paths {
            if self
                .nested
                .keys()
                .any(|prefix| path != Path::new(prefix) && path.starts_with(prefix))
            {
                continue;
            }
            let name = super::git_path::tree_name(path)?;
            if self.nested.contains_key(&name) {
                continue;
            }
            if self.entries.get(&name).is_some_and(|entry| entry.mode != "160000") {
                sources.publish(&root.join(path))?;
            }
        }
        indexes.publish(root)?;
        for (prefix, child) in &self.nested {
            if !paths.iter().any(|path| path.starts_with(prefix)) {
                continue;
            }
            let children = paths
                .iter()
                .filter_map(|path| path.strip_prefix(prefix).ok())
                .filter(|path| !path.as_os_str().is_empty())
                .map(Path::to_path_buf)
                .collect::<Vec<_>>();
            let child_path = root.join(prefix);
            let child_root = replacements.root(&child_path);
            let children = replacements.children(child_root, child, children)?;
            if !children.is_empty() {
                recovered.extend(child.discard_prepared(
                    child_root,
                    &children,
                    indexes,
                    heads,
                    sources,
                    replacements,
                )?);
            }
            recovered.extend(heads.publish(child_root)?);
        }
        Ok(recovered)
    }

    fn preflight_restore(
        &self,
        root: &Path,
        paths: &[PathBuf],
        replacements: &Replacements,
    ) -> Result<(), ProductRunnerError> {
        self.validate()?;
        for prefix in self.nested.keys() {
            if paths.iter().any(|path| path.starts_with(prefix))
                && (!super::paths::parent_is_directory(root, Path::new(prefix))?
                    || !fs::symlink_metadata(replacements.root(&root.join(prefix)))
                        .is_ok_and(|metadata| metadata.is_dir()))
            {
                return Err(failure(
                    "nested repository was removed or replaced; no restore started",
                ));
            }
        }
        self.validate_restore_paths(root, paths)?;
        for path in paths {
            let name = super::git_path::tree_name(path)?;
            if let Some(entry) = self.entries.get(&name)
                && entry.mode != "160000"
            {
                git(root, &["cat-file", "blob", &entry.object], None)?;
            }
            self.verify_index_objects(root, &name)?;
        }
        for (prefix, child) in &self.nested {
            if !paths.iter().any(|path| path.starts_with(prefix)) {
                continue;
            }
            let child_path = root.join(prefix);
            let child_root = replacements.root(&child_path);
            super::head::preflight(
                child_root,
                self.entries.get(prefix).map(|entry| entry.object.as_str()),
            )?;
            let children = paths
                .iter()
                .filter_map(|path| path.strip_prefix(prefix).ok())
                .filter(|path| !path.as_os_str().is_empty())
                .map(Path::to_path_buf)
                .collect::<Vec<_>>();
            let children = replacements.children(child_root, child, children)?;
            if !children.is_empty() {
                child.preflight_restore(child_root, &children, replacements)?;
            }
        }
        Ok(())
    }
}

#[cfg(unix)]
pub(super) fn set_permissions(path: &Path, bits: u32) -> Result<(), ProductRunnerError> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(bits)).map_err(failure)
}

#[cfg(not(unix))]
pub(super) fn set_permissions(path: &Path, bits: u32) -> Result<(), ProductRunnerError> {
    let mut permissions = fs::metadata(path).map_err(failure)?.permissions();
    permissions.set_readonly(bits != 0);
    fs::set_permissions(path, permissions).map_err(failure)
}

pub(super) fn restore_link(path: &Path, bytes: &[u8]) -> Result<(), ProductRunnerError> {
    let parent = path.parent().ok_or_else(|| failure("restore link has no parent"))?;
    let staging =
        tempfile::Builder::new().prefix("peritus-link-").tempdir_in(parent).map_err(failure)?;
    let prepared = staging.path().join("link");
    create_link(&prepared, bytes)?;
    super::recovery::sync_directory(staging.path())?;
    fs::rename(&prepared, path).map_err(failure)?;
    super::recovery::sync_directory(parent)
}

fn create_link(path: &Path, bytes: &[u8]) -> Result<(), ProductRunnerError> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt as _;
        std::os::unix::fs::symlink(std::ffi::OsStr::from_bytes(bytes), path).map_err(failure)?;
    }
    #[cfg(windows)]
    {
        let target = std::str::from_utf8(bytes).map_err(failure)?;
        std::os::windows::fs::symlink_file(target, path).map_err(failure)?;
    }
    Ok(())
}
