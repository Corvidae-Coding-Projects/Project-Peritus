//! Bounded test-only discovery adapters shared by libFuzzer and stable corpus replay.

mod framing;
mod provider;
mod working;

/// Maximum accepted corpus input size.
pub const MAX_INPUT_BYTES: usize = 8192;

/// Registered discovery targets; no unknown target can silently become a no-op.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscoveryTarget {
    /// Raw SSE framing and chunk partition invariance.
    Sse,
    /// Raw NDJSON framing and chunk partition invariance.
    Ndjson,
    /// Structured working graphs, capacity, and checkpoint decoding.
    WorkingState,
    /// Structured normalized provider event sequences and reducer invariants.
    ProviderSequence,
}

impl DiscoveryTarget {
    /// Parses a registered target name.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "sse" => Some(Self::Sse),
            "ndjson" => Some(Self::Ndjson),
            "working_state" => Some(Self::WorkingState),
            "provider_sequence" => Some(Self::ProviderSequence),
            _ => None,
        }
    }
}

/// Exercises a production boundary and asserts its observable invariants.
///
/// # Panics
/// Panics when a product invariant is violated or a caller exceeds the input ceiling.
pub fn check_input(target: DiscoveryTarget, bytes: &[u8]) {
    assert!(bytes.len() <= MAX_INPUT_BYTES, "discovery input exceeds its bound");
    match target {
        DiscoveryTarget::Sse => framing::sse(bytes),
        DiscoveryTarget::Ndjson => framing::ndjson(bytes),
        DiscoveryTarget::WorkingState => working::check(bytes),
        DiscoveryTarget::ProviderSequence => provider::check(bytes),
    }
}
