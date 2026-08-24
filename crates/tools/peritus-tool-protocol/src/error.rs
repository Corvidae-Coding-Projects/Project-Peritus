//! Stable protocol validation errors.

use core::fmt;

const MAX_ERROR_PATH_BYTES: usize = 8 * 1024;
const MAX_ERROR_DETAIL_BYTES: usize = 4 * 1024;
const TRUNCATED_PATH_SUFFIX: &str = "...[truncated]";

/// Stable category for a protocol validation failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolErrorKind {
    /// A validated string was empty or outside its byte bound.
    InvalidText,
    /// A semantic version component or compatibility range was invalid.
    InvalidVersion,
    /// JSON syntax was malformed.
    InvalidJson,
    /// JSON exceeded a configured depth, member, string, or byte limit.
    JsonLimit,
    /// A schema was malformed or outside the supported subset.
    InvalidSchema,
    /// A value did not satisfy the complete schema.
    SchemaViolation,
    /// A descriptor disagreed with its authenticated operation.
    DescriptorMismatch,
    /// A call limit exceeded the immutable descriptor ceiling.
    CallLimit,
    /// Ordered or bounded envelope content was invalid.
    InvalidEnvelope,
}

/// Bounded typed protocol failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtocolError {
    kind: ProtocolErrorKind,
    path: String,
    detail: &'static str,
}

impl ProtocolError {
    pub(crate) const fn new(kind: ProtocolErrorKind, path: String, detail: &'static str) -> Self {
        Self { kind, path, detail }
    }

    pub(crate) fn at(kind: ProtocolErrorKind, path: &str, detail: &'static str) -> Self {
        Self { kind, path: bounded_path(path), detail: bounded_detail(detail) }
    }

    /// Creates a typed invalid-envelope error for an adapter constructor.
    ///
    /// # Errors
    ///
    /// Rejects an empty/oversized path, NUL, or oversized static detail.
    pub fn invalid_envelope(path: String, detail: &'static str) -> Result<Self, Self> {
        if path.is_empty()
            || path.len() > MAX_ERROR_PATH_BYTES
            || path.contains('\0')
            || detail.is_empty()
            || detail.len() > MAX_ERROR_DETAIL_BYTES
        {
            return Err(Self::new(
                ProtocolErrorKind::InvalidText,
                "error".to_owned(),
                "protocol error path or detail exceeds its bound",
            ));
        }
        Ok(Self::new(ProtocolErrorKind::InvalidEnvelope, path, detail))
    }

    /// Returns the stable failure category.
    #[must_use]
    pub const fn kind(&self) -> ProtocolErrorKind {
        self.kind
    }

    /// Returns the bounded JSON/schema path or field name.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns a stable diagnostic detail.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        self.detail
    }

    /// Returns stable version-one canonical protocol-error bytes.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = crate::wire::begin(8);
        bytes.push(match self.kind {
            ProtocolErrorKind::InvalidText => 1,
            ProtocolErrorKind::InvalidVersion => 2,
            ProtocolErrorKind::InvalidJson => 3,
            ProtocolErrorKind::JsonLimit => 4,
            ProtocolErrorKind::InvalidSchema => 5,
            ProtocolErrorKind::SchemaViolation => 6,
            ProtocolErrorKind::DescriptorMismatch => 7,
            ProtocolErrorKind::CallLimit => 8,
            ProtocolErrorKind::InvalidEnvelope => 9,
        });
        crate::wire::text(&mut bytes, &self.path);
        crate::wire::text(&mut bytes, self.detail);
        bytes
    }
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.path, self.detail)
    }
}

impl std::error::Error for ProtocolError {}

fn bounded_path(path: &str) -> String {
    if path.len() <= MAX_ERROR_PATH_BYTES {
        return path.to_owned();
    }

    let mut end = MAX_ERROR_PATH_BYTES - TRUNCATED_PATH_SUFFIX.len();
    while !path.is_char_boundary(end) {
        end -= 1;
    }
    let mut bounded = String::with_capacity(MAX_ERROR_PATH_BYTES);
    bounded.push_str(&path[..end]);
    bounded.push_str(TRUNCATED_PATH_SUFFIX);
    bounded
}

const fn bounded_detail(detail: &'static str) -> &'static str {
    if detail.len() <= MAX_ERROR_DETAIL_BYTES {
        detail
    } else {
        "protocol error detail exceeds its bound"
    }
}
