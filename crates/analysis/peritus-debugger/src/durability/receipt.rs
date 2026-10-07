//! Original commit authority and historical outcomes retained independently of current state.

use core::ops::Deref;

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecLimits};
use peritus_journal::{CommittedBatch, OutboxId};
use peritus_types::{CommandId, EventId, Sha256Digest};

use crate::{
    DebuggerError, DebuggerErrorKind, DebuggerEvent, DebuggerJobId, DebuggerOperation,
    DebuggerRecovery, DebuggerState,
};

/// Journal-owned namespace for immutable E2 operation receipts.
pub const DEBUGGER_RECEIPT_NAMESPACE: u16 = 0xE202;

const RECEIPT_DOMAIN: &[u8] = b"peritus.debugger.operation-receipt.v1\0";
const RECEIPT_KEY_DOMAIN: &[u8] = b"peritus.debugger.operation-receipt-key.v1\0";

/// Original commit mode recorded for one accepted debugger operation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DebuggerCommitMode {
    /// No external-effect claim participated in the commit.
    Ordinary,
    /// A claim authorized the durable pre-effect start transition.
    Claimed,
    /// A claim was acknowledged atomically with effect settlement.
    Settlement,
    /// An operation predates retained claim receipts.
    Legacy,
}

impl DebuggerCommitMode {
    const fn tag(self) -> u8 {
        match self {
            Self::Ordinary => 1,
            Self::Claimed => 2,
            Self::Settlement => 3,
            Self::Legacy => 4,
        }
    }

    fn from_stored_tag(tag: u8) -> Result<Self, DebuggerError> {
        match tag {
            1 => Ok(Self::Ordinary),
            2 => Ok(Self::Claimed),
            3 => Ok(Self::Settlement),
            _ => Err(corrupt("stored debugger receipt has an unknown commit mode")),
        }
    }
}

/// Exact original outbox authority retained for a claim-bound debugger commit.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DebuggerClaimReceipt {
    outbox_id: OutboxId,
    fence: u64,
}

impl DebuggerClaimReceipt {
    /// Original durable directive identity.
    #[must_use]
    pub const fn outbox_id(self) -> OutboxId {
        self.outbox_id
    }

    /// Original positive claim fence accepted by the commit.
    #[must_use]
    pub const fn fence(self) -> u64 {
        self.fence
    }
}

/// Immutable request and authority receipt for one accepted debugger transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DebuggerOperationReceipt {
    command_id: CommandId,
    event_id: EventId,
    job_id: DebuggerJobId,
    sequence: u64,
    command_digest: Sha256Digest,
    base_request_digest: Option<Sha256Digest>,
    request_digest: Sha256Digest,
    event_frame_digest: Sha256Digest,
    successor_state_digest: Sha256Digest,
    mode: DebuggerCommitMode,
    original_claim: Option<DebuggerClaimReceipt>,
}

impl DebuggerOperationReceipt {
    #[allow(clippy::too_many_arguments, reason = "complete original commit evidence is explicit")]
    pub(super) fn retained(
        command_id: CommandId,
        event_id: EventId,
        job_id: DebuggerJobId,
        sequence: u64,
        command_digest: Sha256Digest,
        base_request_digest: Sha256Digest,
        request_digest: Sha256Digest,
        event_frame_digest: Sha256Digest,
        successor_state_digest: Sha256Digest,
        mode: DebuggerCommitMode,
        original_claim: Option<DebuggerClaimReceipt>,
    ) -> Result<Self, DebuggerError> {
        let claim_mode = matches!(mode, DebuggerCommitMode::Claimed | DebuggerCommitMode::Settlement);
        if sequence == 0
            || mode == DebuggerCommitMode::Legacy
            || claim_mode != original_claim.is_some()
        {
            return Err(binding("debugger operation receipt authority is inconsistent"));
        }
        Ok(Self {
            command_id,
            event_id,
            job_id,
            sequence,
            command_digest,
            base_request_digest: Some(base_request_digest),
            request_digest,
            event_frame_digest,
            successor_state_digest,
            mode,
            original_claim,
        })
    }

    #[allow(clippy::too_many_arguments, reason = "legacy event evidence remains explicit")]
    pub(super) const fn legacy(
        command_id: CommandId,
        event_id: EventId,
        job_id: DebuggerJobId,
        sequence: u64,
        command_digest: Sha256Digest,
        request_digest: Sha256Digest,
        event_frame_digest: Sha256Digest,
        successor_state_digest: Sha256Digest,
    ) -> Self {
        Self {
            command_id,
            event_id,
            job_id,
            sequence,
            command_digest,
            base_request_digest: None,
            request_digest,
            event_frame_digest,
            successor_state_digest,
            mode: DebuggerCommitMode::Legacy,
            original_claim: None,
        }
    }

    /// Accepted command identity.
    #[must_use]
    pub const fn command_id(self) -> CommandId {
        self.command_id
    }
    /// Accepted event identity.
    #[must_use]
    pub const fn event_id(self) -> EventId {
        self.event_id
    }
    /// Owning debugger job.
    #[must_use]
    pub const fn job_id(self) -> DebuggerJobId {
        self.job_id
    }
    /// Historical successor sequence.
    #[must_use]
    pub const fn sequence(self) -> u64 {
        self.sequence
    }
    /// Semantic command digest committed in the immutable event.
    #[must_use]
    pub const fn command_digest(self) -> Sha256Digest {
        self.command_digest
    }
    /// Pre-claim canonical request digest, when the receipt was recorded by this schema.
    #[must_use]
    pub const fn base_request_digest(self) -> Option<Sha256Digest> {
        self.base_request_digest
    }
    /// Original exact request digest retained by the C0 command receipt.
    #[must_use]
    pub const fn request_digest(self) -> Sha256Digest {
        self.request_digest
    }
    /// Digest of the accepted immutable event frame.
    #[must_use]
    pub const fn event_frame_digest(self) -> Sha256Digest {
        self.event_frame_digest
    }
    /// Exact historical successor-state digest.
    #[must_use]
    pub const fn successor_state_digest(self) -> Sha256Digest {
        self.successor_state_digest
    }
    /// Original commit mode, or legacy when the accepted operation predates receipts.
    #[must_use]
    pub const fn mode(self) -> DebuggerCommitMode {
        self.mode
    }
    /// Original directive/fence authority, retained for new claim-bound operations.
    #[must_use]
    pub const fn original_claim(self) -> Option<DebuggerClaimReceipt> {
        self.original_claim
    }

    pub(super) fn canonical_bytes(self) -> Result<Vec<u8>, DebuggerError> {
        if self.mode == DebuggerCommitMode::Legacy || self.base_request_digest.is_none() {
            return Err(binding("legacy debugger operation receipt cannot be persisted"));
        }
        let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
        writer.write_bytes(RECEIPT_DOMAIN).map_err(codec)?;
        writer.write_fixed(self.command_id.as_bytes()).map_err(codec)?;
        writer.write_fixed(self.event_id.as_bytes()).map_err(codec)?;
        writer.write_fixed(self.job_id.as_bytes()).map_err(codec)?;
        writer.write_u64(self.sequence).map_err(codec)?;
        writer.write_fixed(self.command_digest.as_bytes()).map_err(codec)?;
        writer
            .write_fixed(
                self.base_request_digest
                    .ok_or_else(|| binding("debugger receipt lost its base request digest"))?
                    .as_bytes(),
            )
            .map_err(codec)?;
        writer.write_fixed(self.request_digest.as_bytes()).map_err(codec)?;
        writer.write_fixed(self.event_frame_digest.as_bytes()).map_err(codec)?;
        writer.write_fixed(self.successor_state_digest.as_bytes()).map_err(codec)?;
        writer.write_u8(self.mode.tag()).map_err(codec)?;
        writer.write_option_tag(self.original_claim.is_some()).map_err(codec)?;
        if let Some(claim) = self.original_claim {
            writer.write_fixed(claim.outbox_id.as_bytes()).map_err(codec)?;
            writer.write_u64(claim.fence).map_err(codec)?;
        }
        Ok(writer.into_bytes())
    }

    pub(super) fn decode(bytes: &[u8]) -> Result<Self, DebuggerError> {
        let mut reader = CanonicalReader::new(bytes, CodecLimits::PRODUCTION);
        if reader.read_bytes().map_err(codec)? != RECEIPT_DOMAIN {
            return Err(corrupt("stored debugger receipt has an unsupported domain"));
        }
        let command_id = CommandId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| corrupt("stored debugger receipt command identity is invalid"))?;
        let event_id = EventId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| corrupt("stored debugger receipt event identity is invalid"))?;
        let job_id = DebuggerJobId::new(reader.read_fixed().map_err(codec)?)?;
        let sequence = reader.read_u64().map_err(codec)?;
        let command_digest = Sha256Digest::new(reader.read_fixed().map_err(codec)?);
        let base_request_digest = Sha256Digest::new(reader.read_fixed().map_err(codec)?);
        let request_digest = Sha256Digest::new(reader.read_fixed().map_err(codec)?);
        let event_frame_digest = Sha256Digest::new(reader.read_fixed().map_err(codec)?);
        let successor_state_digest = Sha256Digest::new(reader.read_fixed().map_err(codec)?);
        let mode = DebuggerCommitMode::from_stored_tag(reader.read_u8().map_err(codec)?)?;
        let original_claim = if reader.read_option_tag().map_err(codec)? {
            let outbox_id = OutboxId::new(reader.read_fixed().map_err(codec)?)
                .map_err(|_| corrupt("stored debugger receipt outbox identity is invalid"))?;
            let fence = reader.read_u64().map_err(codec)?;
            if fence == 0 {
                return Err(corrupt("stored debugger receipt claim fence is zero"));
            }
            Some(DebuggerClaimReceipt { outbox_id, fence })
        } else {
            None
        };
        reader.finish().map_err(codec)?;
        let value = Self::retained(
            command_id,
            event_id,
            job_id,
            sequence,
            command_digest,
            base_request_digest,
            request_digest,
            event_frame_digest,
            successor_state_digest,
            mode,
            original_claim,
        )?;
        Ok(value)
    }
}

/// One accepted operation with its immutable outcome and separately reconstructed current state.
#[derive(Debug)]
pub struct CommittedDebuggerOperation {
    batch: CommittedBatch,
    receipt: DebuggerOperationReceipt,
    event: DebuggerEvent,
    historical_state: DebuggerState,
    current_state: DebuggerState,
}

impl CommittedDebuggerOperation {
    pub(super) const fn new(
        batch: CommittedBatch,
        receipt: DebuggerOperationReceipt,
        event: DebuggerEvent,
        historical_state: DebuggerState,
        current_state: DebuggerState,
    ) -> Self {
        Self { batch, receipt, event, historical_state, current_state }
    }

    /// Original immutable C0 command receipt.
    #[must_use]
    pub const fn batch(&self) -> &CommittedBatch {
        &self.batch
    }
    /// Original debugger request and claim receipt.
    #[must_use]
    pub const fn receipt(&self) -> DebuggerOperationReceipt {
        self.receipt
    }
    /// Exact accepted historical event.
    #[must_use]
    pub const fn event(&self) -> &DebuggerEvent {
        &self.event
    }
    /// Exact successor state produced by this operation.
    #[must_use]
    pub const fn historical_state(&self) -> &DebuggerState {
        &self.historical_state
    }
    /// Current state reconstructed independently from the full accepted aggregate.
    #[must_use]
    pub const fn current_state(&self) -> &DebuggerState {
        &self.current_state
    }
    /// Whether this operation still produced the current checkpoint.
    #[must_use]
    pub fn is_current(&self) -> bool {
        self.historical_state == self.current_state
    }
    /// Consumes the operation into original receipt, historical outcome, and current state.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        CommittedBatch,
        DebuggerOperationReceipt,
        DebuggerEvent,
        DebuggerState,
        DebuggerState,
    ) {
        (
            self.batch,
            self.receipt,
            self.event,
            self.historical_state,
            self.current_state,
        )
    }
}

impl Deref for CommittedDebuggerOperation {
    type Target = CommittedBatch;

    fn deref(&self) -> &Self::Target {
        &self.batch
    }
}

#[must_use]
pub(super) fn receipt_key(command_id: CommandId) -> Vec<u8> {
    let mut key = Vec::with_capacity(RECEIPT_KEY_DOMAIN.len() + command_id.as_bytes().len());
    key.extend_from_slice(RECEIPT_KEY_DOMAIN);
    key.extend_from_slice(command_id.as_bytes());
    key
}

pub(super) const fn claim_receipt(
    outbox_id: OutboxId,
    fence: u64,
) -> DebuggerClaimReceipt {
    DebuggerClaimReceipt { outbox_id, fence }
}

fn codec(error: impl core::fmt::Display) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Corruption,
        DebuggerOperation::DecodeProtocol,
        DebuggerRecovery::Quarantine,
        error.to_string(),
    )
}

fn binding(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Binding,
        DebuggerOperation::CommitTransition,
        DebuggerRecovery::Quarantine,
        detail,
    )
}

fn corrupt(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Corruption,
        DebuggerOperation::Recover,
        DebuggerRecovery::Quarantine,
        detail,
    )
}
