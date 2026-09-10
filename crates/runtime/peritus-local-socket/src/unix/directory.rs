//! Fail-closed creation and identity-bound cleanup of private runtime directories.

use std::{
    fs, io,
    os::unix::fs::{DirBuilderExt as _, MetadataExt as _},
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub(super) struct RuntimeDirectory {
    path: PathBuf,
    device: u64,
    inode: u64,
}

impl RuntimeDirectory {
    pub(super) fn prepare(path: &Path, owner_uid: u32) -> io::Result<Self> {
        // `/tmp` is a root-controlled alias on macOS. Validate its canonical target rather than
        // rejecting that system symlink, and do not trust an environment-selected temp root.
        let base = fs::canonicalize("/tmp")?;
        let metadata = fs::symlink_metadata(&base)?;
        if !metadata.is_dir()
            || !crate::verified::protected_temporary_root(
                metadata.uid(),
                metadata.mode() & 0o022 != 0,
                metadata.mode() & 0o1000 != 0,
            )
        {
            return Err(unsafe_directory(
                "system temporary directory is not root-owned and protected against replacement",
            ));
        }
        match fs::DirBuilder::new().mode(0o700).create(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.file_type().is_dir()
            || !crate::verified::private_directory(
                metadata.uid(),
                owner_uid,
                metadata.mode() & 0o777,
            )
        {
            return Err(unsafe_directory(
                "Unix runtime directory must be a real mode-0700 directory owned by the state-root owner",
            ));
        }
        Ok(Self { path: path.to_path_buf(), device: metadata.dev(), inode: metadata.ino() })
    }
}

impl Drop for RuntimeDirectory {
    fn drop(&mut self) {
        if let Ok(metadata) = fs::symlink_metadata(&self.path)
            && metadata.file_type().is_dir()
            && metadata.dev() == self.device
            && metadata.ino() == self.inode
        {
            // remove_dir never follows a symlink and refuses any nonempty directory.
            let _ = fs::remove_dir(&self.path);
        }
    }
}

fn unsafe_directory(detail: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, detail)
}
