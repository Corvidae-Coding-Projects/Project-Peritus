//! Explicit identities retained across request settlement and reconnect.

use peritus_app_protocol::{CorrelationId, RequestId};

use crate::error::{ClientError, ClientErrorKind};

/// Exact transport identifiers for one application request.
///
/// This identity is independent of any payload-level operation/idempotency key.
/// Reconnecting must not silently replace the latter or repeat an uncertain effect.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestIdentity {
    /// Identifies the request whose response is expected.
    pub request_id: RequestId,
    /// Correlates the response with the originating request.
    pub correlation_id: CorrelationId,
}

impl RequestIdentity {
    /// Allocates fresh identifiers using operating-system randomness.
    ///
    /// # Errors
    /// Returns an identity error if randomness is unavailable, or a protocol error
    /// if the generated value cannot be represented by the protocol.
    pub fn generate() -> Result<Self, ClientError> {
        Ok(Self {
            request_id: RequestId::new(nonzero_id()?)?,
            correlation_id: CorrelationId::new(nonzero_id()?)?,
        })
    }

    /// Restores a previously retained request binding without generating new IDs.
    #[must_use]
    pub const fn new(request_id: RequestId, correlation_id: CorrelationId) -> Self {
        Self { request_id, correlation_id }
    }
}

pub fn nonzero_id() -> Result<[u8; 16], ClientError> {
    // Bound the extremely unlikely all-zero retry rather than admitting a sentinel.
    for _ in 0..4 {
        let mut bytes = [0_u8; 16];
        getrandom::fill(&mut bytes).map_err(|error| {
            ClientError::new(
                ClientErrorKind::Identity,
                "generate application identity",
                error.to_string(),
            )
        })?;
        if bytes != [0; 16] {
            return Ok(bytes);
        }
    }
    Err(ClientError::new(
        ClientErrorKind::Identity,
        "generate application identity",
        "randomness source repeatedly returned the reserved zero identity",
    ))
}

#[cfg(test)]
mod tests {
    use super::RequestIdentity;

    #[test]
    fn restoring_a_binding_preserves_both_identifiers() {
        let request = peritus_app_protocol::RequestId::new([1; 16]).unwrap();
        let correlation = peritus_app_protocol::CorrelationId::new([2; 16]).unwrap();
        let restored = RequestIdentity::new(request, correlation);
        assert_eq!(restored.request_id, request);
        assert_eq!(restored.correlation_id, correlation);
    }

    #[test]
    fn independently_generated_requests_have_distinct_bindings() {
        let first = RequestIdentity::generate().unwrap();
        let second = RequestIdentity::generate().unwrap();
        assert_ne!(first.request_id, second.request_id);
        assert_ne!(first.correlation_id, second.correlation_id);
    }
}
