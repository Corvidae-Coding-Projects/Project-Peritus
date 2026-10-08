//! One streamed source scan with exact inclusion capture and observed-change rejection.

use super::{
    FileReadSelection, FolderIdentity, FolderInspection, InspectedFile, WorkspaceError, changed,
    invalid, read_error, selection::Selection,
};
use cap_fs_ext::MetadataExt as _;
use cap_std::fs::Metadata;
use peritus_patch::WorkspacePath;
use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};
use std::io::Read as _;

impl FolderInspection {
    /// Streams one exact no-follow regular file to a bounded visitor.
    ///
    /// Returning `None` indicates that cancellation was observed between chunks. The complete
    /// source is checked against its opened handle, current path binding, and registered root
    /// identity before a digest is returned.
    ///
    /// # Errors
    /// Returns a typed failure for an unsafe, changed, or unavailable source.
    pub fn scan_file_chunks(
        &self,
        path: &WorkspacePath,
        mut cancelled: impl FnMut() -> bool,
        mut visit: impl FnMut(u64, &[u8]),
    ) -> Result<Option<(u64, Sha256Digest)>, WorkspaceError> {
        let mut file = self.open_file(path)?;
        let before = file.metadata().map_err(|error| read_error(&error))?;
        let mut digest = Sha256::new();
        let mut offset = 0_u64;
        let mut chunk = vec![0_u8; 64 * 1024];
        loop {
            if cancelled() {
                return Ok(None);
            }
            let count = file.read(&mut chunk).map_err(|error| read_error(&error))?;
            if count == 0 {
                break;
            }
            let next = offset
                .checked_add(u64::try_from(count).map_err(|_| changed())?)
                .ok_or_else(changed)?;
            if next > before.len() {
                return Err(changed());
            }
            digest.update(&chunk[..count]);
            visit(offset, &chunk[..count]);
            offset = next;
            if cancelled() {
                return Ok(None);
            }
        }
        if offset != before.len()
            || !same_version(&before, &file.metadata().map_err(|error| read_error(&error))?)?
            || !same_version(
                &before,
                &self.open_file(path)?.metadata().map_err(|error| read_error(&error))?,
            )?
            || FolderIdentity::observe(self.identity.root()).map_err(|error| read_error(&error))?
                != self.identity
        {
            return Err(changed());
        }
        Ok(Some((offset, Sha256Digest::new(digest.finalize().into()))))
    }

    /// Reads an exact selection while hashing the complete source.
    ///
    /// `maximum_bytes` bounds included bytes, not total source size. A whole-file selection
    /// exceeding it rejects with guidance to choose a range. Oversized line selections return
    /// their first page and the exact next source-byte offset.
    ///
    /// # Errors
    /// Rejects invalid bounds/ranges, links, special files, changed identity/metadata, oversized
    /// ranges or inclusions, missing lines, unavailable timestamps, and I/O failures.
    pub fn read_file(
        &self,
        path: &WorkspacePath,
        selection: FileReadSelection,
        maximum_bytes: u64,
    ) -> Result<InspectedFile, WorkspaceError> {
        self.read_file_cancellable(path, selection, maximum_bytes, || false)?.ok_or_else(changed)
    }

    /// Reads an exact selection while checking cancellation between source chunks.
    ///
    /// # Errors
    /// Returns a typed failure for invalid bounds, unsafe paths, source drift, or I/O failures.
    pub fn read_file_cancellable(
        &self,
        path: &WorkspacePath,
        selection: FileReadSelection,
        maximum_bytes: u64,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<Option<InspectedFile>, WorkspaceError> {
        if maximum_bytes == 0 {
            return Err(invalid("included byte bound must be nonzero"));
        }
        let mut file = self.open_file(path)?;
        let before = file.metadata().map_err(|error| read_error(&error))?;
        let mut scan = Scan::new(selection.0, maximum_bytes, before.len())?;
        let mut digest = Sha256::new();
        let mut chunk = vec![0_u8; 64 * 1024];
        loop {
            if cancelled() {
                return Ok(None);
            }
            let count = file.read(&mut chunk).map_err(|error| read_error(&error))?;
            if count == 0 {
                break;
            }
            digest.update(&chunk[..count]);
            scan.accept(&chunk[..count])?;
            if cancelled() {
                return Ok(None);
            }
        }
        if scan.offset != before.len()
            || !same_version(&before, &file.metadata().map_err(|error| read_error(&error))?)?
            || !same_version(
                &before,
                &self.open_file(path)?.metadata().map_err(|error| read_error(&error))?,
            )?
            || FolderIdentity::observe(self.identity.root()).map_err(|error| read_error(&error))?
                != self.identity
        {
            return Err(changed());
        }
        let scanned = scan.finish()?;
        Ok(Some(InspectedFile {
            path: path.clone(),
            source_bytes: before.len(),
            source_digest: Sha256Digest::new(digest.finalize().into()),
            range: scanned.range,
            continuation_offset: scanned.continuation_offset,
            bytes: scanned.bytes,
        }))
    }
}

fn same_version(before: &Metadata, after: &Metadata) -> Result<bool, WorkspaceError> {
    let matches = before.dev() == after.dev()
        && before.ino() == after.ino()
        && before.len() == after.len()
        && before.modified().map_err(|error| read_error(&error))?
            == after.modified().map_err(|error| read_error(&error))?;
    #[cfg(unix)]
    let matches = {
        use cap_std::fs::MetadataExt as _;
        matches && before.ctime() == after.ctime() && before.ctime_nsec() == after.ctime_nsec()
    };
    Ok(matches)
}

struct Scan {
    selection: Selection,
    maximum: u64,
    offset: u64,
    source_size: u64,
    line: u64,
    last_seen_line: u64,
    range: Option<(u64, u64)>,
    continuation_offset: Option<u64>,
    bytes: Vec<u8>,
}

#[derive(Debug, Eq, PartialEq)]
struct ScannedSelection {
    range: (u64, u64),
    bytes: Vec<u8>,
    continuation_offset: Option<u64>,
}

impl Scan {
    fn new(selection: Selection, maximum: u64, source_size: u64) -> Result<Self, WorkspaceError> {
        match selection {
            Selection::All if source_size > maximum => {
                return Err(invalid(
                    "whole file exceeds inclusion limit; select an explicit range",
                ));
            }
            Selection::Bytes { start, end }
                if start > end || end > source_size || end - start > maximum =>
            {
                return Err(invalid("selected byte range is absent or exceeds inclusion limit"));
            }
            _ => {}
        }
        let mut bytes = Vec::new();
        if matches!(selection, Selection::All) {
            let capacity = usize::try_from(source_size)
                .map_err(|_| invalid("whole-file selection cannot be represented in memory"))?;
            bytes.try_reserve_exact(capacity).map_err(|error| {
                let error = std::io::Error::other(error);
                read_error(&error)
            })?;
        }
        Ok(Self {
            selection,
            maximum,
            offset: 0,
            source_size,
            line: 1,
            last_seen_line: 0,
            range: None,
            continuation_offset: None,
            bytes,
        })
    }
    fn accept(&mut self, bytes: &[u8]) -> Result<(), WorkspaceError> {
        if self.offset.saturating_add(bytes.len() as u64) > self.source_size {
            return Err(changed());
        }
        for byte in bytes {
            self.last_seen_line = self.line;
            let included = match self.selection {
                Selection::All => true,
                Selection::Bytes { start, end } => (start..end).contains(&self.offset),
                Selection::Lines { first, last } => (first..=last).contains(&self.line),
            };
            if included {
                if self.bytes.len() as u64 >= self.maximum {
                    self.continuation_offset.get_or_insert(self.offset);
                } else {
                    self.bytes.push(*byte);
                    match &mut self.range {
                        Some((_, end)) => *end = self.offset + 1,
                        None => self.range = Some((self.offset, self.offset + 1)),
                    }
                }
            }
            if *byte == b'\n' {
                self.line = self
                    .line
                    .checked_add(1)
                    .ok_or_else(|| invalid("source line accounting exceeds its representation"))?;
            }
            self.offset += 1;
        }
        Ok(())
    }
    fn finish(self) -> Result<ScannedSelection, WorkspaceError> {
        if let Selection::Lines { last, .. } = self.selection
            && self.last_seen_line < last
        {
            return Err(invalid("selected line range does not exist in the complete source"));
        }
        let empty_range = match self.selection {
            Selection::Bytes { start, .. } => (start, start),
            _ => (0, 0),
        };
        Ok(ScannedSelection {
            range: self.range.unwrap_or(empty_range),
            bytes: self.bytes,
            continuation_offset: self.continuation_offset,
        })
    }
}

#[cfg(test)]
mod tests;
