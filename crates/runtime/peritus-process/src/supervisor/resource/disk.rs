//! Sample a changing workspace without treating ordinary file deletion as owner failure.

use std::{
    fs::{self, DirEntry},
    io,
    path::{Path, PathBuf},
};

use crate::ProcessError;

use super::resource_error;

pub(super) fn disk_usage(root: &Path) -> Result<u64, ProcessError> {
    // Losing the workspace itself is an error, unlike a descendant removed by a build.
    let entries = fs::read_dir(root)
        .map_err(|_| resource_error("workspace disk usage cannot be observed"))?;
    let mut pending = Vec::new();
    let total = entries_usage(entries, &mut pending)?;
    descendants_usage(pending, total)
}

fn descendants_usage(mut pending: Vec<PathBuf>, mut total: u64) -> Result<u64, ProcessError> {
    while let Some(directory) = pending.pop() {
        let entries = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(_) => return Err(resource_error("workspace disk usage cannot be observed")),
        };
        total = total.saturating_add(entries_usage(entries, &mut pending)?);
    }
    Ok(total)
}

fn entries_usage(
    entries: impl IntoIterator<Item = io::Result<DirEntry>>,
    pending: &mut Vec<PathBuf>,
) -> Result<u64, ProcessError> {
    let mut total = 0_u64;
    for entry in entries {
        let entry = entry.map_err(|_| resource_error("workspace entry cannot be observed"))?;
        let metadata = match fs::symlink_metadata(entry.path()) {
            Ok(metadata) => metadata,
            // Directory enumeration and metadata lookup are separate observations. Removed
            // files consume no bytes in this sample; other observation errors remain fatal.
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(_) => return Err(resource_error("workspace metadata cannot be observed")),
        };
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            pending.push(entry.path());
        } else if metadata.is_file() {
            total = total.saturating_add(metadata.len());
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests;
