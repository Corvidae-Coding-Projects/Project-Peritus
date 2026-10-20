//! Snapshot-based trace iteration with header filtering before payload allocation.

use std::{
    fs::File,
    io::{self, Read as _, Seek as _},
    path::Path,
};

use super::{DeveloperTraceFrameKind, TraceFrame, TraceReadError};

/// Reads a trace one record at a time, retaining at most the selected record's payload.
///
/// The file length is captured on open. Later appends and incomplete trailing frames are excluded
/// from this snapshot. Filtering skips payloads by seeking, without reading or allocating them.
pub struct TraceReader {
    file: File,
    length: u64,
    offset: u64,
    records: u64,
}

impl TraceReader {
    /// Opens a read-only snapshot of the current trace length.
    ///
    /// # Errors
    /// Returns [`TraceReadError::Read`] if opening or inspecting the file fails.
    pub fn open(path: &Path) -> Result<Self, TraceReadError> {
        let file = File::open(path).map_err(TraceReadError::Read)?;
        let length = file.metadata().map_err(TraceReadError::Read)?.len();
        Ok(Self { file, length, offset: 0, records: 0 })
    }

    /// Number of complete records traversed, including filtered records.
    #[must_use]
    pub const fn records_read(&self) -> u64 {
        self.records
    }

    /// Rewinds this same snapshot and clears its traversed-record count.
    ///
    /// # Errors
    /// Returns [`TraceReadError::Read`] if the file cannot be rewound.
    pub fn rewind(&mut self) -> Result<(), TraceReadError> {
        self.file.rewind().map_err(TraceReadError::Read)?;
        self.offset = 0;
        self.records = 0;
        Ok(())
    }

    /// Returns the next selected complete frame, skipping other payloads before allocation.
    ///
    /// With an always-false selector, traverses only headers and counts complete records.
    /// Unknown tags are errors even when filtered; incomplete trailing frames are ignored.
    ///
    /// # Errors
    /// Returns [`TraceReadError::Read`] on I/O or allocation failure, or
    /// [`TraceReadError::Framing`] for an unknown tag or unrepresentable selected length.
    pub fn next_matching(
        &mut self,
        mut include: impl FnMut(DeveloperTraceFrameKind) -> bool,
    ) -> Result<Option<TraceFrame>, TraceReadError> {
        while self.length - self.offset >= 9 {
            let mut header = [0_u8; 9];
            self.file.read_exact(&mut header).map_err(TraceReadError::Read)?;
            let kind = DeveloperTraceFrameKind::from_tag(header[0]).ok_or_else(|| {
                TraceReadError::Framing(format!(
                    "unknown record tag {} at byte {}",
                    header[0], self.offset
                ))
            })?;
            let mut declared = [0_u8; 8];
            declared.copy_from_slice(&header[1..]);
            let length = u64::from_le_bytes(declared);
            let payload_start = self.offset + 9;
            if length > self.length - payload_start {
                self.offset = self.length;
                return Ok(None);
            }
            self.offset = payload_start + length;
            self.records += 1;
            if !include(kind) {
                self.file.seek(io::SeekFrom::Start(self.offset)).map_err(TraceReadError::Read)?;
                continue;
            }
            let length = usize::try_from(length).map_err(|_| {
                TraceReadError::Framing("selected trace record length cannot be represented".into())
            })?;
            let mut payload = Vec::new();
            payload.try_reserve_exact(length).map_err(|error| {
                TraceReadError::Read(io::Error::new(io::ErrorKind::OutOfMemory, error))
            })?;
            payload.resize(length, 0);
            self.file.read_exact(&mut payload).map_err(TraceReadError::Read)?;
            return Ok(Some(TraceFrame { kind, payload }));
        }
        Ok(None)
    }
}
