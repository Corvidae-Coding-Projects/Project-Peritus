//! Canonical campaign command, event, and complete-state families 88-90.

use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
};
use peritus_types::{CommandId, EventId, Sha256Digest};

use crate::{
    CampaignCommand, CampaignEvent, CampaignEventKind, CampaignState, EvolutionCampaignId,
    EvolutionError, apply_campaign_event,
};

/// Canonical inert family-88 schema-v1 campaign command frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignCommandFrame {
    command_id: CommandId,
    event_id: EventId,
    campaign_id: EvolutionCampaignId,
    expected_sequence: u64,
    expected_head: Option<EventId>,
    prior_state_digest: Sha256Digest,
    policy_digest: Sha256Digest,
    command_digest: Sha256Digest,
    kind_bytes: Vec<u8>,
}

impl CampaignCommandFrame {
    /// Converts one checked command into family-88 data.
    ///
    /// # Errors
    /// Returns a codec error when the semantic command cannot be encoded canonically.
    pub fn from_command(command: &CampaignCommand) -> Result<Self, CodecError> {
        Ok(Self {
            command_id: command.command_id(),
            event_id: command.event_id(),
            campaign_id: command.campaign_id(),
            expected_sequence: command.expected_sequence(),
            expected_head: command.expected_head(),
            prior_state_digest: command.prior_state_digest(),
            policy_digest: command.policy_digest(),
            command_digest: command.digest(),
            kind_bytes: super::semantic::encode_campaign_kind(command.kind())
                .map_err(super::scalar::semantic)?,
        })
    }
    /// Reconstructs and verifies the complete semantic command.
    ///
    /// # Errors
    /// Returns an evolution error when semantic bytes or their command digest are invalid.
    pub fn into_command(self) -> Result<CampaignCommand, EvolutionError> {
        let kind = super::semantic::decode_campaign_kind(&self.kind_bytes)?;
        let command = CampaignCommand::new(
            self.command_id,
            self.event_id,
            self.campaign_id,
            self.expected_sequence,
            self.expected_head,
            self.prior_state_digest,
            self.policy_digest,
            kind,
        )?;
        if command.digest() != self.command_digest {
            return Err(corrupt());
        }
        Ok(command)
    }
    /// Stable command identity without semantic activation.
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
}

impl CanonicalEncode for CampaignCommandFrame {
    const FAMILY: u16 = 88;
    const SCHEMA_VERSION: u16 = 1;
    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        writer.write_fixed(self.command_id.as_bytes())?;
        writer.write_fixed(self.event_id.as_bytes())?;
        writer.write_fixed(self.campaign_id.as_bytes())?;
        writer.write_u64(self.expected_sequence)?;
        write_event_option(writer, self.expected_head)?;
        writer.write_fixed(self.prior_state_digest.as_bytes())?;
        writer.write_fixed(self.policy_digest.as_bytes())?;
        writer.write_fixed(self.command_digest.as_bytes())?;
        writer.write_bytes(&self.kind_bytes)
    }
}

impl CanonicalDecode for CampaignCommandFrame {
    const FAMILY: u16 = 88;
    const SCHEMA_VERSION: u16 = 1;
    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let command_id = super::scalar::command_id(reader)?;
        let event_id = super::scalar::event_id(reader)?;
        let campaign_id = super::scalar::campaign_id(reader)?;
        let expected_sequence = reader.read_u64()?;
        let expected_head = read_event_option(reader)?;
        if (expected_sequence == 0) != expected_head.is_none() {
            return Err(super::scalar::invalid(reader));
        }
        let prior_state_digest = super::scalar::digest(reader).map_err(super::scalar::semantic)?;
        let policy_digest = super::scalar::digest(reader).map_err(super::scalar::semantic)?;
        let command_digest = super::scalar::digest(reader).map_err(super::scalar::semantic)?;
        let kind_bytes = read_semantic(
            reader,
            super::semantic::decode_campaign_kind,
            super::semantic::encode_campaign_kind,
        )?;
        Ok(Self {
            command_id,
            event_id,
            campaign_id,
            expected_sequence,
            expected_head,
            prior_state_digest,
            policy_digest,
            command_digest,
            kind_bytes,
        })
    }
}

/// Canonical inert family-89 schema-v1 campaign event frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignEventFrame {
    event_id: EventId,
    command_id: CommandId,
    campaign_id: EvolutionCampaignId,
    sequence: u64,
    previous_event: Option<EventId>,
    prior_state_digest: Sha256Digest,
    policy_digest: Sha256Digest,
    command_digest: Sha256Digest,
    successor_state_digest: Sha256Digest,
    kind_bytes: Vec<u8>,
}

impl CampaignEventFrame {
    /// Converts one accepted campaign event into family-89 data.
    ///
    /// # Errors
    /// Returns a codec error when the accepted event kind cannot be encoded canonically.
    pub fn from_event(event: &CampaignEvent) -> Result<Self, CodecError> {
        let CampaignEventKind::Accepted(kind) = event.kind();
        Ok(Self {
            event_id: event.id(),
            command_id: event.command_id(),
            campaign_id: event.campaign_id(),
            sequence: event.sequence(),
            previous_event: event.previous_event(),
            prior_state_digest: event.prior_state_digest(),
            policy_digest: event.policy_digest(),
            command_digest: event.command_digest(),
            successor_state_digest: event.successor_state_digest(),
            kind_bytes: super::semantic::encode_campaign_kind(kind)
                .map_err(super::scalar::semantic)?,
        })
    }
    /// Reconstructs an event and verifies its producing command and exact predecessor replay.
    ///
    /// # Errors
    /// Returns an evolution error for malformed semantics, digest drift, or invalid replay.
    pub fn check(self, prior: Option<&CampaignState>) -> Result<CampaignEvent, EvolutionError> {
        let kind = super::semantic::decode_campaign_kind(&self.kind_bytes)?;
        let expected_sequence = self.sequence.checked_sub(1).ok_or_else(corrupt)?;
        let command = CampaignCommand::new(
            self.command_id,
            self.event_id,
            self.campaign_id,
            expected_sequence,
            self.previous_event,
            self.prior_state_digest,
            self.policy_digest,
            kind.clone(),
        )?;
        if command.digest() != self.command_digest {
            return Err(corrupt());
        }
        let event = CampaignEvent::from_replay_parts(
            self.event_id,
            self.command_id,
            self.campaign_id,
            self.sequence,
            self.previous_event,
            self.prior_state_digest,
            self.policy_digest,
            self.command_digest,
            self.successor_state_digest,
            CampaignEventKind::Accepted(kind),
        );
        let _ = apply_campaign_event(prior, &event)?;
        Ok(event)
    }
    /// Event identity.
    #[must_use]
    pub const fn event_id(&self) -> EventId {
        self.event_id
    }
    /// Positive aggregate sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
}

impl CanonicalEncode for CampaignEventFrame {
    const FAMILY: u16 = 89;
    const SCHEMA_VERSION: u16 = 1;
    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        writer.write_fixed(self.event_id.as_bytes())?;
        writer.write_fixed(self.command_id.as_bytes())?;
        writer.write_fixed(self.campaign_id.as_bytes())?;
        writer.write_u64(self.sequence)?;
        write_event_option(writer, self.previous_event)?;
        for digest in [
            self.prior_state_digest,
            self.policy_digest,
            self.command_digest,
            self.successor_state_digest,
        ] {
            writer.write_fixed(digest.as_bytes())?;
        }
        writer.write_bytes(&self.kind_bytes)
    }
}

impl CanonicalDecode for CampaignEventFrame {
    const FAMILY: u16 = 89;
    const SCHEMA_VERSION: u16 = 1;
    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let event_id = super::scalar::event_id(reader)?;
        let command_id = super::scalar::command_id(reader)?;
        let campaign_id = super::scalar::campaign_id(reader)?;
        let sequence = reader.read_u64()?;
        let previous_event = read_event_option(reader)?;
        if sequence == 0 || (sequence == 1) != previous_event.is_none() {
            return Err(super::scalar::invalid(reader));
        }
        let prior_state_digest = semantic_digest(reader)?;
        let policy_digest = semantic_digest(reader)?;
        let command_digest = semantic_digest(reader)?;
        let successor_state_digest = semantic_digest(reader)?;
        let kind_bytes = read_semantic(
            reader,
            super::semantic::decode_campaign_kind,
            super::semantic::encode_campaign_kind,
        )?;
        Ok(Self {
            event_id,
            command_id,
            campaign_id,
            sequence,
            previous_event,
            prior_state_digest,
            policy_digest,
            command_digest,
            successor_state_digest,
            kind_bytes,
        })
    }
}

/// Canonical family-90 schema-v1 complete campaign checkpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignStateFrame(CampaignState);

impl CampaignStateFrame {
    /// Clones complete authoritative state into a family-90 frame.
    ///
    /// # Errors
    /// Returns a codec error when complete state cannot be represented canonically.
    pub fn from_state(state: &CampaignState) -> Result<Self, CodecError> {
        super::semantic::encode_campaign_state(state).map_err(super::scalar::semantic)?;
        Ok(Self(state.clone()))
    }
    /// Consumes the checked complete state.
    #[must_use]
    pub fn into_state(self) -> CampaignState {
        self.0
    }
    /// Exact complete-state equality.
    #[must_use]
    pub fn matches_state(&self, state: &CampaignState) -> bool {
        &self.0 == state
    }
    /// Campaign identity.
    #[must_use]
    pub const fn campaign_id(&self) -> EvolutionCampaignId {
        self.0.campaign_id()
    }
    /// Aggregate sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.0.sequence()
    }
    /// Aggregate head.
    #[must_use]
    pub const fn last_event_id(&self) -> EventId {
        self.0.last_event()
    }
    /// Complete state digest.
    #[must_use]
    pub const fn state_digest(&self) -> Sha256Digest {
        self.0.state_digest()
    }
}

impl CanonicalEncode for CampaignStateFrame {
    const FAMILY: u16 = 90;
    const SCHEMA_VERSION: u16 = 1;
    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        writer.write_bytes(
            &super::semantic::encode_campaign_state(&self.0).map_err(super::scalar::semantic)?,
        )
    }
}
impl CanonicalDecode for CampaignStateFrame {
    const FAMILY: u16 = 90;
    const SCHEMA_VERSION: u16 = 1;
    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        super::semantic::decode_campaign_state(reader.read_bytes()?)
            .map(Self)
            .map_err(super::scalar::semantic)
    }
}

fn write_event_option(
    writer: &mut CanonicalWriter,
    value: Option<EventId>,
) -> Result<(), CodecError> {
    writer.write_option_tag(value.is_some())?;
    if let Some(value) = value {
        writer.write_fixed(value.as_bytes())?;
    }
    Ok(())
}
fn read_event_option(reader: &mut CanonicalReader<'_>) -> Result<Option<EventId>, CodecError> {
    reader.read_option_tag()?.then(|| super::scalar::event_id(reader)).transpose()
}
fn semantic_digest(reader: &mut CanonicalReader<'_>) -> Result<Sha256Digest, CodecError> {
    super::scalar::digest(reader).map_err(super::scalar::semantic)
}
fn read_semantic<T>(
    reader: &mut CanonicalReader<'_>,
    decode: impl FnOnce(&[u8]) -> Result<T, EvolutionError>,
    encode: impl FnOnce(&T) -> Result<Vec<u8>, EvolutionError>,
) -> Result<Vec<u8>, CodecError> {
    let offset = reader.offset();
    let bytes = reader.read_bytes_owned()?;
    if bytes.is_empty() {
        return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, offset));
    }
    let value = decode(&bytes).map_err(super::scalar::semantic)?;
    if encode(&value).map_err(super::scalar::semantic)? != bytes {
        return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, offset));
    }
    Ok(bytes)
}
const fn corrupt() -> EvolutionError {
    EvolutionError::new(
        crate::EvolutionErrorKind::Corruption,
        crate::EvolutionOperation::Codec,
        crate::EvolutionRecovery::Quarantine,
        "campaign frame digest disagrees with semantic data",
    )
}
