//! Incremental Server-Sent Events framing.

use core::fmt;

use crate::ProviderCoreError;

use super::{FramingLimits, limit, malformed, strip_carriage_return};

/// One bounded SSE event.
#[derive(Clone, Eq, PartialEq)]
pub struct SseFrame {
    event: Option<String>,
    data: String,
    id: Option<String>,
}

impl SseFrame {
    /// Returns the optional event name.
    #[must_use]
    pub fn event(&self) -> Option<&str> {
        self.event.as_deref()
    }

    /// Returns multiline data joined with newline separators.
    #[must_use]
    pub fn data(&self) -> &str {
        &self.data
    }

    /// Returns the optional event identity.
    #[must_use]
    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }
}

impl fmt::Debug for SseFrame {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SseFrame")
            .field("event", &self.event)
            .field("data_bytes", &self.data.len())
            .field("id", &self.id.as_ref().map(|_| "[redacted]"))
            .finish()
    }
}

/// One bounded SSE comment.
#[derive(Clone, Eq, PartialEq)]
pub struct SseComment(pub(super) String);

impl SseComment {
    /// Returns the comment text after the leading colon and optional space.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SseComment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("SseComment").field("bytes", &self.0.len()).finish()
    }
}

/// One parsed SSE item.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SseItem {
    /// A data-bearing event.
    Event(SseFrame),
    /// A comment or keepalive line.
    Comment(SseComment),
    /// The conventional exact `data: [DONE]` sentinel.
    Done,
}

/// Incremental bounded SSE parser.
#[derive(Debug)]
pub struct SseParser {
    limits: FramingLimits,
    pending: Vec<u8>,
    event: Option<String>,
    data_lines: Vec<String>,
    id: Option<String>,
    frame_bytes: usize,
    finished: bool,
}

impl SseParser {
    /// Creates an empty parser.
    #[must_use]
    pub const fn new(limits: FramingLimits) -> Self {
        Self {
            limits,
            pending: Vec::new(),
            event: None,
            data_lines: Vec::new(),
            id: None,
            frame_bytes: 0,
            finished: false,
        }
    }

    /// Adds one arbitrary byte chunk and returns every complete item.
    ///
    /// UTF-8 may be fragmented across chunks.
    ///
    /// # Errors
    ///
    /// Rejects invalid UTF-8 in a complete line, data after `finish`, and frame or buffer limits.
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<SseItem>, ProviderCoreError> {
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

    /// Completes the stream and parses a final unterminated line and event, when present.
    ///
    /// # Errors
    ///
    /// Rejects repeated completion, invalid terminal UTF-8, or an over-limit final frame.
    pub fn finish(&mut self) -> Result<Vec<SseItem>, ProviderCoreError> {
        if self.finished {
            return Err(malformed("framing was already finished"));
        }
        self.finished = true;
        let mut items = self.drain_complete_lines()?;
        if !self.pending.is_empty() {
            let line = core::mem::take(&mut self.pending);
            self.process_line(strip_carriage_return(&line), &mut items)?;
        }
        self.dispatch(&mut items)?;
        Ok(items)
    }

    fn drain_complete_lines(&mut self) -> Result<Vec<SseItem>, ProviderCoreError> {
        let mut items = Vec::new();
        while let Some(position) = self.pending.iter().position(|byte| *byte == b'\n') {
            let mut remainder = self.pending.split_off(position + 1);
            core::mem::swap(&mut remainder, &mut self.pending);
            remainder.pop();
            self.process_line(strip_carriage_return(&remainder), &mut items)?;
        }
        if self.pending.len() > self.limits.max_frame_bytes {
            return Err(limit("unterminated SSE line exceeds the frame byte bound"));
        }
        Ok(items)
    }

    fn process_line(
        &mut self,
        line: &[u8],
        items: &mut Vec<SseItem>,
    ) -> Result<(), ProviderCoreError> {
        let line =
            core::str::from_utf8(line).map_err(|_| malformed("SSE line is not valid UTF-8"))?;
        if line.is_empty() {
            return self.dispatch(items);
        }
        if let Some(comment) = line.strip_prefix(':') {
            if line.len() > self.limits.max_frame_bytes {
                return Err(limit("SSE comment exceeds its frame byte bound"));
            }
            let comment = comment.strip_prefix(' ').unwrap_or(comment);
            items.push(SseItem::Comment(SseComment(comment.to_owned())));
            return Ok(());
        }
        self.add_frame_bytes(line.len())?;
        let (field, value) = line
            .split_once(':')
            .map_or((line, ""), |(field, value)| (field, value.strip_prefix(' ').unwrap_or(value)));
        match field {
            "event" => self.event = Some(value.to_owned()),
            "data" => self.data_lines.push(value.to_owned()),
            "id" if !value.contains('\0') => self.id = Some(value.to_owned()),
            _ => {}
        }
        Ok(())
    }

    fn add_frame_bytes(&mut self, bytes: usize) -> Result<(), ProviderCoreError> {
        self.frame_bytes = self
            .frame_bytes
            .checked_add(bytes)
            .ok_or_else(|| limit("SSE frame byte count overflowed"))?;
        if self.frame_bytes > self.limits.max_frame_bytes {
            return Err(limit("SSE frame exceeds its byte bound"));
        }
        Ok(())
    }

    fn dispatch(&mut self, items: &mut Vec<SseItem>) -> Result<(), ProviderCoreError> {
        if self.data_lines.is_empty() {
            self.reset_frame();
            return Ok(());
        }
        let data_bytes = self.data_lines.iter().try_fold(0_usize, |total, line| {
            total
                .checked_add(line.len())
                .and_then(|value| value.checked_add(1))
                .ok_or_else(|| limit("SSE data length overflowed"))
        })?;
        if data_bytes.saturating_sub(1) > self.limits.max_frame_bytes {
            return Err(limit("SSE data exceeds its frame byte bound"));
        }
        let data = self.data_lines.join("\n");
        if data == "[DONE]" {
            items.push(SseItem::Done);
        } else {
            items.push(SseItem::Event(SseFrame {
                event: self.event.take(),
                data,
                id: self.id.take(),
            }));
        }
        self.reset_frame();
        Ok(())
    }

    fn reset_frame(&mut self) {
        self.event = None;
        self.data_lines.clear();
        self.id = None;
        self.frame_bytes = 0;
    }
}
