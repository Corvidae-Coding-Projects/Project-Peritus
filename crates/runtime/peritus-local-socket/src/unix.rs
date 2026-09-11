//! Owner-checked custody of the private runtime directory used by an oversized endpoint.

use std::{
    io,
    path::{Path, PathBuf},
};

use crate::{NATIVE_MAX_PATH_BYTES, bounded_path};

mod directory;
#[cfg(test)]
mod tests;

/// One usable socket path and, when needed, an owned private runtime directory.
///
/// The listener owner must remove its exact socket before dropping this value. Drop removes an
/// empty runtime directory only if its device/inode still match the directory validated here.
#[derive(Debug)]
pub struct PreparedSocketPath {
    path: PathBuf,
    _directory: Option<directory::RuntimeDirectory>,
}

impl PreparedSocketPath {
    /// Prepares a socket location for the already-validated owner of a protected state root.
    ///
    /// An oversized address uses a mode-0700 directory below the system's root-owned temporary
    /// directory. Existing directories must have the expected owner and mode and must not be
    /// symlinks. They are never repaired by chmod or replaced speculatively.
    ///
    /// # Errors
    /// Returns an input, ownership, permission, or filesystem error without using another path.
    pub fn prepare(original: &Path, owner_uid: u32) -> io::Result<Self> {
        let path = bounded_path(original, NATIVE_MAX_PATH_BYTES)?;
        let directory = if path == original {
            None
        } else {
            let parent =
                path.parent().ok_or_else(|| io::Error::other("runtime endpoint has no parent"))?;
            Some(directory::RuntimeDirectory::prepare(parent, owner_uid)?)
        };
        Ok(Self { path, _directory: directory })
    }

    /// Borrows the real path accepted by standard Unix socket APIs.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}
