//! Canonical family-84 complete debugger checkpoints.

use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError,
};
use peritus_types::{EventId, Sha256Digest};

use crate::{DebuggerJobId, DebuggerState};

/// Canonical inert family-84 schema-v1 debugger state frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DebuggerStateFrame(DebuggerState);

impl DebuggerStateFrame {
    /// Clones complete authoritative state into an inert frame.
    #[must_use]
    pub fn from_state(state: &DebuggerState) -> Self {
        Self(state.clone())
    }
    /// Consumes the checked frame.
    #[must_use]
    pub fn into_state(self) -> DebuggerState {
        self.0
    }
    /// Exact complete-state equality.
    #[must_use]
    pub fn matches_state(&self, state: &DebuggerState) -> bool {
        &self.0 == state
    }
    /// Job identity.
    #[must_use]
    pub const fn job_id(&self) -> DebuggerJobId {
        self.0.job_id()
    }
    /// Event sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.0.sequence()
    }
    /// Aggregate head event.
    #[must_use]
    pub const fn last_event_id(&self) -> EventId {
        self.0.last_event_id()
    }
    /// Complete state digest.
    #[must_use]
    pub const fn state_digest(&self) -> Sha256Digest {
        self.0.state_digest()
    }
}

impl CanonicalEncode for DebuggerStateFrame {
    const FAMILY: u16 = 84;
    const SCHEMA_VERSION: u16 = 1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        writer.write_bytes(&self.0.canonical_bytes().map_err(super::scalar::semantic)?)
    }
}

impl CanonicalDecode for DebuggerStateFrame {
    const FAMILY: u16 = 84;
    const SCHEMA_VERSION: u16 = 1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        DebuggerState::decode_canonical(reader.read_bytes()?)
            .map(Self)
            .map_err(super::scalar::semantic)
    }
}
