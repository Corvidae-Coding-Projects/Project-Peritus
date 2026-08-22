//! Guarded filesystem operations for caller-owned test roots.

use super::{FixtureSymlinkKind, TempRepositoryError, TempRepositoryErrorKind};
use crate::FixturePath;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;

const OWNER_MARKER: &str = ".peritus-test-support-owner";
const OWNER_MARKER_BYTES: &[u8] = b"peritus-test-support-v1\n";
const ROOT_PREFIX: &str = "peritus-test-";

pub(super) fn validate_new_root(root: &Path) -> Result<(), TempRepositoryError> {
    let Some(name) = root.file_name().and_then(OsStr::to_str) else {
        return Err(TempRepositoryError::at(
            TempRepositoryErrorKind::InvalidRoot,
            root,
            "owned root requires a UTF-8 final component",
        ));
    };
    if !name.starts_with(ROOT_PREFIX) || name.len() == ROOT_PREFIX.len() || root.exists() {
        return Err(TempRepositoryError::at(
            TempRepositoryErrorKind::InvalidRoot,
            root,
            "owned root must not exist and must have a specific peritus-test-* name",
        ));
    }
    let parent = root.parent().ok_or_else(|| {
        TempRepositoryError::at(
            TempRepositoryErrorKind::InvalidRoot,
            root,
            "owned root requires a parent",
        )
    })?;
    let metadata = fs::symlink_metadata(parent).map_err(|source| {
        TempRepositoryError::sourced(
            TempRepositoryErrorKind::InvalidRoot,
            parent,
            "owned-root parent could not be inspected",
            source,
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(TempRepositoryError::at(
            TempRepositoryErrorKind::InvalidRoot,
            parent,
            "owned-root parent must be a real directory",
        ));
    }
    Ok(())
}

pub(super) fn write_owner_marker(root: &Path) -> std::io::Result<()> {
    fs::write(root.join(OWNER_MARKER), OWNER_MARKER_BYTES)
}

pub(super) fn create_safe_directories(
    root: &Path,
    path: &FixturePath,
) -> Result<(), TempRepositoryError> {
    let mut current = root.to_path_buf();
    for segment in path.as_str().split('/') {
        current.push(segment);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(TempRepositoryError::at(
                    TempRepositoryErrorKind::UnsafePath,
                    current,
                    "path ancestor was a symlink or non-directory",
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&current).map_err(|source| {
                    TempRepositoryError::sourced(
                        TempRepositoryErrorKind::Io,
                        &current,
                        "could not create contained directory",
                        source,
                    )
                })?;
            }
            Err(source) => {
                return Err(TempRepositoryError::sourced(
                    TempRepositoryErrorKind::Io,
                    current,
                    "could not inspect contained directory",
                    source,
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn reject_existing_symlink(path: &Path) -> Result<(), TempRepositoryError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(TempRepositoryError::at(
                TempRepositoryErrorKind::UnsafePath,
                path,
                "write target was a symlink or non-file",
            ))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(TempRepositoryError::sourced(
            TempRepositoryErrorKind::Io,
            path,
            "could not inspect write target",
            source,
        )),
    }
}

pub(super) fn reject_existing_path(path: &Path) -> Result<(), TempRepositoryError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(TempRepositoryError::at(
            TempRepositoryErrorKind::UnsafePath,
            path,
            "symlink fixture target already exists",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(TempRepositoryError::sourced(
            TempRepositoryErrorKind::Io,
            path,
            "could not inspect symlink target",
            source,
        )),
    }
}

pub(super) fn guarded_cleanup(root: &Path) -> Result<(), TempRepositoryError> {
    let name_ok = root
        .file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|name| name.starts_with(ROOT_PREFIX) && name.len() > ROOT_PREFIX.len());
    let metadata = fs::symlink_metadata(root).map_err(|source| {
        TempRepositoryError::sourced(
            TempRepositoryErrorKind::Cleanup,
            root,
            "could not inspect owned root for cleanup",
            source,
        )
    })?;
    let marker = root.join(OWNER_MARKER);
    let marker_ok = fs::symlink_metadata(&marker).is_ok_and(|marker_metadata| {
        marker_metadata.is_file() && !marker_metadata.file_type().is_symlink()
    }) && fs::read(&marker).is_ok_and(|bytes| bytes == OWNER_MARKER_BYTES);
    if !name_ok || metadata.file_type().is_symlink() || !metadata.is_dir() || !marker_ok {
        return Err(TempRepositoryError::at(
            TempRepositoryErrorKind::Cleanup,
            root,
            "guarded cleanup could not prove exact test-root ownership",
        ));
    }
    fs::remove_dir_all(root).map_err(|source| {
        TempRepositoryError::sourced(
            TempRepositoryErrorKind::Cleanup,
            root,
            "could not remove owned test root",
            source,
        )
    })
}

pub(super) fn guarded_cleanup_partial(root: &Path) -> Result<(), TempRepositoryError> {
    let name_ok = root
        .file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|name| name.starts_with(ROOT_PREFIX) && name.len() > ROOT_PREFIX.len());
    let metadata = fs::symlink_metadata(root).map_err(|source| {
        TempRepositoryError::sourced(
            TempRepositoryErrorKind::Cleanup,
            root,
            "could not inspect partial owned root",
            source,
        )
    })?;
    if !name_ok || metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(TempRepositoryError::at(
            TempRepositoryErrorKind::Cleanup,
            root,
            "partial cleanup could not prove a narrow test root",
        ));
    }
    let allowed =
        [OWNER_MARKER, "repository", "disabled-hooks", "isolated-gitconfig", "process-temp"];
    for entry in fs::read_dir(root).map_err(|source| {
        TempRepositoryError::sourced(
            TempRepositoryErrorKind::Cleanup,
            root,
            "could not enumerate partial owned root",
            source,
        )
    })? {
        let path = entry
            .map_err(|source| {
                TempRepositoryError::sourced(
                    TempRepositoryErrorKind::Cleanup,
                    root,
                    "could not inspect partial owned entry",
                    source,
                )
            })?
            .path();
        let allowed_name =
            path.file_name().and_then(OsStr::to_str).is_some_and(|name| allowed.contains(&name));
        if !allowed_name {
            return Err(TempRepositoryError::at(
                TempRepositoryErrorKind::Cleanup,
                path,
                "partial owned root contained an unrecognized entry",
            ));
        }
    }
    fs::remove_dir_all(root).map_err(|source| {
        TempRepositoryError::sourced(
            TempRepositoryErrorKind::Cleanup,
            root,
            "could not remove partial owned root",
            source,
        )
    })
}

#[cfg(unix)]
pub(super) fn create_symlink(
    target: &Path,
    link: &Path,
    _kind: FixtureSymlinkKind,
) -> Result<(), TempRepositoryError> {
    std::os::unix::fs::symlink(target, link).map_err(|source| {
        TempRepositoryError::sourced(
            TempRepositoryErrorKind::Io,
            link,
            "could not create adversarial symlink",
            source,
        )
    })
}

#[cfg(windows)]
pub(super) fn create_symlink(
    target: &Path,
    link: &Path,
    kind: FixtureSymlinkKind,
) -> Result<(), TempRepositoryError> {
    let result = match kind {
        FixtureSymlinkKind::File => std::os::windows::fs::symlink_file(target, link),
        FixtureSymlinkKind::Directory => std::os::windows::fs::symlink_dir(target, link),
    };
    result.map_err(|source| {
        TempRepositoryError::sourced(
            TempRepositoryErrorKind::Io,
            link,
            "could not create adversarial symlink",
            source,
        )
    })
}

#[cfg(not(any(unix, windows)))]
pub(super) fn create_symlink(
    _target: &Path,
    link: &Path,
    _kind: FixtureSymlinkKind,
) -> Result<(), TempRepositoryError> {
    Err(TempRepositoryError::at(
        TempRepositoryErrorKind::SymlinkUnsupported,
        link,
        "symbolic links are unsupported on this platform",
    ))
}
