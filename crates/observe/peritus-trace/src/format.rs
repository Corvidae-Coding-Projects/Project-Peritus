//! Stable trace wire-format identity.

use peritus_types::Sha256Digest;

/// Canonical B3 family tag for inert trace observations.
pub const TRACE_OBSERVATION_FAMILY: u16 = 60;
/// Canonical trace-observation schema version.
pub const TRACE_OBSERVATION_SCHEMA: u16 = 1;

/// Returns the immutable schema digest bound into every C0 trace record.
#[must_use]
pub fn trace_schema_digest() -> Sha256Digest {
    peritus_codec::sha256(b"PERITUS-C7-TRACE-OBSERVATION-SCHEMA-V1\0")
}
