//! Stable model-protocol validation failures.

use core::fmt;

const MAX_PATH_BYTES: usize = 512;

/// Stable category for rejected C5 protocol data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ProtocolErrorKind {
    /// A protocol version is unsupported.
    UnsupportedVersion,
    /// A name or identity is empty, malformed, or too large.
    InvalidIdentity,
    /// A configured bound is zero or wider than the production ceiling.
    InvalidLimit,
    /// A requested capability is unsupported or unknown.
    UnsupportedCapability,
    /// A capability/profile combination contradicts itself.
    InvalidProfile,
    /// Message or content structure is invalid.
    InvalidContent,
    /// A JSON value or schema is malformed or outside its bounds.
    InvalidSchema,
    /// A request is inconsistent or exceeds negotiated limits.
    InvalidRequest,
    /// A normalized stream event violates ordering or uniqueness.
    InvalidEvent,
    /// A stream ended without one valid terminal outcome.
    IncompleteStream,
    /// Usage observations conflict or regress.
    InvalidUsage,
    /// A retry input or plan is outside the legal retry table.
    InvalidRetry,
}

/// Bounded failure containing only a stable diagnostic, never provider data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtocolError {
    kind: ProtocolErrorKind,
    path: String,
    detail: &'static str,
}

impl ProtocolError {
    pub(crate) fn at(kind: ProtocolErrorKind, path: &str, detail: &'static str) -> Self {
        Self { kind, path: bounded_path(path), detail }
    }

    /// Returns the stable failure category.
    #[must_use]
    pub const fn kind(&self) -> ProtocolErrorKind {
        self.kind
    }

    /// Returns the bounded logical field path.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns the static redaction-safe diagnostic.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        self.detail
    }
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.path, self.detail)
    }
}

impl std::error::Error for ProtocolError {}

fn bounded_path(path: &str) -> String {
    if path.len() <= MAX_PATH_BYTES {
        return path.to_owned();
    }
    let mut end = MAX_PATH_BYTES;
    while !path.is_char_boundary(end) {
        end -= 1;
    }
    path[..end].to_owned()
}
