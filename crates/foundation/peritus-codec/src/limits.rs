//! Shared encoder and decoder contracts, separate from canonical representation widths.

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
    /// Maximum nested aggregate depth; [`Self::UNLIMITED_NESTING`] selects no depth quota.
    pub max_nesting_depth: u16,
}

impl CodecLimits {
    /// Sentinel for schema-owned nesting without a codec work quota.
    ///
    /// The private traversal counter is host-sized, so this value does not introduce a
    /// replacement 65,535-aggregate quota. Explicit smaller peer contracts remain exact.
    pub const UNLIMITED_NESTING: u16 = u16::MAX;

    /// Production codec contract without logical byte, item, or nesting allowances.
    ///
    /// Version-one fields and frame payloads retain their exact `u32` representation. Checked
    /// arithmetic and fallible allocation report actual representation or capacity failures;
    /// these values do not ask the decoder to allocate a declared length.
    pub const PRODUCTION: Self = Self {
        max_frame_bytes: usize::MAX,
        max_payload_bytes: usize::MAX - crate::HEADER_LEN,
        max_collection_items: usize::MAX,
        max_string_bytes: usize::MAX - crate::HEADER_LEN,
        max_opaque_bytes: usize::MAX - crate::HEADER_LEN,
        max_nesting_depth: Self::UNLIMITED_NESTING,
    };

    /// Frozen former default for legacy format classification and compatibility checks.
    ///
    /// This is never the implicit daily-driving contract. A legacy peer may explicitly select
    /// it through the existing codec negotiation fields without changing canonical bytes.
    pub const LEGACY_V1: Self = Self {
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
