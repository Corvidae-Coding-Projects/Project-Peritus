//! Stable identities owned by the application protocol.

use core::fmt;
use peritus_types::IdentifierError;

mod invocation;
pub use invocation::WorkbenchInvocationId;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct OpaqueAppIdentifier {
    bytes: [u8; 16],
}
impl OpaqueAppIdentifier {
    const fn new(bytes: [u8; 16]) -> Result<Self, IdentifierError> {
        if bytes[0] != 0
            || bytes[1] != 0
            || bytes[2] != 0
            || bytes[3] != 0
            || bytes[4] != 0
            || bytes[5] != 0
            || bytes[6] != 0
            || bytes[7] != 0
            || bytes[8] != 0
            || bytes[9] != 0
            || bytes[10] != 0
            || bytes[11] != 0
            || bytes[12] != 0
            || bytes[13] != 0
            || bytes[14] != 0
            || bytes[15] != 0
        {
            Ok(Self { bytes })
        } else {
            Err(IdentifierError::Zero)
        }
    }
    const fn as_bytes(&self) -> &[u8; 16] {
        &self.bytes
    }
    const fn into_bytes(self) -> [u8; 16] {
        self.bytes
    }
}

mod flow;
mod inputs;
mod request;
mod runtime;
mod workbench;

pub use flow::{PromptId, SubscriptionId, TransferId};
pub use inputs::WorkbenchInputId;
pub use request::{CorrelationId, ProtocolId, RequestId};
pub use runtime::{DeliveryAttemptId, HeartbeatId, TerminalAttachmentId};
pub use workbench::{ControlOperationId, ConversationId};

/// Maximum bytes accepted in one idempotency key.
pub const MAX_IDEMPOTENCY_KEY_BYTES: usize = 128;

/// Opaque, bounded key used to recognize retries from one actor in one durable session.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct IdempotencyKey(Vec<u8>);

impl IdempotencyKey {
    /// Creates a nonempty bounded key.
    ///
    /// # Errors
    ///
    /// Returns [`IdempotencyKeyError::Empty`] for an empty key and
    /// [`IdempotencyKeyError::TooLong`] when the key exceeds 128 bytes.
    pub fn new(bytes: Vec<u8>) -> Result<Self, IdempotencyKeyError> {
        if bytes.is_empty() {
            Err(IdempotencyKeyError::Empty)
        } else if bytes.len() > MAX_IDEMPOTENCY_KEY_BYTES {
            Err(IdempotencyKeyError::TooLong)
        } else {
            Ok(Self(bytes))
        }
    }

    /// Borrows the exact opaque key bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8] {
        self.0.as_slice()
    }

    /// Consumes the key and returns its exact bytes.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}

/// Failure to construct a bounded idempotency key.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum IdempotencyKeyError {
    /// The key was empty.
    Empty,
    /// The key exceeded [`MAX_IDEMPOTENCY_KEY_BYTES`].
    TooLong,
}

impl fmt::Display for IdempotencyKeyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "idempotency key must not be empty",
            Self::TooLong => "idempotency key exceeds 128 bytes",
        })
    }
}

impl std::error::Error for IdempotencyKeyError {}
