//! Negotiation and shared protocol field metadata groups.

mod handshake;
mod version_limits;

pub(super) use handshake::HANDSHAKE_TYPES;
pub(super) use version_limits::VERSION_LIMIT_TYPES;
