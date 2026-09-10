//! Public host-delivered replies, never provider reasoning or mutable prompt text.

use super::{ControlError, InvocationId, OperationId};
use peritus_types::Sha256Digest;
use serde::Deserialize;
use serde::Serialize;

/// Immutable public reply artifact reference. Exact bytes belong to the control store.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicReplyReference {
    operation: OperationId,
    after_invocation: InvocationId,
    digest: [u8; 32],
    bytes: u64,
}

impl PublicReplyReference {
    /// Creates a bounded reference; publication must validate and retain the exact text bytes.
    ///
    /// # Errors
    /// Rejects empty or oversized public replies.
    pub const fn new(
        operation: OperationId,
        after_invocation: InvocationId,
        digest: [u8; 32],
        bytes: u64,
    ) -> Result<Self, ControlError> {
        if bytes == 0 || bytes > 1024 * 1024 {
            return Err(ControlError::Capacity);
        }
        Ok(Self { operation, after_invocation, digest, bytes })
    }
    /// Returns the operation that atomically published the artifact reference.
    #[must_use]
    pub const fn operation(&self) -> OperationId {
        self.operation
    }
    /// Returns the last prepared invocation preceding the delivered public reply.
    #[must_use]
    pub const fn after_invocation(&self) -> InvocationId {
        self.after_invocation
    }
    /// Returns the exact public UTF-8 artifact digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        Sha256Digest::new(self.digest)
    }
    /// Returns exact UTF-8 artifact length.
    #[must_use]
    pub const fn bytes(&self) -> u64 {
        self.bytes
    }
}
