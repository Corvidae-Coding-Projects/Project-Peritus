//! Explicit external text snapshots with exact range and source-digest observations.

use crate::image_import::{open_regular, validate_explicit_path};
use peritus_app_protocol::{WorkbenchFileMetadata, WorkbenchFileRange};
#[cfg(test)]
use peritus_app_protocol::MAX_WORKBENCH_FILE_BYTES;
use peritus_product_runner::attachment::ValidatedFileTextSource;
use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};
use std::{
    fmt,
    fs::{File, Metadata},
    io::{Read as _, Seek as _, SeekFrom},
    path::Path,
};

/// Exact selected text and the complete external-source observation made by the client.
pub struct FileBytes {
    source: ValidatedFileTextSource<SelectedFile>,
    pub(super) file: WorkbenchFileMetadata,
    pub(super) label: String,
}

impl fmt::Debug for FileBytes {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FileBytes")
            .field("selected_bytes", &self.source.bytes())
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
        .filter(|value| !value.is_empty())
        .filter(|value| !value.chars().any(char::is_control))
        .ok_or("The file name must be nonempty UTF-8 without controls.")?
        .to_owned();
    let before = std::fs::symlink_metadata(path)
        .map_err(|_| "Cannot inspect the selected external file.")?;
    if !before.is_file() || before.file_type().is_symlink() {
        return Err("Choose a regular file, not a symlink, directory, pipe, or device.");
    }
    let mut file = open_regular(path)?;
    let opened = file.metadata().map_err(|_| "Cannot inspect the opened external file.")?;
    if !same_version(&before, &opened) {
        return Err("The external file changed before it could be read; select it again.");
    }
    let observed = observe_source(&mut file, selection)?;
    let after = file.metadata().map_err(|_| "Cannot recheck the selected external file.")?;
    if observed.source_bytes != before.len() || !same_version(&opened, &after) {
        return Err("The external file changed while reading; select it again.");
    }
    let mut selected = SelectedFile::new(file, observed.range)?;
    let selected_bytes = observed.range.1 - observed.range.0;
    let selected_digest = if observed.range == (0, observed.source_bytes) {
        observed.source_digest
    } else {
        digest_selection(&mut selected, selected_bytes)?
    };
    let mut source = ValidatedFileTextSource::new(selected, selected_digest, selected_bytes)
        .map_err(|_| "Selected bytes must be exact UTF-8 text without binary controls.")?;
    if !same_version(
        &after,
        &source
            .reader_mut()
            .metadata()
            .map_err(|_| "Cannot recheck the selected external file.")?,
    ) {
        return Err("The external file changed while validating; select it again.");
    }
    let file = WorkbenchFileMetadata::new(
        observed.source_digest,
        observed.source_bytes,
        observed.range,
        selected_digest,
    )
    .map_err(|_| "The selected range exceeds the file import limits.")?;
    Ok(FileBytes {
        source,
        file,
        label,
    })
}

impl FileBytes {
    pub(super) const fn byte_len(&self) -> u64 {
        self.source.bytes()
    }

    pub(super) fn read_chunk(
        &mut self,
        offset: u64,
        maximum_bytes: usize,
    ) -> Result<Vec<u8>, &'static str> {
        if maximum_bytes == 0 || offset >= self.byte_len() {
            return Err("Text upload chunk selection is invalid.");
        }
        self.source
            .reader_mut()
            .seek(SeekFrom::Start(offset))
            .map_err(|_| "Could not seek the selected text.")?;
        let maximum =
            u64::try_from(maximum_bytes).map_err(|_| "Text chunk length is not representable.")?;
        let length = usize::try_from((self.byte_len() - offset).min(maximum))
            .map_err(|_| "Text chunk length is not representable.")?;
        let mut bytes = vec![0_u8; length];
        self.source
            .reader_mut()
            .read_exact(&mut bytes)
            .map_err(|_| "The selected text changed while uploading.")?;
        Ok(bytes)
    }
}

struct SourceObservation {
    source_bytes: u64,
    source_digest: Sha256Digest,
    range: (u64, u64),
}

fn observe_source(
    file: &mut File,
    selection: WorkbenchFileRange,
) -> Result<SourceObservation, &'static str> {
    file.seek(SeekFrom::Start(0))
        .map_err(|_| "Could not start reading the selected external file.")?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut source_bytes = 0_u64;
    let mut lines = match selection {
        WorkbenchFileRange::Lines { first, last } => Some(LineScan::new(first, last)),
        _ => None,
    };
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|_| "Could not read the selected external file completely.")?;
        if count == 0 {
            break;
        }
        if let Some(lines) = &mut lines {
            lines.accept(source_bytes, &buffer[..count])?;
        }
        source_bytes = source_bytes
            .checked_add(u64::try_from(count).map_err(|_| "Source size is not representable.")?)
            .ok_or("Source size is not representable.")?;
        digest.update(&buffer[..count]);
    }
    let range = match selection {
        WorkbenchFileRange::All => (0, source_bytes),
        WorkbenchFileRange::Bytes { start, end } if end <= source_bytes => (start, end),
        WorkbenchFileRange::Bytes { .. } => {
            return Err("Byte range is outside the selected source.");
        }
        WorkbenchFileRange::Lines { .. } => lines
            .ok_or("The selected line range is invalid.")?
            .finish(source_bytes)?,
    };
    Ok(SourceObservation {
        source_bytes,
        source_digest: Sha256Digest::new(digest.finalize().into()),
        range,
    })
}

struct LineScan {
    first: u64,
    last: u64,
    current: u64,
    start: Option<u64>,
    end: Option<u64>,
    trailing_newline: bool,
}

impl LineScan {
    const fn new(first: u64, last: u64) -> Self {
        Self { first, last, current: 1, start: None, end: None, trailing_newline: false }
    }

    fn accept(&mut self, base: u64, bytes: &[u8]) -> Result<(), &'static str> {
        for (index, byte) in bytes.iter().copied().enumerate() {
            let offset = base
                .checked_add(u64::try_from(index).map_err(|_| "Source size is not representable.")?)
                .ok_or("Source size is not representable.")?;
            if self.current == self.first && self.start.is_none() {
                self.start = Some(offset);
            }
            if (self.first..=self.last).contains(&self.current) {
                self.end = Some(
                    offset.checked_add(1).ok_or("Source size is not representable.")?,
                );
            }
            self.trailing_newline = byte == b'\n';
            if self.trailing_newline {
                self.current = self
                    .current
                    .checked_add(1)
                    .ok_or("Line count is not representable.")?;
            }
        }
        Ok(())
    }

    fn finish(self, source_bytes: u64) -> Result<(u64, u64), &'static str> {
        if source_bytes == 0
            || self.current < self.last
            || (self.trailing_newline && self.current == self.last)
        {
            return Err("The selected line range does not exist in the complete source.");
        }
        let start =
            self.start.ok_or("The selected line range does not exist in the complete source.")?;
        Ok((start, self.end.unwrap_or(start)))
    }
}

struct SelectedFile {
    file: File,
    start: u64,
    bytes: u64,
    position: u64,
}

impl SelectedFile {
    fn new(mut file: File, range: (u64, u64)) -> Result<Self, &'static str> {
        file.seek(SeekFrom::Start(range.0))
            .map_err(|_| "Could not seek to the selected text.")?;
        Ok(Self { file, start: range.0, bytes: range.1 - range.0, position: 0 })
    }

    fn metadata(&self) -> std::io::Result<Metadata> {
        self.file.metadata()
    }
}

impl std::io::Read for SelectedFile {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let remaining = self.bytes.saturating_sub(self.position);
        if remaining == 0 || buffer.is_empty() {
            return Ok(0);
        }
        let buffer_bytes = u64::try_from(buffer.len()).unwrap_or(u64::MAX);
        let length = usize::try_from(remaining.min(buffer_bytes))
            .map_err(|_| std::io::Error::other("selected text length overflow"))?;
        let count = self.file.read(&mut buffer[..length])?;
        self.position = self
            .position
            .checked_add(u64::try_from(count).map_err(|_| {
                std::io::Error::other("selected text position overflow")
            })?)
            .ok_or_else(|| std::io::Error::other("selected text position overflow"))?;
        Ok(count)
    }
}

impl std::io::Seek for SelectedFile {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        let target = match position {
            SeekFrom::Start(offset) => i128::from(offset),
            SeekFrom::End(offset) => i128::from(self.bytes) + i128::from(offset),
            SeekFrom::Current(offset) => i128::from(self.position) + i128::from(offset),
        };
        if target < 0 || target > i128::from(self.bytes) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "seek outside selected text",
            ));
        }
        let target = u64::try_from(target)
            .map_err(|_| std::io::Error::other("selected text position overflow"))?;
        let absolute = self
            .start
            .checked_add(target)
            .ok_or_else(|| std::io::Error::other("selected text position overflow"))?;
        self.file.seek(SeekFrom::Start(absolute))?;
        self.position = target;
        Ok(target)
    }
}

fn digest_selection(
    selected: &mut SelectedFile,
    expected_bytes: u64,
) -> Result<Sha256Digest, &'static str> {
    selected
        .seek(SeekFrom::Start(0))
        .map_err(|_| "Could not seek to the selected text.")?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut total = 0_u64;
    loop {
        let count = selected
            .read(&mut buffer)
            .map_err(|_| "Could not read the selected text completely.")?;
        if count == 0 {
            break;
        }
        total = total
            .checked_add(u64::try_from(count).map_err(|_| "Text size is not representable.")?)
            .ok_or("Text size is not representable.")?;
        digest.update(&buffer[..count]);
    }
    if total != expected_bytes {
        return Err("The selected text changed while validating; select it again.");
    }
    selected
        .seek(SeekFrom::Start(0))
        .map_err(|_| "Could not rewind the selected text.")?;
    Ok(Sha256Digest::new(digest.finalize().into()))
}

fn same_version(before: &Metadata, after: &Metadata) -> bool {
    before.is_file()
        && after.is_file()
        && before.len() == after.len()
        && before.permissions().readonly() == after.permissions().readonly()
        && before.modified().ok() == after.modified().ok()
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
