//! Explicit external text snapshots with exact range and source-digest observations.

use crate::image_import::{open_regular, validate_explicit_path};
use peritus_app_protocol::{WorkbenchFileMetadata, WorkbenchFileRange};
use sha2::{Digest, Sha256};
use std::{fmt, io::Read as _, path::Path};

type ScannedSource = (Vec<u8>, peritus_types::Sha256Digest, u64, (u64, u64));

const SCAN_BUFFER_BYTES: usize = 64 * 1024;

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
    selection.validate().map_err(|_| "The selected range is malformed.")?;
    let label = path
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .filter(|value| !value.is_empty() && value.len() <= 4096)
        .filter(|value| !value.chars().any(char::is_control))
        .ok_or("The file name must be 1-4096 bytes of UTF-8 without controls.")?
        .to_owned();
    let before = std::fs::symlink_metadata(path)
        .map_err(|_| "Cannot inspect the selected external file.")?;
    if !before.is_file() || before.file_type().is_symlink() {
        return Err("Choose a regular file, not a symlink, directory, pipe, or device.");
    }
    let mut file = open_regular(path)?;
    let (bytes, source_digest, source_bytes, range) =
        scan_source(&mut file, selection, before.len())?;
    let after = file.metadata().map_err(|_| "Cannot recheck the selected external file.")?;
    if source_bytes != before.len()
        || after.len() != before.len()
        || after.modified().ok() != before.modified().ok()
    {
        return Err("The external file changed while reading; select it again.");
    }
    peritus_product_runner::attachment::ValidatedFileText::new(bytes.clone())
        .map_err(|_| "Selected bytes must be exact UTF-8 text without binary controls.")?;
    let digest = peritus_codec::sha256(&bytes);
    let file = WorkbenchFileMetadata::new(source_digest, source_bytes, range, digest)
        .map_err(|_| "The selected source metadata is invalid.")?;
    Ok(FileBytes { bytes, file, label })
}

fn scan_source(
    source: &mut std::fs::File,
    selection: WorkbenchFileRange,
    expected_bytes: u64,
) -> Result<ScannedSource, &'static str> {
    let range = match selection {
        WorkbenchFileRange::All => (0, expected_bytes),
        WorkbenchFileRange::Bytes { start, end } if end <= expected_bytes => (start, end),
        WorkbenchFileRange::Bytes { .. } => {
            return Err("The selected byte range is outside the source.");
        }
        WorkbenchFileRange::Lines { .. } => (0, 0),
    };
    let line_selection = match selection {
        WorkbenchFileRange::Lines { first, last } => Some((u64::from(first), u64::from(last))),
        _ => None,
    };
    let mut selected = Vec::new();
    let mut hasher = Sha256::new();
    let mut buffer = Vec::new();
    buffer
        .try_reserve_exact(SCAN_BUFFER_BYTES)
        .map_err(|_| "Cannot allocate source read buffer.")?;
    buffer.resize(SCAN_BUFFER_BYTES, 0);
    let mut total = 0_u64;
    let mut line = 1_u64;
    let mut start = line_selection.map_or(range.0, |_| 0);
    let mut end = if line_selection.is_none() { range.1 } else { 0 };
    let mut line_found = line_selection.is_none();
    let mut last_byte = None;
    loop {
        let count = source
            .read(&mut buffer)
            .map_err(|_| "Could not read the selected external file completely.")?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
        selected.try_reserve(count).map_err(|_| "Cannot allocate selected source range.")?;
        for (index, byte) in buffer[..count].iter().copied().enumerate() {
            let offset = total.checked_add(index as u64).ok_or("External source size overflow.")?;
            let selected_byte = if let Some((first, last)) = line_selection {
                if line == first && !line_found {
                    start = offset;
                    line_found = true;
                }
                let selected = line_found && line <= last;
                if selected {
                    end = offset + 1;
                }
                selected
            } else {
                offset >= range.0 && offset < range.1
            };
            if selected_byte {
                selected.push(byte);
            }
            last_byte = Some(byte);
            if byte == b'\n' {
                line = line.saturating_add(1);
            }
        }
        total = total.checked_add(count as u64).ok_or("External source size overflow.")?;
    }
    if let Some((_, last)) = line_selection {
        if !line_found || line < last || (last_byte == Some(b'\n') && line == last) {
            return Err("The selected line range does not exist in the complete source.");
        }
        end = end.max(start);
    }
    Ok((selected, peritus_types::Sha256Digest::new(hasher.finalize().into()), total, (start, end)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Seek as _, SeekFrom, Write as _};

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
        file.set_len(64 * 1024 * 1024 + 1).expect("sparse fixture");
        let mut file = std::fs::OpenOptions::new().write(true).open(&path).expect("open");
        file.seek(SeekFrom::End(-4)).expect("seek tail");
        file.write_all(b"tail").expect("write tail");
        let selected = read(
            &path,
            WorkbenchFileRange::Bytes { start: 64 * 1024 * 1024 - 3, end: 64 * 1024 * 1024 + 1 },
        )
        .expect("stream large source and retain chosen range");
        assert_eq!(selected.bytes, b"tail");
        assert_eq!(selected.file.source_bytes(), 64 * 1024 * 1024 + 1);
    }

    #[test]
    fn selected_text_has_no_256_kibibyte_admission_ceiling() {
        let directory = tempfile::tempdir().expect("directory");
        let path = directory.path().join("large.txt");
        let bytes = vec![b'a'; 300 * 1024];
        std::fs::write(&path, &bytes).expect("source");
        let selected = read(&path, WorkbenchFileRange::All).expect("complete valid selection");
        assert_eq!(selected.bytes.len(), bytes.len());
        assert_eq!(selected.file.digest(), peritus_codec::sha256(&bytes));
    }
}
