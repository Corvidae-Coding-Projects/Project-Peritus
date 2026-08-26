//! Canonical family-87 complete evaluation checkpoints.

use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError,
};
use peritus_types::{EventId, Sha256Digest};

use crate::{EvaluationCampaignId, EvaluationState};

/// Canonical inert family-87 schema-v1 complete state frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvaluationStateFrame(EvaluationState);

impl EvaluationStateFrame {
    /// Clones complete authoritative state into an inert frame.
    #[must_use]
    pub fn from_state(state: &EvaluationState) -> Self {
        Self(state.clone())
    }
    /// Consumes the checked frame.
    #[must_use]
    pub fn into_state(self) -> EvaluationState {
        self.0
    }
    /// Exact complete-state equality.
    #[must_use]
    pub fn matches_state(&self, state: &EvaluationState) -> bool {
        &self.0 == state
    }
    /// Campaign identity.
    #[must_use]
    pub const fn campaign_id(&self) -> EvaluationCampaignId {
        self.0.campaign_id()
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

impl CanonicalEncode for EvaluationStateFrame {
    const FAMILY: u16 = 87;
    const SCHEMA_VERSION: u16 = 1;
    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        writer.write_bytes(&self.0.canonical_bytes().map_err(super::scalar::semantic)?)
    }
}

impl CanonicalDecode for EvaluationStateFrame {
    const FAMILY: u16 = 87;
    const SCHEMA_VERSION: u16 = 1;
    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        EvaluationState::decode_canonical(reader.read_bytes()?)
            .map(Self)
            .map_err(super::scalar::semantic)
    }
}
