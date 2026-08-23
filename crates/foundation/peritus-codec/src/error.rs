//! Stable codec failure vocabulary.

use core::fmt;

/// Bounded resource checked by an encoding or decoding operation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CodecLimit {
    /// Complete frame bytes.
    FrameBytes,
    /// Payload bytes inside one frame.
    PayloadBytes,
    /// Items in one collection.
    CollectionItems,
    /// Bytes in one UTF-8 string.
    StringBytes,
    /// Bytes in one opaque value.
    OpaqueBytes,
    /// Nested aggregate depth.
    NestingDepth,
}

/// Stable machine-actionable codec failure class.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CodecErrorKind {
    /// A configured resource limit was exceeded.
    LimitExceeded,
    /// A length cannot be represented or added safely.
    LengthOverflow,
    /// Input ended before a declared value was complete.
    Truncated,
    /// A boolean tag was not zero or one.
    InvalidBoolean,
    /// An option tag was not zero or one.
    InvalidOption,
    /// A string was not valid UTF-8.
    InvalidUtf8,
    /// The frame magic was not `PRTS`.
    InvalidMagic,
    /// The codec format version is unsupported.
    UnsupportedFormatVersion,
    /// A frame used the reserved zero family tag.
    InvalidFamily,
    /// A frame used the reserved zero schema version.
    InvalidSchemaVersion,
    /// Reserved frame flags were nonzero.
    NonzeroFlags,
    /// Bytes remained after the declared value.
    TrailingBytes,
    /// A frame family does not match the requested message type.
    WrongFamily,
    /// A schema version does not match the requested message type.
    WrongSchemaVersion,
    /// A closed protocol enum used an unassigned discriminant.
    UnknownTag,
    /// A decoded primitive could not form its checked domain value.
    InvalidDomainValue,
}

/// Typed codec failure with the exact byte offset and optional violated limit.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CodecError {
    kind: CodecErrorKind,
    offset: usize,
    limit: Option<CodecLimit>,
}

impl CodecError {
    pub(crate) const fn new(kind: CodecErrorKind, offset: usize) -> Self {
        Self { kind, offset, limit: None }
    }

    pub(crate) const fn limited(offset: usize, limit: CodecLimit) -> Self {
        Self { kind: CodecErrorKind::LimitExceeded, offset, limit: Some(limit) }
    }

    /// Creates a non-limit protocol validation failure at an exact byte offset.
    ///
    /// This is intentionally available to protocol crates that build closed enums and checked
    /// domain values on top of the primitive codec. Resource-limit failures remain owned by the
    /// codec because they must carry a [`CodecLimit`].
    #[must_use]
    pub const fn at(kind: CodecErrorKind, offset: usize) -> Self {
        Self { kind, offset, limit: None }
    }

    /// Returns the stable failure class.
    #[must_use]
    pub const fn kind(self) -> CodecErrorKind {
        self.kind
    }

    /// Returns the absolute byte offset at which validation failed.
    #[must_use]
    pub const fn offset(self) -> usize {
        self.offset
    }

    /// Returns the violated resource dimension when the failure is limit-related.
    #[must_use]
    pub const fn limit(self) -> Option<CodecLimit> {
        self.limit
    }
}

impl fmt::Display for CodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("codec ")?;
        if let Some(limit) = self.limit {
            fmt::Debug::fmt(&limit, formatter)?;
            formatter.write_str(" limit exceeded at byte ")?;
        } else {
            fmt::Debug::fmt(&self.kind, formatter)?;
            formatter.write_str(" at byte ")?;
        }
        fmt::Display::fmt(&self.offset, formatter)
    }
}

impl std::error::Error for CodecError {}
