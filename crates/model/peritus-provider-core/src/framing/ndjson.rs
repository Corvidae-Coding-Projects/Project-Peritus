//! Incremental newline-delimited JSON framing.

use core::fmt;

use crate::ProviderCoreError;

use super::{FramingLimits, limit, malformed, strip_carriage_return};

/// One bounded newline-delimited JSON record.
#[derive(Clone, Eq, PartialEq)]
pub struct NdjsonFrame(String);

impl NdjsonFrame {
    /// Returns the record without its line delimiter.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for NdjsonFrame {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("NdjsonFrame").field("bytes", &self.0.len()).finish()
    }
}

/// Incremental bounded newline-delimited JSON framer.
#[derive(Debug)]
pub struct NdjsonParser {
    limits: FramingLimits,
    pending: Vec<u8>,
    finished: bool,
}

impl NdjsonParser {
    /// Creates an empty parser.
    #[must_use]
    pub const fn new(limits: FramingLimits) -> Self {
        Self { limits, pending: Vec::new(), finished: false }
    }

    /// Adds one arbitrary byte chunk and returns complete nonempty records.
    ///
    /// # Errors
    ///
    /// Rejects invalid UTF-8 in a complete record, bytes after `finish`, and resource limits.
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<NdjsonFrame>, ProviderCoreError> {
        if self.finished {
            return Err(malformed("cannot add bytes after framing is finished"));
        }
        let combined = self
            .pending
            .len()
            .checked_add(chunk.len())
            .ok_or_else(|| limit("framing buffer length overflowed"))?;
        if combined > self.limits.max_buffer_bytes {
            return Err(limit("framing buffer exceeds its byte bound"));
        }
        self.pending.extend_from_slice(chunk);
        self.drain_complete_lines()
    }

    /// Completes the stream and returns a final unterminated nonempty record.
    ///
    /// # Errors
    ///
    /// Rejects repeated completion, invalid terminal UTF-8, or an over-limit record.
    pub fn finish(&mut self) -> Result<Vec<NdjsonFrame>, ProviderCoreError> {
        if self.finished {
            return Err(malformed("framing was already finished"));
        }
        self.finished = true;
        let mut frames = self.drain_complete_lines()?;
        if !self.pending.is_empty() {
            let line = core::mem::take(&mut self.pending);
            if let Some(frame) = self.parse_line(strip_carriage_return(&line))? {
                frames.push(frame);
            }
        }
        Ok(frames)
    }

    fn drain_complete_lines(&mut self) -> Result<Vec<NdjsonFrame>, ProviderCoreError> {
        let mut frames = Vec::new();
        while let Some(position) = self.pending.iter().position(|byte| *byte == b'\n') {
            let mut remainder = self.pending.split_off(position + 1);
            core::mem::swap(&mut remainder, &mut self.pending);
            remainder.pop();
            if let Some(frame) = self.parse_line(strip_carriage_return(&remainder))? {
                frames.push(frame);
            }
        }
        if self.pending.len() > self.limits.max_frame_bytes {
            return Err(limit("unterminated NDJSON record exceeds the frame byte bound"));
        }
        Ok(frames)
    }

    fn parse_line(&self, line: &[u8]) -> Result<Option<NdjsonFrame>, ProviderCoreError> {
        if line.is_empty() {
            return Ok(None);
        }
        if line.len() > self.limits.max_frame_bytes {
            return Err(limit("NDJSON record exceeds its frame byte bound"));
        }
        let line = core::str::from_utf8(line)
            .map_err(|_| malformed("NDJSON record is not valid UTF-8"))?;
        Ok(Some(NdjsonFrame(line.to_owned())))
    }
}
