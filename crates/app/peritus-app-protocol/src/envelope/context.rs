//! Exact negotiated relationship and durable-session binding.

use crate::{ProtocolId, ProtocolVersion};
use peritus_types::SessionId;

/// Common context carried by every post-negotiation application frame.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProtocolContext {
    protocol_id: ProtocolId,
    version: ProtocolVersion,
    session_id: SessionId,
}

impl ProtocolContext {
    /// Creates an exact post-negotiation context.
    #[must_use]
    pub const fn new(
        protocol_id: ProtocolId,
        version: ProtocolVersion,
        session_id: SessionId,
    ) -> Self {
        Self { protocol_id, version, session_id }
    }

    /// Returns the negotiated relationship identity.
    #[must_use]
    pub const fn protocol_id(self) -> ProtocolId {
        self.protocol_id
    }
    /// Returns the negotiated application version.
    #[must_use]
    pub const fn version(self) -> ProtocolVersion {
        self.version
    }
    /// Returns the durable user session identity.
    #[must_use]
    pub const fn session_id(self) -> SessionId {
        self.session_id
    }
}
