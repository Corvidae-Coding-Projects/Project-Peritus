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
