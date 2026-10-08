//! Versioned canonical encoding for normalized provider event envelopes.

mod decode;
mod encode;
mod page;
mod primitive;

pub use decode::decode_event_envelope;
pub use encode::encode_event_envelope;
pub use page::{
    EventArchivePage, decode_event_archive_page, encode_event_archive_page, is_event_archive_page,
};

/// Canonical normalized-event schema version.
pub const EVENT_ENVELOPE_SCHEMA_VERSION: u16 = 1;
