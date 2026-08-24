//! Immutable resource ceilings for one model request and response stream.

use crate::{ProtocolError, ProtocolErrorKind};

/// Complete protocol resource ceilings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_field_names,
    reason = "the max_ prefix distinguishes ceilings from observed counts"
)]
pub struct ProtocolLimits {
    max_messages: usize,
    max_content_blocks: usize,
    max_text_bytes: usize,
    max_inline_media_bytes: usize,
    max_total_media_bytes: usize,
    max_tools: usize,
    max_schema_bytes: usize,
    max_events: usize,
    max_items: usize,
    max_event_bytes: usize,
    max_output_bytes: usize,
    max_tool_argument_bytes: usize,
    max_extension_bytes: usize,
}

impl ProtocolLimits {
    /// Production-wide upper bounds. Provider profiles may only narrow them.
    pub const PRODUCTION: Self = Self {
        max_messages: 4_096,
        max_content_blocks: 16_384,
        max_text_bytes: 16 * 1024 * 1024,
        max_inline_media_bytes: 32 * 1024 * 1024,
        max_total_media_bytes: 128 * 1024 * 1024,
        max_tools: 1_024,
        max_schema_bytes: 2 * 1024 * 1024,
        max_events: 1_000_000,
        max_items: 65_536,
        max_event_bytes: 8 * 1024 * 1024,
        max_output_bytes: 256 * 1024 * 1024,
        max_tool_argument_bytes: 16 * 1024 * 1024,
        max_extension_bytes: 2 * 1024 * 1024,
    };

    /// Creates limits that are nonzero and no wider than production ceilings.
    ///
    /// # Errors
    ///
    /// Rejects a zero or widened field.
    pub fn new(values: [usize; 13]) -> Result<Self, ProtocolError> {
        let production = Self::PRODUCTION.as_array();
        if values.iter().zip(production).any(|(value, ceiling)| *value == 0 || *value > ceiling) {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidLimit,
                "protocol_limits",
                "every protocol limit must be nonzero and within its production ceiling",
            ));
        }
        Ok(Self::from_array(values))
    }

    /// Returns ceilings in stable canonical field order.
    #[must_use]
    pub const fn as_array(self) -> [usize; 13] {
        [
            self.max_messages,
            self.max_content_blocks,
            self.max_text_bytes,
            self.max_inline_media_bytes,
            self.max_total_media_bytes,
            self.max_tools,
            self.max_schema_bytes,
            self.max_events,
            self.max_items,
            self.max_event_bytes,
            self.max_output_bytes,
            self.max_tool_argument_bytes,
            self.max_extension_bytes,
        ]
    }

    /// Maximum messages per request.
    #[must_use]
    pub const fn max_messages(self) -> usize {
        self.max_messages
    }
    /// Maximum aggregate content blocks per request.
    #[must_use]
    pub const fn max_content_blocks(self) -> usize {
        self.max_content_blocks
    }
    /// Maximum bytes in one text value.
    #[must_use]
    pub const fn max_text_bytes(self) -> usize {
        self.max_text_bytes
    }
    /// Maximum bytes in one inline media value.
    #[must_use]
    pub const fn max_inline_media_bytes(self) -> usize {
        self.max_inline_media_bytes
    }
    /// Maximum aggregate inline media bytes.
    #[must_use]
    pub const fn max_total_media_bytes(self) -> usize {
        self.max_total_media_bytes
    }
    /// Maximum function tools.
    #[must_use]
    pub const fn max_tools(self) -> usize {
        self.max_tools
    }
    /// Maximum canonical bytes in one schema.
    #[must_use]
    pub const fn max_schema_bytes(self) -> usize {
        self.max_schema_bytes
    }
    /// Maximum normalized events.
    #[must_use]
    pub const fn max_events(self) -> usize {
        self.max_events
    }
    /// Maximum response items.
    #[must_use]
    pub const fn max_items(self) -> usize {
        self.max_items
    }
    /// Maximum bytes represented by one event.
    #[must_use]
    pub const fn max_event_bytes(self) -> usize {
        self.max_event_bytes
    }
    /// Maximum assembled response bytes.
    #[must_use]
    pub const fn max_output_bytes(self) -> usize {
        self.max_output_bytes
    }
    /// Maximum assembled JSON argument bytes per call.
    #[must_use]
    pub const fn max_tool_argument_bytes(self) -> usize {
        self.max_tool_argument_bytes
    }
    /// Maximum bounded provider-extension bytes.
    #[must_use]
    pub const fn max_extension_bytes(self) -> usize {
        self.max_extension_bytes
    }

    const fn from_array(values: [usize; 13]) -> Self {
        Self {
            max_messages: values[0],
            max_content_blocks: values[1],
            max_text_bytes: values[2],
            max_inline_media_bytes: values[3],
            max_total_media_bytes: values[4],
            max_tools: values[5],
            max_schema_bytes: values[6],
            max_events: values[7],
            max_items: values[8],
            max_event_bytes: values[9],
            max_output_bytes: values[10],
            max_tool_argument_bytes: values[11],
            max_extension_bytes: values[12],
        }
    }
}
