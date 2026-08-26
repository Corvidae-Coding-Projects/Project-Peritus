//! Inert canonical family-85 evaluation command frames.

use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
};
use peritus_types::{CommandId, EventId, Sha256Digest};

use crate::{EvaluationCampaignId, EvaluationCommand, EvaluationError, ProfileDigest};

/// Canonical inert family-85 schema-v1 command frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvaluationCommandFrame {
    command_id: CommandId,
    event_id: EventId,
    campaign_id: EvaluationCampaignId,
    expected_sequence: u64,
    expected_previous_event: Option<EventId>,
    prior_state_digest: Sha256Digest,
    profile_digest: ProfileDigest,
    command_digest: Sha256Digest,
    kind_bytes: Vec<u8>,
}

impl EvaluationCommandFrame {
    /// Converts one checked command into inert family-85 data.
    ///
    /// # Errors
    /// Returns a codec error if the semantic command exceeds canonical limits.
    pub fn from_command(command: &EvaluationCommand) -> Result<Self, CodecError> {
        Ok(Self {
            command_id: command.command_id(),
            event_id: command.event_id(),
            campaign_id: command.campaign_id(),
            expected_sequence: command.expected_sequence(),
            expected_previous_event: command.expected_previous_event(),
            prior_state_digest: command.prior_state_digest(),
            profile_digest: command.profile_digest(),
            command_digest: command.digest(),
            kind_bytes: super::semantic::encode(command.kind()).map_err(super::scalar::semantic)?,
        })
    }
    /// Activates inert data through the complete semantic constructor.
    ///
    /// # Errors
    /// Rejects malformed semantics or command-digest disagreement.
    pub fn check(self) -> Result<EvaluationCommand, EvaluationError> {
        let kind = super::semantic::decode(&self.kind_bytes)?;
        let command = EvaluationCommand::new(
            self.command_id,
            self.event_id,
            self.campaign_id,
            self.expected_sequence,
            self.expected_previous_event,
            self.prior_state_digest,
            self.profile_digest,
            kind,
        )?;
        if command.digest() != self.command_digest {
            return Err(EvaluationError::new(
                crate::EvaluationErrorKind::Binding,
                crate::EvaluationOperation::Codec,
                crate::EvaluationRecovery::Quarantine,
                "family-85 command digest disagrees with semantic data",
            ));
        }
        Ok(command)
    }
    /// Command identity without semantic activation.
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
}

impl CanonicalEncode for EvaluationCommandFrame {
    const FAMILY: u16 = 85;
    const SCHEMA_VERSION: u16 = 1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        writer.write_fixed(self.command_id.as_bytes())?;
        writer.write_fixed(self.event_id.as_bytes())?;
        writer.write_fixed(self.campaign_id.as_bytes())?;
        writer.write_u64(self.expected_sequence)?;
        writer.write_option_tag(self.expected_previous_event.is_some())?;
        if let Some(value) = self.expected_previous_event {
            writer.write_fixed(value.as_bytes())?;
        }
        writer.write_fixed(self.prior_state_digest.as_bytes())?;
        writer.write_fixed(self.profile_digest.as_bytes())?;
        writer.write_fixed(self.command_digest.as_bytes())?;
        writer.write_bytes(&self.kind_bytes)
    }
}

impl CanonicalDecode for EvaluationCommandFrame {
    const FAMILY: u16 = 85;
    const SCHEMA_VERSION: u16 = 1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let command_id = super::scalar::command_id(reader)?;
        let event_id = super::scalar::event_id(reader)?;
        let campaign_id = super::scalar::campaign_id(reader)?;
        let expected_sequence = reader.read_u64()?;
        let expected_previous_event =
            reader.read_option_tag()?.then(|| super::scalar::event_id(reader)).transpose()?;
        if (expected_sequence == 0) != expected_previous_event.is_none() {
            return Err(super::scalar::invalid(reader));
        }
        let prior_state_digest = super::scalar::digest(reader)?;
        let profile_digest = ProfileDigest::new(super::scalar::digest(reader)?);
        let command_digest = super::scalar::digest(reader)?;
        let offset = reader.offset();
        let kind_bytes = reader.read_bytes_owned()?;
        if kind_bytes.is_empty() {
            return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, offset));
        }
        let _ = super::semantic::decode(&kind_bytes).map_err(super::scalar::semantic)?;
        Ok(Self {
            command_id,
            event_id,
            campaign_id,
            expected_sequence,
            expected_previous_event,
            prior_state_digest,
            profile_digest,
            command_digest,
            kind_bytes,
        })
    }
}
