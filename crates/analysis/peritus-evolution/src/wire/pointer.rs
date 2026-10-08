//! Canonical production-pointer command, event, and complete-state families 91-93.

use std::sync::Arc;

use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
    CodecLimits,
};
use peritus_types::{CommandId, EventId, ProjectId, Sha256Digest};

use crate::{
    EvolutionError, PointerCommand, PointerEvent, PointerEventKind, ProductionHarnessState,
    apply_pointer_event,
};

/// Canonical inert family-91 schema-v1 pointer command frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PointerCommandFrame {
    command_id: CommandId,
    event_id: EventId,
    project_id: ProjectId,
    expected_sequence: u64,
    expected_head: Option<EventId>,
    expected_generation: u64,
    prior_state_digest: Sha256Digest,
    policy_digest: Sha256Digest,
    command_digest: Sha256Digest,
    kind_bytes: Arc<[u8]>,
}

impl PointerCommandFrame {
    /// Converts one checked pointer command into family-91 data.
    ///
    /// # Errors
    /// Returns a codec error when the semantic command cannot be encoded canonically.
    pub fn from_command(command: &PointerCommand) -> Result<Self, CodecError> {
        Ok(Self {
            command_id: command.command_id(),
            event_id: command.event_id(),
            project_id: command.project_id(),
            expected_sequence: command.expected_sequence(),
            expected_head: command.expected_head(),
            expected_generation: command.expected_generation(),
            prior_state_digest: command.prior_state_digest(),
            policy_digest: command.policy_digest(),
            command_digest: command.digest(),
            kind_bytes: super::semantic::encode_pointer_kind(command.kind())
                .map_err(super::scalar::semantic)?
                .into(),
        })
    }
    /// Reconstructs and verifies the complete semantic command.
    ///
    /// # Errors
    /// Returns an evolution error when semantic bytes or their command digest are invalid.
    pub fn into_command(self) -> Result<PointerCommand, EvolutionError> {
        let kind = super::semantic::decode_pointer_kind(&self.kind_bytes)?;
        let command = PointerCommand::new(
            self.command_id,
            self.event_id,
            self.project_id,
            self.expected_sequence,
            self.expected_head,
            self.expected_generation,
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

    /// Aggregate identity without activating semantic data.
    #[must_use]
    pub const fn project_id(&self) -> ProjectId {
        self.project_id
    }
}

impl CanonicalEncode for PointerCommandFrame {
    const FAMILY: u16 = 91;
    const SCHEMA_VERSION: u16 = 1;
    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        writer.write_fixed(self.command_id.as_bytes())?;
        writer.write_fixed(self.event_id.as_bytes())?;
        writer.write_fixed(self.project_id.as_bytes())?;
        writer.write_u64(self.expected_sequence)?;
        write_event_option(writer, self.expected_head)?;
        writer.write_u64(self.expected_generation)?;
        for digest in [self.prior_state_digest, self.policy_digest, self.command_digest] {
            writer.write_fixed(digest.as_bytes())?;
        }
        writer.write_bytes(self.kind_bytes.as_ref())
    }
}

impl CanonicalDecode for PointerCommandFrame {
    const FAMILY: u16 = 91;
    const SCHEMA_VERSION: u16 = 1;
    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let command_id = super::scalar::command_id(reader)?;
        let event_id = super::scalar::event_id(reader)?;
        let project_id = super::scalar::project_id(reader)?;
        let expected_sequence = reader.read_u64()?;
        let expected_head = read_event_option(reader)?;
        let expected_generation = reader.read_u64()?;
        if (expected_sequence == 0) != expected_head.is_none()
            || (expected_sequence == 0) != (expected_generation == 0)
        {
            return Err(super::scalar::invalid(reader));
        }
        let prior_state_digest = semantic_digest(reader)?;
        let policy_digest = semantic_digest(reader)?;
        let command_digest = semantic_digest(reader)?;
        let kind_bytes = read_semantic(reader)?;
        Ok(Self {
            command_id,
            event_id,
            project_id,
            expected_sequence,
            expected_head,
            expected_generation,
            prior_state_digest,
            policy_digest,
            command_digest,
            kind_bytes,
        })
    }
}

/// Canonical inert family-92 schema-v1 production-pointer event frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PointerEventFrame {
    event_id: EventId,
    command_id: CommandId,
    project_id: ProjectId,
    sequence: u64,
    previous_event: Option<EventId>,
    prior_generation: u64,
    successor_generation: u64,
    prior_state_digest: Sha256Digest,
    policy_digest: Sha256Digest,
    command_digest: Sha256Digest,
    successor_state_digest: Sha256Digest,
    kind_bytes: Arc<[u8]>,
}

impl PointerEventFrame {
    /// Converts one accepted pointer event into family-92 data.
    ///
    /// # Errors
    /// Returns a codec error when the accepted event kind cannot be encoded canonically.
    pub fn from_event(event: &PointerEvent) -> Result<Self, CodecError> {
        let PointerEventKind::Accepted(kind) = event.kind();
        Ok(Self {
            event_id: event.id(),
            command_id: event.command_id(),
            project_id: event.project_id(),
            sequence: event.sequence(),
            previous_event: event.previous_event(),
            prior_generation: event.prior_generation(),
            successor_generation: event.successor_generation(),
            prior_state_digest: event.prior_state_digest(),
            policy_digest: event.policy_digest(),
            command_digest: event.command_digest(),
            successor_state_digest: event.successor_state_digest(),
            kind_bytes: super::semantic::encode_pointer_kind(kind)
                .map_err(super::scalar::semantic)?
                .into(),
        })
    }
    pub(crate) fn from_event_and_command(
        event: &PointerEvent,
        command: &PointerCommandFrame,
    ) -> Result<Self, CodecError> {
        if command.command_id != event.command_id()
            || command.event_id != event.id()
            || command.project_id != event.project_id()
            || command.expected_sequence.checked_add(1) != Some(event.sequence())
            || command.expected_head != event.previous_event()
            || command.expected_generation != event.prior_generation()
            || command.prior_state_digest != event.prior_state_digest()
            || command.policy_digest != event.policy_digest()
            || command.command_digest != event.command_digest()
        {
            return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, 0));
        }
        Ok(Self {
            event_id: event.id(),
            command_id: event.command_id(),
            project_id: event.project_id(),
            sequence: event.sequence(),
            previous_event: event.previous_event(),
            prior_generation: event.prior_generation(),
            successor_generation: event.successor_generation(),
            prior_state_digest: event.prior_state_digest(),
            policy_digest: event.policy_digest(),
            command_digest: event.command_digest(),
            successor_state_digest: event.successor_state_digest(),
            kind_bytes: Arc::clone(&command.kind_bytes),
        })
    }
    /// Reconstructs an event and verifies its producing command and exact predecessor replay.
    ///
    /// # Errors
    /// Returns an evolution error for malformed semantics, digest drift, or invalid replay.
    pub fn check(
        self,
        prior: Option<&ProductionHarnessState>,
    ) -> Result<PointerEvent, EvolutionError> {
        let event = self.into_event()?;
        let _ = apply_pointer_event(prior, &event)?;
        Ok(event)
    }

    /// Reconstructs and digest-checks the event without selecting a replay compatibility rule.
    ///
    /// Historical recovery uses this before comparing current and legacy reducer results with the
    /// exact immutable checkpoint installed by the same command.
    pub(crate) fn into_event(self) -> Result<PointerEvent, EvolutionError> {
        let kind = super::semantic::decode_pointer_kind(&self.kind_bytes)?;
        let expected_sequence = self.sequence.checked_sub(1).ok_or_else(corrupt)?;
        let command = PointerCommand::new(
            self.command_id,
            self.event_id,
            self.project_id,
            expected_sequence,
            self.previous_event,
            self.prior_generation,
            self.prior_state_digest,
            self.policy_digest,
            kind.clone(),
        )?;
        if command.digest() != self.command_digest {
            return Err(corrupt());
        }
        let event = PointerEvent::from_replay_parts(
            self.event_id,
            self.command_id,
            self.project_id,
            self.sequence,
            self.previous_event,
            self.prior_generation,
            self.successor_generation,
            self.prior_state_digest,
            self.policy_digest,
            self.command_digest,
            self.successor_state_digest,
            PointerEventKind::Accepted(kind),
        );
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

impl CanonicalEncode for PointerEventFrame {
    const FAMILY: u16 = 92;
    const SCHEMA_VERSION: u16 = 1;
    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        writer.write_fixed(self.event_id.as_bytes())?;
        writer.write_fixed(self.command_id.as_bytes())?;
        writer.write_fixed(self.project_id.as_bytes())?;
        writer.write_u64(self.sequence)?;
        write_event_option(writer, self.previous_event)?;
        writer.write_u64(self.prior_generation)?;
        writer.write_u64(self.successor_generation)?;
        for digest in [
            self.prior_state_digest,
            self.policy_digest,
            self.command_digest,
            self.successor_state_digest,
        ] {
            writer.write_fixed(digest.as_bytes())?;
        }
        writer.write_bytes(self.kind_bytes.as_ref())
    }
}

impl CanonicalDecode for PointerEventFrame {
    const FAMILY: u16 = 92;
    const SCHEMA_VERSION: u16 = 1;
    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let event_id = super::scalar::event_id(reader)?;
        let command_id = super::scalar::command_id(reader)?;
        let project_id = super::scalar::project_id(reader)?;
        let sequence = reader.read_u64()?;
        let previous_event = read_event_option(reader)?;
        let prior_generation = reader.read_u64()?;
        let successor_generation = reader.read_u64()?;
        if sequence == 0
            || (sequence == 1) != previous_event.is_none()
            || (sequence == 1) != (prior_generation == 0)
            || successor_generation == 0
        {
            return Err(super::scalar::invalid(reader));
        }
        let prior_state_digest = semantic_digest(reader)?;
        let policy_digest = semantic_digest(reader)?;
        let command_digest = semantic_digest(reader)?;
        let successor_state_digest = semantic_digest(reader)?;
        let kind_bytes = read_semantic(reader)?;
        Ok(Self {
            event_id,
            command_id,
            project_id,
            sequence,
            previous_event,
            prior_generation,
            successor_generation,
            prior_state_digest,
            policy_digest,
            command_digest,
            successor_state_digest,
            kind_bytes,
        })
    }
}

/// Canonical family-93 schema-v1 complete production-pointer checkpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PointerStateFrame(ProductionHarnessState);

impl PointerStateFrame {
    /// Clones complete authoritative pointer state into a family-93 frame.
    ///
    /// # Errors
    /// Returns a codec error when complete state cannot be represented canonically.
    pub fn from_state(state: &ProductionHarnessState) -> Result<Self, CodecError> {
        super::semantic::encode_pointer_state(state).map_err(super::scalar::semantic)?;
        Ok(Self(state.clone()))
    }
    /// Consumes the checked complete state.
    #[must_use]
    pub fn into_state(self) -> ProductionHarnessState {
        self.0
    }
    /// Exact complete-state equality.
    #[must_use]
    pub fn matches_state(&self, state: &ProductionHarnessState) -> bool {
        &self.0 == state
    }
    /// Project aggregate identity.
    #[must_use]
    pub const fn project_id(&self) -> ProjectId {
        self.0.project_id()
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

impl CanonicalEncode for PointerStateFrame {
    const FAMILY: u16 = 93;
    const SCHEMA_VERSION: u16 = 1;
    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        writer.write_bytes(
            &super::semantic::encode_pointer_state(&self.0).map_err(super::scalar::semantic)?,
        )
    }
}
impl CanonicalDecode for PointerStateFrame {
    const FAMILY: u16 = 93;
    const SCHEMA_VERSION: u16 = 1;
    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        super::semantic::decode_pointer_state(reader.read_bytes()?)
            .map(Self)
            .map_err(super::scalar::semantic)
    }
}

const POINTER_CHECKPOINT_REFERENCE_DOMAIN: &[u8] =
    b"peritus.f0.pointer-checkpoint-reference.v2\0";

/// Schema-v2 checkpoint projection referencing immutable pointer events by exact state identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PointerCheckpointFrame {
    project_id: ProjectId,
    current_digest: Sha256Digest,
    policy_digest: Sha256Digest,
    generation: u64,
    sequence: u64,
    last_event: EventId,
    state_digest: Sha256Digest,
    revision_digest: Sha256Digest,
}

impl PointerCheckpointFrame {
    pub(crate) fn from_state(state: &ProductionHarnessState) -> Self {
        Self {
            project_id: state.project_id(),
            current_digest: state.current().digest(),
            policy_digest: state.policy().digest(),
            generation: state.generation(),
            sequence: state.sequence(),
            last_event: state.last_event(),
            state_digest: state.state_digest(),
            revision_digest: pointer_checkpoint_revision_digest(state),
        }
    }

    pub(crate) const fn project_id(self) -> ProjectId {
        self.project_id
    }

    pub(crate) const fn generation(self) -> u64 {
        self.generation
    }

    pub(crate) const fn sequence(self) -> u64 {
        self.sequence
    }

    pub(crate) const fn last_event_id(self) -> EventId {
        self.last_event
    }

    pub(crate) const fn state_digest(self) -> Sha256Digest {
        self.state_digest
    }

    pub(crate) const fn policy_digest(self) -> Sha256Digest {
        self.policy_digest
    }

    pub(crate) const fn revision_digest(self) -> Sha256Digest {
        self.revision_digest
    }

    pub(crate) fn matches_state(self, state: &ProductionHarnessState) -> bool {
        self.project_id == state.project_id()
            && self.current_digest == state.current().digest()
            && self.policy_digest == state.policy().digest()
            && self.generation == state.generation()
            && self.sequence == state.sequence()
            && self.last_event == state.last_event()
            && self.state_digest == state.state_digest()
            && self.revision_digest == pointer_checkpoint_revision_digest(state)
    }
}

impl CanonicalEncode for PointerCheckpointFrame {
    const FAMILY: u16 = 93;
    const SCHEMA_VERSION: u16 = 2;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        let reference = pointer_checkpoint_reference(*self)?;
        writer.write_fixed(&reference)?;
        writer.write_fixed(peritus_codec::sha256(&reference).as_bytes())
    }
}

impl CanonicalDecode for PointerCheckpointFrame {
    const FAMILY: u16 = 93;
    const SCHEMA_VERSION: u16 = 2;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        if reader.read_bytes()? != POINTER_CHECKPOINT_REFERENCE_DOMAIN {
            return Err(super::scalar::invalid(reader));
        }
        let value = Self {
            project_id: super::scalar::project_id(reader)?,
            current_digest: semantic_digest(reader)?,
            policy_digest: semantic_digest(reader)?,
            generation: reader.read_u64()?,
            sequence: reader.read_u64()?,
            last_event: super::scalar::event_id(reader)?,
            state_digest: semantic_digest(reader)?,
            revision_digest: semantic_digest(reader)?,
        };
        let encoded_digest = semantic_digest(reader)?;
        if value.generation == 0
            || value.sequence == 0
            || encoded_digest != peritus_codec::sha256(&pointer_checkpoint_reference(value)?)
        {
            return Err(super::scalar::invalid(reader));
        }
        Ok(value)
    }
}

fn pointer_checkpoint_reference(value: PointerCheckpointFrame) -> Result<Vec<u8>, CodecError> {
    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    writer.write_bytes(POINTER_CHECKPOINT_REFERENCE_DOMAIN)?;
    writer.write_fixed(value.project_id.as_bytes())?;
    writer.write_fixed(value.current_digest.as_bytes())?;
    writer.write_fixed(value.policy_digest.as_bytes())?;
    writer.write_u64(value.generation)?;
    writer.write_u64(value.sequence)?;
    writer.write_fixed(value.last_event.as_bytes())?;
    writer.write_fixed(value.state_digest.as_bytes())?;
    writer.write_fixed(value.revision_digest.as_bytes())?;
    Ok(writer.into_bytes())
}

fn pointer_checkpoint_revision_digest(state: &ProductionHarnessState) -> Sha256Digest {
    state.history().last().map_or_else(
        || state.state_digest(),
        |record| peritus_evidence::revision_digest(&record.successor().revision()),
    )
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
fn read_semantic(reader: &mut CanonicalReader<'_>) -> Result<Arc<[u8]>, CodecError> {
    let offset = reader.offset();
    let bytes = reader.read_bytes_owned()?;
    if bytes.is_empty() {
        return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, offset));
    }
    let value = super::semantic::decode_pointer_kind(&bytes).map_err(super::scalar::semantic)?;
    if super::semantic::encode_pointer_kind(&value).map_err(super::scalar::semantic)? != bytes {
        return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, offset));
    }
    Ok(bytes.into())
}
const fn corrupt() -> EvolutionError {
    EvolutionError::new(
        crate::EvolutionErrorKind::Corruption,
        crate::EvolutionOperation::Codec,
        crate::EvolutionRecovery::Quarantine,
        "pointer frame digest disagrees with semantic data",
    )
}
