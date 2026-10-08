//! Atomic operation publication and directory synchronization.
use super::{Result, problem};
use crate::error::uncertain;
use std::{fs::File, io::Write, path::Path};

pub(super) fn record_names(directory: &Path) -> Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            return Err(problem("The operation store contains an unexpected directory entry"));
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| problem("The operation store contains a non-UTF-8 record name"))?;
        let Some(digest) = name.strip_suffix(".json") else {
            return Err(problem("The operation store contains an unexpected record name"));
        };
        if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(problem("The operation store contains an invalid record identity"));
        }
        names.push(digest.to_owned());
    }
    names.sort_unstable();
    Ok(names)
}

pub(super) fn create_directories(root: &Path) -> Result<()> {
    std::fs::create_dir_all(root)?;
    for name in ["pending", "settled", "locks", "tmp"] {
        std::fs::create_dir_all(root.join(name))?;
    }
    sync_directory(root)?;
    if let Some(parent) = root.parent() {
        sync_directory(parent)?;
        if let Some(grandparent) = parent.parent() {
            sync_directory(grandparent)?;
        }
    }
    Ok(())
}

pub(super) fn publish_noclobber(root: &Path, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut temporary = tempfile::NamedTempFile::new_in(root.join("tmp"))?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    match temporary.persist_noclobber(path) {
        Ok(_) => {
            sync_directory_io(path.parent().expect("operation record parent"))?;
            Ok(())
        }
        Err(error) => Err(error.error),
    }
}

pub(super) fn publish_replace(root: &Path, path: &Path, bytes: &[u8]) -> Result<()> {
    let mut temporary = tempfile::NamedTempFile::new_in(root.join("tmp"))?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(problem)?;
    sync_directory(path.parent().expect("operation record parent"))
}

pub(super) fn sync_directory(path: &Path) -> Result<()> {
    sync_directory_io(path).map_err(uncertain)
}

#[cfg(unix)]
fn sync_directory_io(path: &Path) -> std::io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory_io(_path: &Path) -> std::io::Result<()> {
    Ok(())
}
