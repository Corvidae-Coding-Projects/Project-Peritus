//! Explicit external text snapshots with exact range and source-digest observations.

use crate::image_import::{open_regular, validate_explicit_path};
use peritus_app_protocol::{MAX_WORKBENCH_FILE_BYTES, WorkbenchFileMetadata, WorkbenchFileRange};
use std::{fmt, io::Read as _, path::Path};

const MAX_SOURCE_BYTES: u64 = 64 * 1024 * 1024;

/// Exact selected text and the complete external-source observation made by the client.
pub struct FileBytes {
    pub(super) bytes: Vec<u8>,
    pub(super) file: WorkbenchFileMetadata,
    pub(super) label: String,
}

impl fmt::Debug for FileBytes {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FileBytes")
            .field("selected_bytes", &self.bytes.len())
            .field("source_bytes", &self.file.source_bytes())
            .field("source_digest", &self.file.source_digest())
            .finish_non_exhaustive()
    }
}

/// Reads and selects one explicitly chosen external file without retaining path authority.
pub fn read(path: &Path, selection: WorkbenchFileRange) -> Result<FileBytes, &'static str> {
    validate_explicit_path(path)?;
    let label = path
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .filter(|value| !value.is_empty() && value.len() <= 1024)
        .filter(|value| !value.chars().any(char::is_control))
        .ok_or("The file name must be 1-1024 bytes of UTF-8 without controls.")?
        .to_owned();
    let before = std::fs::symlink_metadata(path)
        .map_err(|_| "Cannot inspect the selected external file.")?;
    if !before.is_file() || before.file_type().is_symlink() {
        return Err("Choose a regular file, not a symlink, directory, pipe, or device.");
    }
    if before.len() > MAX_SOURCE_BYTES {
        return Err("External text source exceeds 64 MiB; nothing was read or truncated.");
    }
    let mut source = Vec::with_capacity(
        usize::try_from(before.len()).map_err(|_| "External text source is too large.")?,
    );
    let mut file = open_regular(path)?;
    (&mut file)
        .take(MAX_SOURCE_BYTES + 1)
        .read_to_end(&mut source)
        .map_err(|_| "Could not read the selected external file completely.")?;
    let after = file.metadata().map_err(|_| "Cannot recheck the selected external file.")?;
    if source.len() as u64 != before.len()
        || after.len() != before.len()
        || after.modified().ok() != before.modified().ok()
        || source.len() as u64 > MAX_SOURCE_BYTES
    {
        return Err("The external file changed while reading; select it again.");
    }
    let range = resolve_range(&source, selection)?;
    let selected = source
        .get(range.0..range.1)
        .ok_or("The selected range is outside the external source.")?
        .to_vec();
    peritus_product_runner::attachment::ValidatedFileText::new(selected.clone()).map_err(
        |_| "Selected bytes must be at most 256 KiB of exact UTF-8 text without binary controls.",
    )?;
    let source_digest = peritus_codec::sha256(&source);
    let digest = peritus_codec::sha256(&selected);
    let file = WorkbenchFileMetadata::new(
        source_digest,
        source.len() as u64,
        (range.0 as u64, range.1 as u64),
        digest,
    )
    .map_err(|_| "The selected range exceeds the file import limits.")?;
    Ok(FileBytes { bytes: selected, file, label })
}

fn resolve_range(
    source: &[u8],
    selection: WorkbenchFileRange,
) -> Result<(usize, usize), &'static str> {
    match selection {
        WorkbenchFileRange::All => {
            if source.len() as u64 > MAX_WORKBENCH_FILE_BYTES {
                return Err("Whole file exceeds 256 KiB; choose an explicit byte or line range.");
            }
            Ok((0, source.len()))
        }
        WorkbenchFileRange::Bytes { start, end } => {
            let start = usize::try_from(start).map_err(|_| "Byte range is too large.")?;
            let end = usize::try_from(end).map_err(|_| "Byte range is too large.")?;
            if end > source.len() || end.saturating_sub(start) as u64 > MAX_WORKBENCH_FILE_BYTES {
                return Err("Byte range is absent or exceeds 256 KiB.");
            }
            Ok((start, end))
        }
        WorkbenchFileRange::Lines { first, last } => resolve_lines(source, first, last),
    }
}

fn resolve_lines(source: &[u8], first: u32, last: u32) -> Result<(usize, usize), &'static str> {
    let mut line = 1_u32;
    let mut start = None;
    let mut end = None;
    for (offset, byte) in source.iter().copied().enumerate() {
        if line == first && start.is_none() {
            start = Some(offset);
        }
        if (first..=last).contains(&line) {
            end = Some(offset + 1);
            if end.unwrap_or(0).saturating_sub(start.unwrap_or(0)) as u64 > MAX_WORKBENCH_FILE_BYTES
            {
                return Err("Selected lines exceed 256 KiB; choose a smaller range.");
            }
        }
        if byte == b'\n' {
            line = line.saturating_add(1);
        }
    }
    if source.is_empty() || line < last || (source.last() == Some(&b'\n') && line == last) {
        return Err("The selected line range does not exist in the complete source.");
    }
    let start = start.ok_or("The selected line range does not exist in the complete source.")?;
    Ok((start, end.unwrap_or(start)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_external_ranges_preserve_line_endings_and_source_identity() {
        let directory = tempfile::tempdir().expect("directory");
        let path = directory.path().join("source.txt");
        std::fs::write(&path, b"first\r\n\nlast").expect("source");
        let selected =
            read(&path, WorkbenchFileRange::Lines { first: 2, last: 3 }).expect("selection");
        assert_eq!(selected.bytes, b"\nlast");
        assert_eq!(selected.file.range(), (7, 12));
        assert_eq!(selected.file.source_digest(), peritus_codec::sha256(b"first\r\n\nlast"));
        assert_eq!(selected.file.digest(), peritus_codec::sha256(b"\nlast"));
        assert!(!format!("{selected:?}").contains("last"));
    }

    #[test]
    fn external_import_rejects_escape_missing_lines_binary_and_large_whole_source() {
        let directory = tempfile::tempdir().expect("directory");
        let path = directory.path().join("source.txt");
        std::fs::write(&path, b"one\ntwo\n").expect("source");
        assert!(read(Path::new("source.txt"), WorkbenchFileRange::All).is_err());
        assert!(
            read(&path, WorkbenchFileRange::Lines { first: 3, last: 3 }).is_err(),
            "a trailing newline does not create another source line"
        );
        std::fs::write(&path, b"text\0binary").expect("binary");
        assert!(read(&path, WorkbenchFileRange::All).is_err());
        let file = std::fs::File::create(&path).expect("large");
        file.set_len(MAX_WORKBENCH_FILE_BYTES + 1).expect("sparse fixture");
        assert!(read(&path, WorkbenchFileRange::All).is_err());
    }
}
