//! Negotiated JSON frame parsing and validation bounds.

const PARSER_MAX_DEPTH: usize = 128;

/// Complete bounds applied while accepting JSON values and schemas.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_field_names,
    reason = "the max_ prefix distinguishes immutable ceilings from observed JSON measurements"
)]
pub struct JsonLimits {
    /// Maximum serialized canonical byte length.
    pub(crate) max_bytes: usize,
    /// Maximum recursive container depth, including the root.
    pub(crate) max_depth: usize,
    /// Maximum total object members and array elements.
    pub(crate) max_members: usize,
    /// Maximum UTF-8 bytes in one string or property name.
    pub(crate) max_string_bytes: usize,
}

impl JsonLimits {
    /// Historical version-one and version-two call/schema limits.
    pub const PRODUCTION: Self = Self {
        max_bytes: 256 * 1024,
        max_depth: 32,
        max_members: 4096,
        max_string_bytes: 64 * 1024,
    };

    /// Widest frame accepted by the in-process JSON representation.
    ///
    /// Byte, member, and string capacity are selected by the enclosing physical frame. Recursive
    /// depth remains bounded by the parser's concrete stack-safety contract.
    pub const MAXIMUM: Self = Self {
        max_bytes: usize::MAX,
        max_depth: PARSER_MAX_DEPTH,
        max_members: usize::MAX,
        max_string_bytes: usize::MAX,
    };

    /// Creates a physical JSON frame whose only finite capacity is its encoded byte length.
    ///
    /// Member and string counts cannot exceed the frame byte count, while depth uses the
    /// parser's concrete recursion contract.
    ///
    /// # Errors
    /// Returns an envelope error when the frame byte capacity is zero.
    pub fn frame(max_bytes: usize) -> Result<Self, crate::ProtocolError> {
        Self::new(max_bytes, PARSER_MAX_DEPTH, max_bytes, max_bytes)
    }

    /// Creates explicit nonzero JSON limits.
    ///
    /// # Errors
    ///
    /// Returns an envelope error when any limit is zero or depth exceeds the parser's physical
    /// recursion contract.
    pub fn new(
        max_bytes: usize,
        max_depth: usize,
        max_members: usize,
        max_string_bytes: usize,
    ) -> Result<Self, crate::ProtocolError> {
        if max_bytes == 0
            || max_depth == 0
            || max_members == 0
            || max_string_bytes == 0
            || max_depth > PARSER_MAX_DEPTH
        {
            return Err(crate::ProtocolError::at(
                crate::ProtocolErrorKind::InvalidEnvelope,
                "json_limits",
                "JSON frame limits must be nonzero and fit the parser depth contract",
            ));
        }
        Ok(Self { max_bytes, max_depth, max_members, max_string_bytes })
    }

    pub(crate) const fn fits(self, maximum: Self) -> bool {
        self.max_bytes <= maximum.max_bytes
            && self.max_depth <= maximum.max_depth
            && self.max_members <= maximum.max_members
            && self.max_string_bytes <= maximum.max_string_bytes
    }

    pub(crate) fn canonical_bytes(self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(32);
        bytes.extend_from_slice(&(self.max_bytes as u64).to_be_bytes());
        bytes.extend_from_slice(&(self.max_depth as u64).to_be_bytes());
        bytes.extend_from_slice(&(self.max_members as u64).to_be_bytes());
        bytes.extend_from_slice(&(self.max_string_bytes as u64).to_be_bytes());
        bytes
    }

    pub(crate) fn from_wire(
        max_bytes: u64,
        max_depth: u64,
        max_members: u64,
        max_string_bytes: u64,
    ) -> Result<Self, crate::ProtocolError> {
        Self::new(
            usize::try_from(max_bytes).map_err(|_| host_limit())?,
            usize::try_from(max_depth).map_err(|_| host_limit())?,
            usize::try_from(max_members).map_err(|_| host_limit())?,
            usize::try_from(max_string_bytes).map_err(|_| host_limit())?,
        )
    }

    /// Returns the canonical byte ceiling.
    #[must_use]
    pub const fn max_bytes(self) -> usize {
        self.max_bytes
    }

    /// Returns the recursive container-depth ceiling.
    #[must_use]
    pub const fn max_depth(self) -> usize {
        self.max_depth
    }

    /// Returns the aggregate member ceiling.
    #[must_use]
    pub const fn max_members(self) -> usize {
        self.max_members
    }

    /// Returns the per-string UTF-8 byte ceiling.
    #[must_use]
    pub const fn max_string_bytes(self) -> usize {
        self.max_string_bytes
    }
}

fn host_limit() -> crate::ProtocolError {
    crate::ProtocolError::at(
        crate::ProtocolErrorKind::InvalidEnvelope,
        "json_limits",
        "JSON frame limit is not representable on this host",
    )
}
