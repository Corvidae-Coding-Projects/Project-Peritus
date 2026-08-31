//! Canonical filesystem identity checks for native controller resources.

use std::fs;
use std::path::{Path, PathBuf};

use super::NativeAdapterError;

pub(super) fn canonical_file(
    path: &Path,
    label: &'static str,
) -> Result<PathBuf, NativeAdapterError> {
    let canonical = fs::canonicalize(path)
        .map_err(|error| NativeAdapterError::filesystem("canonicalize path", path, error))?;
    if !fs::metadata(&canonical).is_ok_and(|metadata| metadata.is_file()) {
        return Err(NativeAdapterError::PathType {
            label,
            expected: "a regular file",
            path: canonical,
        });
    }
    Ok(canonical)
}

pub(super) fn canonical_directory(
    path: &Path,
    label: &'static str,
) -> Result<PathBuf, NativeAdapterError> {
    let canonical = fs::canonicalize(path)
        .map_err(|error| NativeAdapterError::filesystem("canonicalize path", path, error))?;
    if !fs::metadata(&canonical).is_ok_and(|metadata| metadata.is_dir()) {
        return Err(NativeAdapterError::PathType {
            label,
            expected: "an existing directory",
            path: canonical,
        });
    }
    Ok(canonical)
}
