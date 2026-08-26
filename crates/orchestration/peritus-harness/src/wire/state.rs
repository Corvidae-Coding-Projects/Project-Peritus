//! Canonical family-81 complete harness checkpoints.

use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
};
use peritus_types::{EventId, HarnessId, Sha256Digest};

use crate::aggregate::{AggregateError, HarnessState};

/// Canonical family-81 schema-v1 complete aggregate checkpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HarnessStateFrame(HarnessState);

impl HarnessStateFrame {
    /// Clones one complete authoritative state into an inert transport frame.
    #[must_use]
    pub fn from_state(state: &HarnessState) -> Self {
        Self(state.clone())
    }
    /// Consumes the checked frame into complete state.
    #[must_use]
    pub fn into_state(self) -> HarnessState {
        self.0
    }
    /// Returns whether every decoded field equals authoritative state.
    #[must_use]
    pub fn matches_state(&self, state: &HarnessState) -> bool {
        &self.0 == state
    }
    /// Returns harness identity.
    #[must_use]
    pub const fn harness_id(&self) -> HarnessId {
        self.0.harness_id()
    }
    /// Returns event sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.0.sequence()
    }
    /// Returns aggregate head event.
    #[must_use]
    pub const fn last_event_id(&self) -> EventId {
        self.0.last_event_id()
    }
    /// Returns complete state digest.
    #[must_use]
    pub const fn state_digest(&self) -> Sha256Digest {
        self.0.state_digest()
    }
}

impl CanonicalEncode for HarnessStateFrame {
    const FAMILY: u16 = 81;
    const SCHEMA_VERSION: u16 = 1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        writer.write_bytes(&self.0.canonical_bytes().map_err(codec)?)
    }
}

impl CanonicalDecode for HarnessStateFrame {
    const FAMILY: u16 = 81;
    const SCHEMA_VERSION: u16 = 1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        HarnessState::decode_canonical(reader.read_bytes()?).map(Self).map_err(codec)
    }
}

fn codec(_error: AggregateError) -> CodecError {
    CodecError::at(CodecErrorKind::InvalidDomainValue, 0)
}
