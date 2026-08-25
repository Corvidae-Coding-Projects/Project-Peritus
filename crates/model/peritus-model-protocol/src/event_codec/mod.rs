//! Versioned canonical encoding for normalized provider event envelopes.

mod decode;
mod encode;
mod primitive;

pub use decode::decode_event_envelope;
pub use encode::encode_event_envelope;

/// Canonical normalized-event schema version.
pub const EVENT_ENVELOPE_SCHEMA_VERSION: u16 = 1;
