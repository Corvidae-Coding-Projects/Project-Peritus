//! Handle-relative empty-directory identity, permissions and observed-change rejection.

use super::{
    FolderIdentity, FolderInspection, WorkspaceError, changed, reject_cap_reparse, snapshot_io,
};
use cap_fs_ext::{DirExt as _, MetadataExt as _};
use cap_std::fs::{Dir, Metadata};
use peritus_patch::{DirectoryMode, WorkspacePath};

impl FolderInspection {
    /// Observes empty-directory permissions without following any path component.
    ///
    /// # Errors
    /// Rejects nonempty directories, links, changed identity or permissions, and I/O failures.
    pub fn empty_directory_mode(
        &self,
        path: &WorkspacePath,
    ) -> Result<DirectoryMode, WorkspaceError> {
        let directory = self.open_directory(path)?;
        let before = directory.dir_metadata().map_err(snapshot_io)?;
        if directory
            .entries()
            .map_err(snapshot_io)?
            .next()
            .transpose()
            .map_err(snapshot_io)?
            .is_some()
        {
            return Err(changed());
        }
        let after = directory.dir_metadata().map_err(snapshot_io)?;
        let reopened = self.open_directory(path)?.dir_metadata().map_err(snapshot_io)?;
        if !same_directory(&before, &after)?
            || !same_directory(&before, &reopened)?
            || FolderIdentity::observe(self.identity.root()).map_err(snapshot_io)? != self.identity
        {
            return Err(changed());
        }
        Ok(directory_mode(&after))
    }

    fn open_directory(&self, path: &WorkspacePath) -> Result<Dir, WorkspaceError> {
        let mut directory = self.root.try_clone().map_err(snapshot_io)?;
        for component in path.as_str().split('/') {
            directory = directory.open_dir_nofollow(component).map_err(snapshot_io)?;
            reject_cap_reparse(&directory.dir_metadata().map_err(snapshot_io)?)?;
        }
        Ok(directory)
    }
}

fn same_directory(before: &Metadata, after: &Metadata) -> Result<bool, WorkspaceError> {
    let same = before.dev() == after.dev()
        && before.ino() == after.ino()
        && before.modified().map_err(snapshot_io)? == after.modified().map_err(snapshot_io)?
        && directory_mode(before) == directory_mode(after);
    #[cfg(unix)]
    let same = {
        use cap_std::fs::MetadataExt as _;
        same && before.ctime() == after.ctime() && before.ctime_nsec() == after.ctime_nsec()
    };
    Ok(same)
}

#[cfg(unix)]
fn directory_mode(metadata: &Metadata) -> DirectoryMode {
    use cap_std::fs::PermissionsExt as _;
    DirectoryMode::new((metadata.permissions().mode() & 0o7777) as u16)
        .expect("masked permission bits")
}

#[cfg(not(unix))]
fn directory_mode(metadata: &Metadata) -> DirectoryMode {
    DirectoryMode::new(if metadata.permissions().readonly() { 0o555 } else { 0o777 })
        .expect("portable permission bits")
}
