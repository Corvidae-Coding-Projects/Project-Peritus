//! Stable identities owned by the application protocol.

use core::fmt;
use peritus_types::IdentifierError;
use vstd::prelude::*;

verus! {

/// Returns whether an application-protocol identifier is not the reserved all-zero value.
pub open spec fn valid_app_identifier_bytes(bytes: [u8; 16]) -> bool {
    bytes[0] != 0 || bytes[1] != 0 || bytes[2] != 0 || bytes[3] != 0
        || bytes[4] != 0 || bytes[5] != 0 || bytes[6] != 0 || bytes[7] != 0
        || bytes[8] != 0 || bytes[9] != 0 || bytes[10] != 0 || bytes[11] != 0
        || bytes[12] != 0 || bytes[13] != 0 || bytes[14] != 0 || bytes[15] != 0
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct OpaqueAppIdentifier {
    bytes: [u8; 16],
}

impl OpaqueAppIdentifier {
    #[verifier::type_invariant]
    closed spec fn invariant(&self) -> bool {
        valid_app_identifier_bytes(self.bytes)
    }

    closed spec fn spec_bytes(&self) -> [u8; 16] {
        self.bytes
    }

    const fn new(bytes: [u8; 16]) -> (result: Result<Self, IdentifierError>)
        ensures
            result.is_ok() == valid_app_identifier_bytes(bytes),
            match result {
                Ok(identifier) => identifier.spec_bytes() == bytes,
                Err(_) => true,
            },
    {
        if bytes[0] != 0 || bytes[1] != 0 || bytes[2] != 0 || bytes[3] != 0
            || bytes[4] != 0 || bytes[5] != 0 || bytes[6] != 0 || bytes[7] != 0
            || bytes[8] != 0 || bytes[9] != 0 || bytes[10] != 0 || bytes[11] != 0
            || bytes[12] != 0 || bytes[13] != 0 || bytes[14] != 0 || bytes[15] != 0
        {
            Ok(Self { bytes })
        } else {
            Err(IdentifierError::Zero)
        }
    }

    const fn as_bytes(&self) -> (bytes: &[u8; 16])
        ensures *bytes == self.spec_bytes()
    {
        &self.bytes
    }

    const fn into_bytes(self) -> (bytes: [u8; 16])
        ensures bytes == self.spec_bytes()
    {
        self.bytes
    }
}

} // verus!

mod flow;
mod request;
mod runtime;

pub use flow::{PromptId, SubscriptionId, TransferId};
pub use request::{CorrelationId, ProtocolId, RequestId};
pub use runtime::{DeliveryAttemptId, HeartbeatId, TerminalAttachmentId};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nominal_id_rejects_zero_and_preserves_exact_bytes() {
        assert_eq!(ProtocolId::new([0; 16]), Err(IdentifierError::Zero));
        let bytes = [7; 16];
        assert_eq!(ProtocolId::new(bytes).unwrap().into_bytes(), bytes);
    }

    #[test]
    fn idempotency_key_is_nonempty_and_bounded() {
        assert_eq!(IdempotencyKey::new(Vec::new()), Err(IdempotencyKeyError::Empty));
        assert_eq!(
            IdempotencyKey::new(vec![0; MAX_IDEMPOTENCY_KEY_BYTES + 1]),
            Err(IdempotencyKeyError::TooLong),
        );
        assert_eq!(IdempotencyKey::new(vec![1, 2]).unwrap().as_bytes(), &[1, 2]);
    }
}
