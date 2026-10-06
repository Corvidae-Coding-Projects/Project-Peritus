//! Stream complete source identity and exact selections without total admission ceilings.

use super::{
    FileReadSelection, FolderIdentity, FolderInspection, InspectedFile, InspectedSelection,
    WorkspaceError, changed, invalid, selection::Selection, snapshot_io,
};
use cap_fs_ext::MetadataExt as _;
use cap_std::fs::{File, Metadata};
use peritus_patch::WorkspacePath;
use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};
use std::io::{Read as _, Write};

mod scan;
use scan::Scan;

impl FolderInspection {
    /// Streams a complete snapshot into caller-owned storage using fixed working memory.
    ///
    /// # Errors
    /// Rejects unsafe paths, changed source/root identity and storage or read failures.
    pub fn copy_snapshot(
        &self,
        path: &WorkspacePath,
        output: &mut impl Write,
    ) -> Result<(Sha256Digest, u64), WorkspaceError> {
        let observed = self.copy_selection(path, FileReadSelection::all(), output)?;
        Ok((observed.source_digest(), observed.source_bytes()))
    }

    /// Streams an exact selection while hashing the complete source in fixed working memory.
    /// The caller owns storage and publication. A failed scan may leave an unaccepted prefix
    /// in that storage; only successful completion produces observation metadata.
    ///
    /// # Errors
    /// Rejects unsafe paths, absent ranges, observed source/root changes, and original I/O errors.
    pub fn copy_selection(
        &self,
        path: &WorkspacePath,
        selection: FileReadSelection,
        output: &mut impl Write,
    ) -> Result<InspectedSelection, WorkspaceError> {
        let mut file = self.open_file(path)?;
        self.inspect_open_file(path, selection.0, None, &mut file, output)
    }

    /// Collects exact selected bytes into a buffer with an explicit caller-owned capacity.
    /// Source size has no ceiling. Use `copy_selection` or `capture_file` to include arbitrary
    /// content without collecting it into one allocation; no partial result is returned here.
    ///
    /// # Errors
    /// Rejects zero capacity, invalid/absent ranges, inclusion beyond the caller's capacity,
    /// unsafe paths, changed source/root identity, unavailable metadata, and original I/O errors.
    pub fn read_file(
        &self,
        path: &WorkspacePath,
        selection: FileReadSelection,
        maximum_bytes: u64,
    ) -> Result<InspectedFile, WorkspaceError> {
        if maximum_bytes == 0 {
            return Err(invalid("caller-owned inclusion capacity must be nonzero"));
        }
        let mut file = self.open_file(path)?;
        let mut bytes = Vec::new();
        let observed =
            self.inspect_open_file(path, selection.0, Some(maximum_bytes), &mut file, &mut bytes)?;
        Ok(InspectedFile {
            path: path.clone(),
            source_bytes: observed.source_bytes(),
            source_digest: observed.source_digest(),
            range: observed.range(),
            bytes,
        })
    }

    pub(super) fn inspect_open_file(
        &self,
        path: &WorkspacePath,
        selection: Selection,
        capacity: Option<u64>,
        file: &mut File,
        output: &mut dyn Write,
    ) -> Result<InspectedSelection, WorkspaceError> {
        let before = file.metadata().map_err(snapshot_io)?;
        let mut scan = Scan::new(selection, capacity, before.len())?;
        let mut digest = Sha256::new();
        let mut chunk = vec![0_u8; 64 * 1024];
        loop {
            let count = file.read(&mut chunk).map_err(snapshot_io)?;
            if count == 0 {
                break;
            }
            digest.update(&chunk[..count]);
            scan.accept(&chunk[..count], output)?;
        }
        if scan.offset() != before.len()
            || !same_version(&before, &file.metadata().map_err(snapshot_io)?)?
            || !same_version(&before, &self.open_file(path)?.metadata().map_err(snapshot_io)?)?
            || FolderIdentity::observe(self.identity.root()).map_err(snapshot_io)? != self.identity
        {
            return Err(changed());
        }
        let (range, selected_digest) = scan.finish()?;
        Ok(InspectedSelection::observed(
            self.identity.digest(),
            path.clone(),
            before.len(),
            Sha256Digest::new(digest.finalize().into()),
            range,
            selected_digest,
        ))
    }
}

pub(super) fn same_version(before: &Metadata, after: &Metadata) -> Result<bool, WorkspaceError> {
    let matches = before.dev() == after.dev()
        && before.ino() == after.ino()
        && before.len() == after.len()
        && before.permissions().readonly() == after.permissions().readonly()
        && before.modified().map_err(snapshot_io)? == after.modified().map_err(snapshot_io)?;
    #[cfg(unix)]
    let matches = {
        use cap_std::fs::MetadataExt as _;
        matches && before.ctime() == after.ctime() && before.ctime_nsec() == after.ctime_nsec()
    };
    Ok(matches)
}

#[cfg(test)]
mod tests;
