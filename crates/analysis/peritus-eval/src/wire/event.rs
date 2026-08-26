//! Inert canonical family-86 evaluation event frames.

use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError,
};
use peritus_types::{CommandId, EventId, Sha256Digest};

use crate::{
    EvaluationCampaignId, EvaluationError, EvaluationEvent, EvaluationEventKind, EvaluationState,
    ProfileDigest, apply_event,
};

/// Canonical inert family-86 schema-v1 event frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvaluationEventFrame {
    event_id: EventId,
    command_id: CommandId,
    campaign_id: EvaluationCampaignId,
    sequence: u64,
    previous_event: Option<EventId>,
    prior_state_digest: Sha256Digest,
    profile_digest: ProfileDigest,
    command_digest: Sha256Digest,
    successor_state_digest: Sha256Digest,
    kind_bytes: Vec<u8>,
}

impl EvaluationEventFrame {
    /// Converts one accepted event to inert family-86 data.
    ///
    /// # Errors
    /// Returns a codec error if the semantic event exceeds canonical limits.
    pub fn from_event(event: &EvaluationEvent) -> Result<Self, CodecError> {
        let EvaluationEventKind::Accepted(kind) = event.kind();
        Ok(Self {
            event_id: event.id(),
            command_id: event.command_id(),
            campaign_id: event.campaign_id(),
            sequence: event.sequence(),
            previous_event: event.previous_event(),
            prior_state_digest: event.prior_state_digest(),
            profile_digest: event.profile_digest(),
            command_digest: event.command_digest(),
            successor_state_digest: event.successor_state_digest(),
            kind_bytes: super::semantic::encode(kind).map_err(super::scalar::semantic)?,
        })
    }
    /// Activates event data only through exact predecessor replay.
    ///
    /// # Errors
    /// Rejects malformed semantics, broken fences, or successor drift.
    pub fn check(
        self,
        prior: Option<&EvaluationState>,
    ) -> Result<EvaluationEvent, EvaluationError> {
        let kind = super::semantic::decode(&self.kind_bytes)?;
        let event = EvaluationEvent::new(
            self.event_id,
            self.command_id,
            self.campaign_id,
            self.sequence,
            self.previous_event,
            self.prior_state_digest,
            self.profile_digest,
            self.command_digest,
            self.successor_state_digest,
            EvaluationEventKind::Accepted(kind),
        );
        let _ = apply_event(prior, &event)?;
        Ok(event)
    }
    /// Event identity without semantic activation.
    #[must_use]
    pub const fn event_id(&self) -> EventId {
        self.event_id
    }
    /// Aggregate sequence without semantic activation.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
}

impl CanonicalEncode for EvaluationEventFrame {
    const FAMILY: u16 = 86;
    const SCHEMA_VERSION: u16 = 1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        writer.write_fixed(self.event_id.as_bytes())?;
        writer.write_fixed(self.command_id.as_bytes())?;
        writer.write_fixed(self.campaign_id.as_bytes())?;
        writer.write_u64(self.sequence)?;
        writer.write_option_tag(self.previous_event.is_some())?;
        if let Some(value) = self.previous_event {
            writer.write_fixed(value.as_bytes())?;
        }
        writer.write_fixed(self.prior_state_digest.as_bytes())?;
        writer.write_fixed(self.profile_digest.as_bytes())?;
        writer.write_fixed(self.command_digest.as_bytes())?;
        writer.write_fixed(self.successor_state_digest.as_bytes())?;
        writer.write_bytes(&self.kind_bytes)
    }
}

impl CanonicalDecode for EvaluationEventFrame {
    const FAMILY: u16 = 86;
    const SCHEMA_VERSION: u16 = 1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let event_id = super::scalar::event_id(reader)?;
        let command_id = super::scalar::command_id(reader)?;
        let campaign_id = super::scalar::campaign_id(reader)?;
        let sequence = reader.read_u64()?;
        let previous_event =
            reader.read_option_tag()?.then(|| super::scalar::event_id(reader)).transpose()?;
        if sequence == 0 || (sequence == 1) != previous_event.is_none() {
            return Err(super::scalar::invalid(reader));
        }
        let prior_state_digest = super::scalar::digest(reader)?;
        let profile_digest = ProfileDigest::new(super::scalar::digest(reader)?);
        let command_digest = super::scalar::digest(reader)?;
        let successor_state_digest = super::scalar::digest(reader)?;
        let kind_bytes = reader.read_bytes_owned()?;
        let _ = super::semantic::decode(&kind_bytes).map_err(super::scalar::semantic)?;
        Ok(Self {
            event_id,
            command_id,
            campaign_id,
            sequence,
            previous_event,
            prior_state_digest,
            profile_digest,
            command_digest,
            successor_state_digest,
            kind_bytes,
        })
    }
}
