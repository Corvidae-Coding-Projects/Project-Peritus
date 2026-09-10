//! One bounded source scan with exact range capture and observed-change rejection.

use super::{
    FileReadSelection, FolderIdentity, FolderInspection, InspectedFile,
    MAX_INSPECTION_SOURCE_BYTES, WorkspaceError, changed, invalid, read_error,
    selection::Selection,
};
use cap_fs_ext::MetadataExt as _;
use cap_std::fs::Metadata;
use peritus_patch::WorkspacePath;
use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};
use std::io::Read as _;

impl FolderInspection {
    /// Reads an exact selection while hashing the complete bounded source.
    ///
    /// `maximum_bytes` bounds included bytes, not total source size. A whole-file selection
    /// exceeding it rejects with guidance to choose a range. No partial result is returned.
    ///
    /// # Errors
    /// Rejects invalid bounds/ranges, links, special files, changed identity/metadata, oversized
    /// sources or inclusions, missing lines, unavailable timestamps, and I/O failures.
    pub fn read_file(
        &self,
        path: &WorkspacePath,
        selection: FileReadSelection,
        maximum_bytes: u64,
    ) -> Result<InspectedFile, WorkspaceError> {
        if maximum_bytes == 0 || maximum_bytes > crate::MAX_INSPECTION_FILE_BYTES {
            return Err(invalid("included byte bound is outside the C1 maximum"));
        }
        let mut file = self.open_file(path)?;
        let before = file.metadata().map_err(|error| read_error(&error))?;
        let mut scan = Scan::new(selection.0, maximum_bytes, before.len())?;
        let mut digest = Sha256::new();
        let mut chunk = vec![0_u8; 64 * 1024];
        loop {
            let count = file.read(&mut chunk).map_err(|error| read_error(&error))?;
            if count == 0 {
                break;
            }
            digest.update(&chunk[..count]);
            scan.accept(&chunk[..count])?;
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
        let (range, bytes) = scan.finish()?;
        Ok(InspectedFile {
            path: path.clone(),
            source_bytes: before.len(),
            source_digest: Sha256Digest::new(digest.finalize().into()),
            range,
            bytes,
        })
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
    line: u32,
    last_seen_line: u32,
    range: Option<(u64, u64)>,
    bytes: Vec<u8>,
}
impl Scan {
    const fn new(
        selection: Selection,
        maximum: u64,
        source_size: u64,
    ) -> Result<Self, WorkspaceError> {
        if source_size > MAX_INSPECTION_SOURCE_BYTES {
            return Err(invalid("source exceeds the 64 MiB inspection ceiling"));
        }
        match selection {
            Selection::All if source_size > maximum => {
                return Err(invalid(
                    "whole file exceeds inclusion limit; select an explicit range",
                ));
            }
            Selection::Bytes { start, end } if end > source_size || end - start > maximum => {
                return Err(invalid("selected byte range is absent or exceeds inclusion limit"));
            }
            _ => {}
        }
        Ok(Self {
            selection,
            maximum,
            offset: 0,
            source_size,
            line: 1,
            last_seen_line: 0,
            range: None,
            bytes: Vec::new(),
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
                    return Err(invalid(
                        "selected lines exceed inclusion limit; select a smaller range",
                    ));
                }
                self.bytes.push(*byte);
                match &mut self.range {
                    Some((_, end)) => *end = self.offset + 1,
                    None => self.range = Some((self.offset, self.offset + 1)),
                }
            }
            if *byte == b'\n' {
                self.line += 1;
            }
            self.offset += 1;
        }
        Ok(())
    }
    fn finish(self) -> Result<((u64, u64), Vec<u8>), WorkspaceError> {
        if let Selection::Lines { last, .. } = self.selection
            && self.last_seen_line < last
        {
            return Err(invalid("selected line range does not exist in the complete source"));
        }
        Ok((self.range.unwrap_or((0, 0)), self.bytes))
    }
}

#[cfg(test)]
mod tests;
