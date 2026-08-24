//! Checked public names and redacted provider identities.

mod public;
mod sensitive;

use peritus_types::Sha256Digest;

use crate::{ProtocolError, ProtocolErrorKind};

pub use public::{ExtensionName, ModelName, OutputName, ProviderName, ToolName};
pub use sensitive::{CacheKey, EventId, IdempotencyKey, ItemId, RequestId, ResponseId, ToolCallId};

#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct CheckedIdentity(String);

impl CheckedIdentity {
    fn new(value: String, maximum: usize, path: &'static str) -> Result<Self, ProtocolError> {
        if value.is_empty()
            || value.len() > maximum
            || value.contains('\0')
            || value.chars().any(char::is_control)
        {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidIdentity,
                path,
                "identity is empty, contains controls, or exceeds its byte bound",
            ));
        }
        Ok(Self(value))
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

/// Digest of exact canonical request bytes.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RequestFingerprint(Sha256Digest);

impl RequestFingerprint {
    /// Wraps a canonical SHA-256 digest.
    #[must_use]
    pub const fn new(digest: Sha256Digest) -> Self {
        Self(digest)
    }

    /// Returns the digest.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.0
    }
}
