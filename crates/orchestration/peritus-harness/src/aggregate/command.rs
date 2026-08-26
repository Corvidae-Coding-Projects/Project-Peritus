//! Closed administrative harness command vocabulary.

use peritus_codec::{CanonicalWriter, CodecLimits};
use peritus_types::{CommandId, EventId, HarnessId, Sha256Digest};

use crate::{
    domain::HarnessRevision,
    materialization::{
        MaterializationFailure, MaterializationPlan, MaterializationPlanId, MaterializationReceipt,
        MaterializationReceiptId,
    },
};

use super::{AggregateError, AggregateErrorKind, AggregateRecovery};

const COMMAND_DOMAIN: &[u8] = b"peritus.harness.command.v1\0";

/// Exact observation used to reconcile one committed pending plan.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(
    clippy::large_enum_variant,
    reason = "closed recovery evidence remains directly inspectable"
)]
pub enum ReconciliationDecision {
    /// The target remains untouched and the same directive may be redelivered.
    Retry,
    /// Exact C1 evidence proves the already-completed candidate.
    Completed(MaterializationReceipt),
    /// Exact observations conflict and the pending plan must be quarantined.
    Conflict(MaterializationFailure),
}

/// Closed E1 command vocabulary. It contains no evaluation or promotion command.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(clippy::large_enum_variant, reason = "closed semantic values remain directly inspectable")]
pub enum HarnessCommandKind {
    /// Registers the checked immutable genesis revision.
    RegisterGenesis {
        /// Complete checked genesis revision.
        revision: HarnessRevision,
    },
    /// Registers one checked direct successor revision.
    RegisterSuccessor {
        /// Complete checked successor revision.
        revision: HarnessRevision,
    },
    /// Commits an exact materialization plan before any C1 effect.
    PlanMaterialization {
        /// Exact deterministic plan to retain and deliver.
        plan: MaterializationPlan,
    },
    /// Records transport delivery of the stable outbox directive.
    AcknowledgeDirectiveDelivery {
        /// Exact pending plan.
        plan_id: MaterializationPlanId,
        /// Durable delivery observation time.
        delivered_at_millis: u64,
    },
    /// Settles exact pending work with a complete receipt.
    RecordMaterialization {
        /// Complete checked C1 receipt.
        receipt: MaterializationReceipt,
    },
    /// Settles or quarantines exact pending work with a typed failure.
    RecordMaterializationFailure {
        /// Typed materialization failure evidence.
        failure: MaterializationFailure,
    },
    /// Reconciles pending work from exact restart observations.
    ReconcilePendingMaterialization {
        /// Exact pending plan.
        plan_id: MaterializationPlanId,
        /// Checked restart disposition.
        decision: ReconciliationDecision,
    },
    /// Removes one settled receipt from the bounded hot projection only.
    RetireSettledReceipt {
        /// Exact settled receipt to retire from hot state.
        receipt_id: MaterializationReceiptId,
    },
}

/// One fenced, digest-bound harness aggregate command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HarnessCommand {
    command_id: CommandId,
    event_id: EventId,
    harness_id: HarnessId,
    expected_sequence: u64,
    expected_previous_event: Option<EventId>,
    prior_state_digest: Sha256Digest,
    kind: HarnessCommandKind,
    digest: Sha256Digest,
}

impl HarnessCommand {
    /// Constructs and binds one complete command.
    ///
    /// # Errors
    /// Rejects invalid genesis fences or a payload naming another command, event, or harness.
    #[allow(clippy::too_many_arguments, reason = "all aggregate fence fields remain explicit")]
    pub fn new(
        command_id: CommandId,
        event_id: EventId,
        harness_id: HarnessId,
        expected_sequence: u64,
        expected_previous_event: Option<EventId>,
        prior_state_digest: Sha256Digest,
        kind: HarnessCommandKind,
    ) -> Result<Self, AggregateError> {
        if (expected_sequence == 0) != expected_previous_event.is_none() {
            return Err(invalid("command sequence and predecessor presence disagree"));
        }
        validate_kind(command_id, event_id, harness_id, &kind)?;
        let mut command = Self {
            command_id,
            event_id,
            harness_id,
            expected_sequence,
            expected_previous_event,
            prior_state_digest,
            kind,
            digest: Sha256Digest::new([0; 32]),
        };
        command.digest = peritus_codec::sha256(&command.canonical_identity_bytes()?);
        Ok(command)
    }

    /// Returns command identity.
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
    /// Returns the event identity reserved for acceptance.
    #[must_use]
    pub const fn event_id(&self) -> EventId {
        self.event_id
    }
    /// Returns the harness lineage.
    #[must_use]
    pub const fn harness_id(&self) -> HarnessId {
        self.harness_id
    }
    /// Returns the expected current event sequence.
    #[must_use]
    pub const fn expected_sequence(&self) -> u64 {
        self.expected_sequence
    }
    /// Returns the expected aggregate head event.
    #[must_use]
    pub const fn expected_previous_event(&self) -> Option<EventId> {
        self.expected_previous_event
    }
    /// Returns the exact expected prior state digest.
    #[must_use]
    pub const fn prior_state_digest(&self) -> Sha256Digest {
        self.prior_state_digest
    }
    /// Returns the semantic command.
    #[must_use]
    pub const fn kind(&self) -> &HarnessCommandKind {
        &self.kind
    }
    /// Returns the complete canonical command digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    pub(crate) fn canonical_identity_bytes(&self) -> Result<Vec<u8>, AggregateError> {
        let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
        writer.write_fixed(COMMAND_DOMAIN).map_err(codec)?;
        writer.write_fixed(self.command_id.as_bytes()).map_err(codec)?;
        writer.write_fixed(self.event_id.as_bytes()).map_err(codec)?;
        writer.write_fixed(self.harness_id.as_bytes()).map_err(codec)?;
        writer.write_u64(self.expected_sequence).map_err(codec)?;
        writer.write_option_tag(self.expected_previous_event.is_some()).map_err(codec)?;
        if let Some(event) = self.expected_previous_event {
            writer.write_fixed(event.as_bytes()).map_err(codec)?;
        }
        writer.write_fixed(self.prior_state_digest.as_bytes()).map_err(codec)?;
        encode_kind(&mut writer, &self.kind)?;
        Ok(writer.into_bytes())
    }
}

fn validate_kind(
    command_id: CommandId,
    event_id: EventId,
    harness_id: HarnessId,
    kind: &HarnessCommandKind,
) -> Result<(), AggregateError> {
    match kind {
        HarnessCommandKind::RegisterGenesis { revision }
        | HarnessCommandKind::RegisterSuccessor { revision }
            if revision.harness_id() != harness_id =>
        {
            Err(invalid("revision belongs to another harness"))
        }
        HarnessCommandKind::PlanMaterialization { plan }
            if plan.harness_id() != harness_id
                || plan.command_id() != command_id
                || plan.causal_event_id() != event_id =>
        {
            Err(invalid("plan identity does not match its planning command"))
        }
        HarnessCommandKind::RecordMaterialization { receipt }
            if receipt.harness_id() != harness_id =>
        {
            Err(invalid("receipt belongs to another harness"))
        }
        _ => Ok(()),
    }
}

fn encode_kind(
    writer: &mut CanonicalWriter,
    kind: &HarnessCommandKind,
) -> Result<(), AggregateError> {
    match kind {
        HarnessCommandKind::RegisterGenesis { revision } => {
            opaque(writer, 1, &revision.canonical_bytes())
        }
        HarnessCommandKind::RegisterSuccessor { revision } => {
            opaque(writer, 2, &revision.canonical_bytes())
        }
        HarnessCommandKind::PlanMaterialization { plan } => {
            opaque(writer, 3, &plan.canonical_bytes().map_err(codec)?)
        }
        HarnessCommandKind::AcknowledgeDirectiveDelivery { plan_id, delivered_at_millis } => {
            writer.write_u8(4).map_err(codec)?;
            writer.write_fixed(plan_id.as_bytes()).map_err(codec)?;
            writer.write_u64(*delivered_at_millis).map_err(codec)
        }
        HarnessCommandKind::RecordMaterialization { receipt } => {
            opaque(writer, 5, &receipt.canonical_bytes().map_err(codec)?)
        }
        HarnessCommandKind::RecordMaterializationFailure { failure } => {
            opaque(writer, 6, &failure.canonical_bytes().map_err(codec)?)
        }
        HarnessCommandKind::ReconcilePendingMaterialization { plan_id, decision } => {
            writer.write_u8(7).map_err(codec)?;
            writer.write_fixed(plan_id.as_bytes()).map_err(codec)?;
            match decision {
                ReconciliationDecision::Retry => writer.write_u8(1).map_err(codec),
                ReconciliationDecision::Completed(receipt) => {
                    writer.write_u8(2).map_err(codec)?;
                    writer.write_bytes(&receipt.canonical_bytes().map_err(codec)?).map_err(codec)
                }
                ReconciliationDecision::Conflict(failure) => {
                    writer.write_u8(3).map_err(codec)?;
                    writer.write_bytes(&failure.canonical_bytes().map_err(codec)?).map_err(codec)
                }
            }
        }
        HarnessCommandKind::RetireSettledReceipt { receipt_id } => {
            writer.write_u8(8).map_err(codec)?;
            writer.write_fixed(receipt_id.as_bytes()).map_err(codec)
        }
    }
}

fn opaque(writer: &mut CanonicalWriter, tag: u8, bytes: &[u8]) -> Result<(), AggregateError> {
    writer.write_u8(tag).map_err(codec)?;
    writer.write_bytes(bytes).map_err(codec)
}

fn codec(error: impl core::fmt::Display) -> AggregateError {
    AggregateError::new(AggregateErrorKind::Codec, AggregateRecovery::Quarantine, error.to_string())
}

fn invalid(detail: &'static str) -> AggregateError {
    AggregateError::new(
        AggregateErrorKind::InvalidCommand,
        AggregateRecovery::CorrectCommand,
        detail,
    )
}
