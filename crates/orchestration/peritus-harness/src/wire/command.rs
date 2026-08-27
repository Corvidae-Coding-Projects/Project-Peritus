//! Inert canonical family-79 harness command frames.

use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
    CodecLimits,
};
use peritus_types::{CommandId, EventId, HarnessId, Sha256Digest};

use crate::{
    aggregate::{
        AggregateError, AggregateErrorKind, AggregateRecovery, HarnessCommand, HarnessCommandKind,
        HarnessState, ReconciliationDecision,
    },
    domain::HarnessRevision,
    materialization::{
        MaterializationFailure, MaterializationPlan, MaterializationPlanId, MaterializationReceipt,
        MaterializationReceiptId,
    },
};

/// Canonical inert family-79 schema-v1 command frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HarnessCommandFrame {
    command_id: CommandId,
    event_id: EventId,
    harness_id: HarnessId,
    expected_sequence: u64,
    expected_previous_event: Option<EventId>,
    prior_state_digest: Sha256Digest,
    command_digest: Sha256Digest,
    kind_tag: u8,
    kind_bytes: Vec<u8>,
}

impl HarnessCommandFrame {
    /// Converts one already-checked command to an inert frame.
    ///
    /// # Errors
    /// Returns a codec error if the semantic payload exceeds family bounds.
    pub fn from_command(command: &HarnessCommand) -> Result<Self, CodecError> {
        let (kind_tag, kind_bytes) = encode_kind(command.kind())?;
        Ok(Self {
            command_id: command.command_id(),
            event_id: command.event_id(),
            harness_id: command.harness_id(),
            expected_sequence: command.expected_sequence(),
            expected_previous_event: command.expected_previous_event(),
            prior_state_digest: command.prior_state_digest(),
            command_digest: command.digest(),
            kind_tag,
            kind_bytes,
        })
    }

    /// Checks inert decoded data against exact predecessor context and domain constructors.
    ///
    /// # Errors
    /// Rejects invalid revisions, plans, receipts, decisions, fences, or digest mismatch.
    pub fn check(self, prior: Option<&HarnessState>) -> Result<HarnessCommand, AggregateError> {
        let kind = decode_kind(self.kind_tag, &self.kind_bytes, prior)?;
        let command = HarnessCommand::new(
            self.command_id,
            self.event_id,
            self.harness_id,
            self.expected_sequence,
            self.expected_previous_event,
            self.prior_state_digest,
            kind,
        )?;
        if command.digest() != self.command_digest {
            return Err(AggregateError::new(
                AggregateErrorKind::Conflict,
                AggregateRecovery::Quarantine,
                "family-79 command digest disagrees with checked semantic data",
            ));
        }
        Ok(command)
    }

    /// Returns command identity without activating semantic data.
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }

    /// Returns the aggregate identity without activating semantic data.
    #[must_use]
    pub const fn harness_id(&self) -> HarnessId {
        self.harness_id
    }
}

impl CanonicalEncode for HarnessCommandFrame {
    const FAMILY: u16 = 79;
    const SCHEMA_VERSION: u16 = 1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        writer.write_fixed(self.command_id.as_bytes())?;
        writer.write_fixed(self.event_id.as_bytes())?;
        writer.write_fixed(self.harness_id.as_bytes())?;
        writer.write_u64(self.expected_sequence)?;
        writer.write_option_tag(self.expected_previous_event.is_some())?;
        if let Some(event) = self.expected_previous_event {
            writer.write_fixed(event.as_bytes())?;
        }
        writer.write_fixed(self.prior_state_digest.as_bytes())?;
        writer.write_fixed(self.command_digest.as_bytes())?;
        writer.write_u8(self.kind_tag)?;
        writer.write_bytes(&self.kind_bytes)
    }
}

impl CanonicalDecode for HarnessCommandFrame {
    const FAMILY: u16 = 79;
    const SCHEMA_VERSION: u16 = 1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let command_id = super::canonical::read_command_id(reader)?;
        let event_id = super::canonical::read_event_id(reader)?;
        let harness_id = read_harness_id(reader)?;
        let expected_sequence = reader.read_u64()?;
        let expected_previous_event = reader
            .read_option_tag()?
            .then(|| super::canonical::read_event_id(reader))
            .transpose()?;
        if (expected_sequence == 0) != expected_previous_event.is_none() {
            return Err(super::canonical::invalid(reader));
        }
        let prior_state_digest = super::canonical::read_digest(reader)?;
        let command_digest = super::canonical::read_digest(reader)?;
        let tag_offset = reader.offset();
        let kind_tag = reader.read_u8()?;
        if !(1..=8).contains(&kind_tag) {
            return Err(super::canonical::unknown(tag_offset));
        }
        let kind_bytes = reader.read_bytes_owned()?;
        Ok(Self {
            command_id,
            event_id,
            harness_id,
            expected_sequence,
            expected_previous_event,
            prior_state_digest,
            command_digest,
            kind_tag,
            kind_bytes,
        })
    }
}

pub(super) fn encode_kind(kind: &HarnessCommandKind) -> Result<(u8, Vec<u8>), CodecError> {
    match kind {
        HarnessCommandKind::RegisterGenesis { revision } => Ok((1, revision.canonical_bytes())),
        HarnessCommandKind::RegisterSuccessor { revision } => Ok((2, revision.canonical_bytes())),
        HarnessCommandKind::PlanMaterialization { plan } => {
            Ok((3, plan.canonical_bytes().map_err(nested)?))
        }
        HarnessCommandKind::AcknowledgeDirectiveDelivery { plan_id, delivered_at_millis } => {
            let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
            writer.write_fixed(plan_id.as_bytes())?;
            writer.write_u64(*delivered_at_millis)?;
            Ok((4, writer.into_bytes()))
        }
        HarnessCommandKind::RecordMaterialization { receipt } => {
            Ok((5, receipt.canonical_bytes().map_err(nested)?))
        }
        HarnessCommandKind::RecordMaterializationFailure { failure } => {
            Ok((6, failure.canonical_bytes().map_err(nested)?))
        }
        HarnessCommandKind::ReconcilePendingMaterialization { plan_id, decision } => {
            let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
            writer.write_fixed(plan_id.as_bytes())?;
            match decision {
                ReconciliationDecision::Retry => writer.write_u8(1)?,
                ReconciliationDecision::Completed(receipt) => {
                    writer.write_u8(2)?;
                    writer.write_bytes(&receipt.canonical_bytes().map_err(nested)?)?;
                }
                ReconciliationDecision::Conflict(failure) => {
                    writer.write_u8(3)?;
                    writer.write_bytes(&failure.canonical_bytes().map_err(nested)?)?;
                }
            }
            Ok((7, writer.into_bytes()))
        }
        HarnessCommandKind::RetireSettledReceipt { receipt_id } => {
            Ok((8, receipt_id.as_bytes().to_vec()))
        }
    }
}

pub(super) fn decode_kind(
    tag: u8,
    bytes: &[u8],
    prior: Option<&HarnessState>,
) -> Result<HarnessCommandKind, AggregateError> {
    match tag {
        1 => HarnessRevision::decode_canonical(bytes, None)
            .map(|revision| HarnessCommandKind::RegisterGenesis { revision })
            .map_err(aggregate),
        2 => {
            let state = prior.ok_or_else(|| malformed("successor command has no prior state"))?;
            let predecessor_digest = HarnessRevision::predecessor_from_canonical(bytes)
                .map_err(aggregate)?
                .ok_or_else(|| malformed("successor bytes name genesis"))?;
            let predecessor = state
                .history()
                .revision(predecessor_digest)
                .ok_or_else(|| malformed("successor predecessor is absent from history"))?;
            HarnessRevision::decode_canonical(bytes, Some(predecessor))
                .map(|revision| HarnessCommandKind::RegisterSuccessor { revision })
                .map_err(aggregate)
        }
        3 => MaterializationPlan::decode_canonical(bytes)
            .map(|plan| HarnessCommandKind::PlanMaterialization { plan })
            .map_err(aggregate),
        4 => decode_ack(bytes),
        5 => MaterializationReceipt::decode_canonical(bytes)
            .map(|receipt| HarnessCommandKind::RecordMaterialization { receipt })
            .map_err(aggregate),
        6 => MaterializationFailure::decode_canonical(bytes)
            .map(|failure| HarnessCommandKind::RecordMaterializationFailure { failure })
            .map_err(aggregate),
        7 => decode_reconcile(bytes),
        8 => {
            let value: [u8; 16] = bytes
                .try_into()
                .map_err(|_| malformed("retired receipt identity has wrong length"))?;
            Ok(HarnessCommandKind::RetireSettledReceipt {
                receipt_id: MaterializationReceiptId::decode(value).map_err(aggregate)?,
            })
        }
        _ => Err(malformed("unknown harness command kind")),
    }
}

fn decode_ack(bytes: &[u8]) -> Result<HarnessCommandKind, AggregateError> {
    let mut reader = CanonicalReader::new(bytes, CodecLimits::PRODUCTION);
    let plan_id = MaterializationPlanId::decode(reader.read_fixed().map_err(aggregate)?)
        .map_err(aggregate)?;
    let delivered_at_millis = reader.read_u64().map_err(aggregate)?;
    reader.finish().map_err(aggregate)?;
    Ok(HarnessCommandKind::AcknowledgeDirectiveDelivery { plan_id, delivered_at_millis })
}

fn decode_reconcile(bytes: &[u8]) -> Result<HarnessCommandKind, AggregateError> {
    let mut reader = CanonicalReader::new(bytes, CodecLimits::PRODUCTION);
    let plan_id = MaterializationPlanId::decode(reader.read_fixed().map_err(aggregate)?)
        .map_err(aggregate)?;
    let decision = match reader.read_u8().map_err(aggregate)? {
        1 => ReconciliationDecision::Retry,
        2 => ReconciliationDecision::Completed(
            MaterializationReceipt::decode_canonical(reader.read_bytes().map_err(aggregate)?)
                .map_err(aggregate)?,
        ),
        3 => ReconciliationDecision::Conflict(
            MaterializationFailure::decode_canonical(reader.read_bytes().map_err(aggregate)?)
                .map_err(aggregate)?,
        ),
        _ => return Err(malformed("unknown reconciliation decision")),
    };
    reader.finish().map_err(aggregate)?;
    Ok(HarnessCommandKind::ReconcilePendingMaterialization { plan_id, decision })
}

fn read_harness_id(reader: &mut CanonicalReader<'_>) -> Result<HarnessId, CodecError> {
    let offset = reader.offset();
    HarnessId::new(reader.read_fixed()?)
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))
}

fn nested(_error: impl core::fmt::Display) -> CodecError {
    CodecError::at(CodecErrorKind::InvalidDomainValue, 0)
}

fn aggregate(error: impl core::fmt::Display) -> AggregateError {
    AggregateError::new(AggregateErrorKind::Codec, AggregateRecovery::Quarantine, error.to_string())
}

fn malformed(detail: &'static str) -> AggregateError {
    AggregateError::new(AggregateErrorKind::Codec, AggregateRecovery::Quarantine, detail)
}
