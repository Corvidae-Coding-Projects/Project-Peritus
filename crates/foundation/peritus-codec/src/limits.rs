//! Shared encoder and decoder resource limits.

/// Explicit resource ceilings for one canonical message.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CodecLimits {
    /// Maximum complete frame size.
    pub max_frame_bytes: usize,
    /// Maximum payload size inside a frame.
    pub max_payload_bytes: usize,
    /// Maximum number of elements in one collection.
    pub max_collection_items: usize,
    /// Maximum UTF-8 bytes in one string.
    pub max_string_bytes: usize,
    /// Maximum bytes in one opaque field.
    pub max_opaque_bytes: usize,
    /// Maximum nested aggregate depth.
    pub max_nesting_depth: u16,
}

impl CodecLimits {
    /// Production version-one limits.
    pub const PRODUCTION: Self = Self {
        max_frame_bytes: 16 * 1024 * 1024,
        max_payload_bytes: 16 * 1024 * 1024 - crate::HEADER_LEN,
        max_collection_items: 65_535,
        max_string_bytes: 1024 * 1024,
        max_opaque_bytes: 8 * 1024 * 1024,
        max_nesting_depth: 64,
    };

    /// Creates explicit limits, useful for constrained callers and boundary tests.
    #[must_use]
    pub const fn new(
        max_frame_bytes: usize,
        max_payload_bytes: usize,
        max_collection_items: usize,
        max_string_bytes: usize,
        max_opaque_bytes: usize,
        max_nesting_depth: u16,
    ) -> Self {
        Self {
            max_frame_bytes,
            max_payload_bytes,
            max_collection_items,
            max_string_bytes,
            max_opaque_bytes,
            max_nesting_depth,
        }
    }
}

impl Default for CodecLimits {
    fn default() -> Self {
        Self::PRODUCTION
    }
}
