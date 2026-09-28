//! Validate restore paths and remove only task-owned structural replacements.

use super::{ManagedBaseline, failure, validate_path};
use crate::ProductRunnerError;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn parent_is_directory(root: &Path, relative: &Path) -> Result<bool, ProductRunnerError> {
    let mut parent = root.to_path_buf();
    let components = relative.components().collect::<Vec<_>>();
    for component in components.iter().take(components.len().saturating_sub(1)) {
        parent.push(component);
        match fs::symlink_metadata(&parent) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Ok(false);
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(failure(error)),
        }
    }
    Ok(true)
}

impl ManagedBaseline {
    pub(super) fn validate_restore_paths(
        &self,
        root: &Path,
        paths: &[PathBuf],
    ) -> Result<(), ProductRunnerError> {
        let new_repositories = self.new_repositories(root, paths)?;
        for path in paths {
            validate_path(path)?;
            if new_repositories.iter().any(|prefix| path.starts_with(prefix))
                || self.nested.keys().any(|prefix| path.starts_with(prefix))
            {
                continue;
            }
            let mut parent = path.parent();
            while let Some(relative) = parent.filter(|path| !path.as_os_str().is_empty()) {
                if let Ok(metadata) = fs::symlink_metadata(root.join(relative))
                    && (!metadata.is_dir() || metadata.file_type().is_symlink())
                    && (!paths.iter().any(|path| path == relative)
                        || self.entries.contains_key(&relative.to_string_lossy().into_owned()))
                {
                    return Err(failure("restore path has an unowned non-directory parent"));
                }
                parent = relative.parent();
            }
            if !parent_is_directory(root, path)? {
                continue;
            }
            if fs::symlink_metadata(root.join(path)).is_ok_and(|metadata| metadata.is_dir())
                && !self.nested.contains_key(&path.to_string_lossy().into_owned())
            {
                validate_directory(root, path, paths)?;
            }
        }
        Ok(())
    }

    pub(super) fn prepare_restore_paths(
        &self,
        root: &Path,
        paths: &[PathBuf],
    ) -> Result<(), ProductRunnerError> {
        self.validate_restore_paths(root, paths)?;
        let mut deepest = paths.to_vec();
        deepest.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
        for path in deepest {
            if !parent_is_directory(root, &path)? {
                continue;
            }
            let name = path.to_string_lossy();
            if self.nested.keys().any(|prefix| path.starts_with(prefix)) {
                continue;
            }
            let absolute = root.join(&path);
            match fs::symlink_metadata(&absolute) {
                Ok(metadata) if metadata.is_dir() => remove_empty_directories(&absolute)?,
                Ok(_) if !self.entries.contains_key(name.as_ref()) => {
                    fs::remove_file(&absolute).map_err(failure)?;
                    super::recovery::sync_directory(
                        absolute.parent().ok_or_else(|| failure("restore path has no parent"))?,
                    )?;
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(failure(error)),
            }
        }
        Ok(())
    }
}

fn validate_directory(
    root: &Path,
    relative: &Path,
    paths: &[PathBuf],
) -> Result<(), ProductRunnerError> {
    for entry in fs::read_dir(root.join(relative)).map_err(failure)? {
        let entry = entry.map_err(failure)?;
        let path = relative.join(entry.file_name());
        if entry.file_type().map_err(failure)?.is_dir() {
            validate_directory(root, &path, paths)?;
        } else if !paths.contains(&path) {
            return Err(failure(format!(
                "{} contains an unowned file; no restore started",
                relative.display()
            )));
        }
    }
    Ok(())
}

fn remove_empty_directories(path: &Path) -> Result<(), ProductRunnerError> {
    for entry in fs::read_dir(path).map_err(failure)? {
        let entry = entry.map_err(failure)?;
        if !entry.file_type().map_err(failure)?.is_dir() {
            return Err(failure("restore directory still contains a file"));
        }
        remove_empty_directories(&entry.path())?;
    }
    fs::remove_dir(path).map_err(failure)?;
    super::recovery::sync_directory(
        path.parent().ok_or_else(|| failure("restore directory has no parent"))?,
    )
}
