//! Fixed JSON parsing and validation ceilings.

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
    /// Production call/schema limits.
    pub const PRODUCTION: Self = Self {
        max_bytes: 256 * 1024,
        max_depth: 32,
        max_members: 4096,
        max_string_bytes: 64 * 1024,
    };

    /// Creates explicit nonzero JSON limits.
    ///
    /// # Errors
    ///
    /// Returns an envelope error when any limit is zero or widens the production ceiling.
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
            || max_bytes > Self::PRODUCTION.max_bytes
            || max_depth > Self::PRODUCTION.max_depth
            || max_members > Self::PRODUCTION.max_members
            || max_string_bytes > Self::PRODUCTION.max_string_bytes
        {
            return Err(crate::ProtocolError::at(
                crate::ProtocolErrorKind::InvalidEnvelope,
                "json_limits",
                "JSON limits must be nonzero and no wider than production ceilings",
            ));
        }
        Ok(Self { max_bytes, max_depth, max_members, max_string_bytes })
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
