//! Inert canonical family-80 harness event frames.

use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
};
use peritus_types::{CommandId, EventId, HarnessId, Sha256Digest};

use crate::aggregate::{
    AggregateError, HarnessCommandKind, HarnessEvent, HarnessEventKind, HarnessState, apply_event,
};
use crate::domain::RevisionDigest;

/// Canonical inert family-80 schema-v1 semantic event frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HarnessEventFrame {
    event_id: EventId,
    command_id: CommandId,
    harness_id: HarnessId,
    sequence: u64,
    previous_event: Option<EventId>,
    prior_state_digest: Sha256Digest,
    command_digest: Sha256Digest,
    successor_state_digest: Sha256Digest,
    revision_digest: RevisionDigest,
    artifact_roots: Vec<Sha256Digest>,
    kind_tag: u8,
    kind_bytes: Vec<u8>,
}

impl HarnessEventFrame {
    /// Converts one accepted event into its inert canonical frame.
    ///
    /// # Errors
    /// Returns a codec error when nested semantic bytes exceed family bounds.
    pub fn from_event(event: &HarnessEvent) -> Result<Self, CodecError> {
        let command_kind = event_kind_to_command(event.kind().clone());
        let (kind_tag, kind_bytes) = super::command::encode_kind(&command_kind)?;
        Ok(Self {
            event_id: event.id(),
            command_id: event.command_id(),
            harness_id: event.harness_id(),
            sequence: event.sequence(),
            previous_event: event.previous_event(),
            prior_state_digest: event.prior_state_digest(),
            command_digest: event.command_digest(),
            successor_state_digest: event.successor_state_digest(),
            revision_digest: event.revision_digest(),
            artifact_roots: event.artifact_roots().to_vec(),
            kind_tag,
            kind_bytes,
        })
    }

    /// Activates inert event data only through nested constructors and deterministic replay.
    ///
    /// # Errors
    /// Rejects malformed semantic bytes or any predecessor/successor/digest disagreement.
    pub fn check(self, prior: Option<&HarnessState>) -> Result<HarnessEvent, AggregateError> {
        let command_kind = super::command::decode_kind(self.kind_tag, &self.kind_bytes, prior)?;
        let kind = command_kind_to_event(command_kind);
        let event = HarnessEvent::new(
            self.event_id,
            self.command_id,
            self.harness_id,
            self.sequence,
            self.previous_event,
            self.prior_state_digest,
            self.command_digest,
            self.successor_state_digest,
            self.revision_digest,
            self.artifact_roots,
            kind,
        );
        let _checked_successor = apply_event(prior, &event)?;
        Ok(event)
    }

    /// Returns event identity without activating semantic data.
    #[must_use]
    pub const fn event_id(&self) -> EventId {
        self.event_id
    }
    /// Returns aggregate sequence without activating semantic data.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
}

impl CanonicalEncode for HarnessEventFrame {
    const FAMILY: u16 = 80;
    const SCHEMA_VERSION: u16 = 1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        writer.write_fixed(self.event_id.as_bytes())?;
        writer.write_fixed(self.command_id.as_bytes())?;
        writer.write_fixed(self.harness_id.as_bytes())?;
        writer.write_u64(self.sequence)?;
        writer.write_option_tag(self.previous_event.is_some())?;
        if let Some(event) = self.previous_event {
            writer.write_fixed(event.as_bytes())?;
        }
        writer.write_fixed(self.prior_state_digest.as_bytes())?;
        writer.write_fixed(self.command_digest.as_bytes())?;
        writer.write_fixed(self.successor_state_digest.as_bytes())?;
        writer.write_fixed(self.revision_digest.as_bytes())?;
        writer.write_collection_len(self.artifact_roots.len())?;
        for root in &self.artifact_roots {
            writer.write_fixed(root.as_bytes())?;
        }
        writer.write_u8(self.kind_tag)?;
        writer.write_bytes(&self.kind_bytes)
    }
}

impl CanonicalDecode for HarnessEventFrame {
    const FAMILY: u16 = 80;
    const SCHEMA_VERSION: u16 = 1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let event_id = super::canonical::read_event_id(reader)?;
        let command_id = super::canonical::read_command_id(reader)?;
        let harness_id = read_harness_id(reader)?;
        let sequence = reader.read_u64()?;
        let previous_event = reader
            .read_option_tag()?
            .then(|| super::canonical::read_event_id(reader))
            .transpose()?;
        if sequence == 0 || (sequence == 1) != previous_event.is_none() {
            return Err(super::canonical::invalid(reader));
        }
        let prior_state_digest = super::canonical::read_digest(reader)?;
        let command_digest = super::canonical::read_digest(reader)?;
        let successor_state_digest = super::canonical::read_digest(reader)?;
        let revision_digest = RevisionDigest::new(super::canonical::read_digest(reader)?);
        let root_count = reader.read_collection_len()?;
        let mut artifact_roots = Vec::with_capacity(root_count);
        for _ in 0..root_count {
            artifact_roots.push(super::canonical::read_digest(reader)?);
        }
        if artifact_roots.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(super::canonical::invalid(reader));
        }
        let tag_offset = reader.offset();
        let kind_tag = reader.read_u8()?;
        if !(1..=8).contains(&kind_tag) {
            return Err(super::canonical::unknown(tag_offset));
        }
        let kind_bytes = reader.read_bytes_owned()?;
        Ok(Self {
            event_id,
            command_id,
            harness_id,
            sequence,
            previous_event,
            prior_state_digest,
            command_digest,
            successor_state_digest,
            revision_digest,
            artifact_roots,
            kind_tag,
            kind_bytes,
        })
    }
}

fn event_kind_to_command(kind: HarnessEventKind) -> HarnessCommandKind {
    match kind {
        HarnessEventKind::GenesisRegistered { revision } => {
            HarnessCommandKind::RegisterGenesis { revision }
        }
        HarnessEventKind::SuccessorRegistered { revision } => {
            HarnessCommandKind::RegisterSuccessor { revision }
        }
        HarnessEventKind::MaterializationPlanned { plan } => {
            HarnessCommandKind::PlanMaterialization { plan }
        }
        HarnessEventKind::DirectiveDeliveryAcknowledged { plan_id, delivered_at_millis } => {
            HarnessCommandKind::AcknowledgeDirectiveDelivery { plan_id, delivered_at_millis }
        }
        HarnessEventKind::MaterializationRecorded { receipt } => {
            HarnessCommandKind::RecordMaterialization { receipt }
        }
        HarnessEventKind::MaterializationFailureRecorded { failure } => {
            HarnessCommandKind::RecordMaterializationFailure { failure }
        }
        HarnessEventKind::PendingMaterializationReconciled { plan_id, decision } => {
            HarnessCommandKind::ReconcilePendingMaterialization { plan_id, decision }
        }
        HarnessEventKind::SettledReceiptRetired { receipt_id } => {
            HarnessCommandKind::RetireSettledReceipt { receipt_id }
        }
    }
}

fn command_kind_to_event(kind: HarnessCommandKind) -> HarnessEventKind {
    match kind {
        HarnessCommandKind::RegisterGenesis { revision } => {
            HarnessEventKind::GenesisRegistered { revision }
        }
        HarnessCommandKind::RegisterSuccessor { revision } => {
            HarnessEventKind::SuccessorRegistered { revision }
        }
        HarnessCommandKind::PlanMaterialization { plan } => {
            HarnessEventKind::MaterializationPlanned { plan }
        }
        HarnessCommandKind::AcknowledgeDirectiveDelivery { plan_id, delivered_at_millis } => {
            HarnessEventKind::DirectiveDeliveryAcknowledged { plan_id, delivered_at_millis }
        }
        HarnessCommandKind::RecordMaterialization { receipt } => {
            HarnessEventKind::MaterializationRecorded { receipt }
        }
        HarnessCommandKind::RecordMaterializationFailure { failure } => {
            HarnessEventKind::MaterializationFailureRecorded { failure }
        }
        HarnessCommandKind::ReconcilePendingMaterialization { plan_id, decision } => {
            HarnessEventKind::PendingMaterializationReconciled { plan_id, decision }
        }
        HarnessCommandKind::RetireSettledReceipt { receipt_id } => {
            HarnessEventKind::SettledReceiptRetired { receipt_id }
        }
    }
}

fn read_harness_id(reader: &mut CanonicalReader<'_>) -> Result<HarnessId, CodecError> {
    let offset = reader.offset();
    HarnessId::new(reader.read_fixed()?)
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))
}
