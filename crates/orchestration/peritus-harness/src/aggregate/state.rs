//! Complete bounded harness aggregate state and canonical checkpoint data.

use std::collections::{BTreeMap, BTreeSet};

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecLimits};
use peritus_types::{EventId, HarnessId, Sha256Digest};

use crate::{
    domain::{HarnessHistory, HarnessLimitKind, HarnessLimits},
    materialization::{
        MaterializationFailure, MaterializationPlan, MaterializationPlanId, MaterializationReceipt,
        MaterializationReceiptId,
    },
};

use super::{AggregateError, AggregateErrorKind, AggregateRecovery};

const STATE_DOMAIN: &[u8] = b"peritus.harness.state.v1\0";

/// Durable outbox-delivery observation for one pending materialization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryState {
    /// Stable directive remains eligible for claim/redelivery.
    Pending,
    /// Transport acknowledged delivery; materialization remains unsettled.
    Acknowledged {
        /// Caller-observed durable delivery time.
        delivered_at_millis: u64,
    },
}

/// Exact committed materialization work retained across crashes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingMaterialization {
    plan: MaterializationPlan,
    pub(crate) delivery: DeliveryState,
}

impl PendingMaterialization {
    pub(crate) const fn new(plan: MaterializationPlan) -> Self {
        Self { plan, delivery: DeliveryState::Pending }
    }

    /// Returns the exact committed plan.
    #[must_use]
    pub const fn plan(&self) -> &MaterializationPlan {
        &self.plan
    }
    /// Returns durable delivery state.
    #[must_use]
    pub const fn delivery(&self) -> DeliveryState {
        self.delivery
    }
}

/// Complete authoritative current E1 aggregate checkpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HarnessState {
    pub(crate) harness_id: HarnessId,
    pub(crate) limits: HarnessLimits,
    pub(crate) sequence: u64,
    pub(crate) last_event_id: EventId,
    pub(crate) state_digest: Sha256Digest,
    pub(crate) history: HarnessHistory,
    pub(crate) pending: BTreeMap<MaterializationPlanId, PendingMaterialization>,
    pub(crate) receipts: BTreeMap<MaterializationReceiptId, MaterializationReceipt>,
    pub(crate) failures: Vec<MaterializationFailure>,
    pub(crate) retired_receipts: BTreeSet<MaterializationReceiptId>,
}

impl HarnessState {
    /// Returns the harness lineage.
    #[must_use]
    pub const fn harness_id(&self) -> HarnessId {
        self.harness_id
    }
    /// Returns tightened E1 limits.
    #[must_use]
    pub const fn limits(&self) -> HarnessLimits {
        self.limits
    }
    /// Returns the positive applied-event sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
    /// Returns current aggregate head event.
    #[must_use]
    pub const fn last_event_id(&self) -> EventId {
        self.last_event_id
    }
    /// Returns complete state digest.
    #[must_use]
    pub const fn state_digest(&self) -> Sha256Digest {
        self.state_digest
    }
    /// Returns append-only revision history.
    #[must_use]
    pub const fn history(&self) -> &HarnessHistory {
        &self.history
    }
    /// Returns pending plans in plan-identity order.
    #[must_use]
    pub const fn pending(&self) -> &BTreeMap<MaterializationPlanId, PendingMaterialization> {
        &self.pending
    }
    /// Returns hot receipts in receipt-identity order.
    #[must_use]
    pub const fn receipts(&self) -> &BTreeMap<MaterializationReceiptId, MaterializationReceipt> {
        &self.receipts
    }
    /// Returns bounded retained failure diagnostics.
    #[must_use]
    pub fn failures(&self) -> &[MaterializationFailure] {
        &self.failures
    }
    /// Returns receipt identities retired from the hot projection.
    #[must_use]
    pub const fn retired_receipts(&self) -> &BTreeSet<MaterializationReceiptId> {
        &self.retired_receipts
    }

    /// Returns one exact pending plan.
    #[must_use]
    pub fn pending_plan(&self, id: MaterializationPlanId) -> Option<&PendingMaterialization> {
        self.pending.get(&id)
    }

    /// Returns one retained receipt.
    #[must_use]
    pub fn receipt(&self, id: MaterializationReceiptId) -> Option<&MaterializationReceipt> {
        self.receipts.get(&id)
    }

    /// Returns canonical complete checkpoint bytes including their state digest.
    ///
    /// # Errors
    /// Returns a codec error if configured E1 bounds cannot represent complete state.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, AggregateError> {
        let identity = self.canonical_identity_bytes()?;
        let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
        writer.write_fixed(STATE_DOMAIN).map_err(codec)?;
        writer.write_fixed(self.state_digest.as_bytes()).map_err(codec)?;
        writer.write_bytes(&identity).map_err(codec)?;
        Ok(writer.into_bytes())
    }

    /// Reconstructs a complete checkpoint and verifies its digest and all nested canonical bytes.
    ///
    /// # Errors
    /// Rejects malformed, noncanonical, trailing, oversized, or digest-mismatched state.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, AggregateError> {
        let mut reader = CanonicalReader::new(bytes, CodecLimits::PRODUCTION);
        if reader.read_fixed::<25>().map_err(codec)?.as_slice() != STATE_DOMAIN {
            return Err(replay_error("state checkpoint domain separator differs"));
        }
        let state_digest = Sha256Digest::new(reader.read_fixed().map_err(codec)?);
        let identity = reader.read_bytes().map_err(codec)?;
        reader.finish().map_err(codec)?;
        let mut state = Self::decode_identity(identity)?;
        if peritus_codec::sha256(identity) != state_digest {
            return Err(replay_error("state checkpoint digest disagrees with exact bytes"));
        }
        state.state_digest = state_digest;
        Ok(state)
    }

    pub(crate) fn refresh_digest(&mut self) -> Result<(), AggregateError> {
        self.state_digest = peritus_codec::sha256(&self.canonical_identity_bytes()?);
        Ok(())
    }

    fn canonical_identity_bytes(&self) -> Result<Vec<u8>, AggregateError> {
        let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
        writer.write_fixed(self.harness_id.as_bytes()).map_err(codec)?;
        encode_limits(&mut writer, self.limits)?;
        writer.write_u64(self.sequence).map_err(codec)?;
        writer.write_fixed(self.last_event_id.as_bytes()).map_err(codec)?;
        writer.write_bytes(&self.history.canonical_snapshot()).map_err(codec)?;
        writer.write_collection_len(self.pending.len()).map_err(codec)?;
        for pending in self.pending.values() {
            writer.write_bytes(&pending.plan.canonical_bytes().map_err(nested)?).map_err(codec)?;
            match pending.delivery {
                DeliveryState::Pending => writer.write_u8(1).map_err(codec)?,
                DeliveryState::Acknowledged { delivered_at_millis } => {
                    writer.write_u8(2).map_err(codec)?;
                    writer.write_u64(delivered_at_millis).map_err(codec)?;
                }
            }
        }
        writer.write_collection_len(self.receipts.len()).map_err(codec)?;
        for receipt in self.receipts.values() {
            writer.write_bytes(&receipt.canonical_bytes().map_err(nested)?).map_err(codec)?;
        }
        writer.write_collection_len(self.failures.len()).map_err(codec)?;
        for failure in &self.failures {
            writer.write_bytes(&failure.canonical_bytes().map_err(nested)?).map_err(codec)?;
        }
        writer.write_collection_len(self.retired_receipts.len()).map_err(codec)?;
        for receipt in &self.retired_receipts {
            writer.write_fixed(receipt.as_bytes()).map_err(codec)?;
        }
        Ok(writer.into_bytes())
    }

    fn decode_identity(bytes: &[u8]) -> Result<Self, AggregateError> {
        let mut reader = CanonicalReader::new(bytes, CodecLimits::PRODUCTION);
        let harness_id = HarnessId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| replay_error("state harness identity is zero"))?;
        let limits = decode_limits(&mut reader)?;
        let sequence = reader.read_u64().map_err(codec)?;
        if sequence == 0 {
            return Err(replay_error("materialized aggregate state has zero sequence"));
        }
        let last_event_id = EventId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| replay_error("state last event identity is zero"))?;
        let history =
            HarnessHistory::decode_canonical_snapshot(reader.read_bytes().map_err(codec)?, limits)
                .map_err(nested)?;
        if history.genesis().harness_id() != harness_id || history.limits() != limits {
            return Err(replay_error("state history identity or limits disagree"));
        }
        let pending = decode_pending(&mut reader, limits)?;
        let receipts = decode_receipts(&mut reader, limits)?;
        let failures = decode_failures(&mut reader, limits)?;
        let retired_receipts = decode_retired(&mut reader)?;
        reader.finish().map_err(codec)?;
        Ok(Self {
            harness_id,
            limits,
            sequence,
            last_event_id,
            state_digest: Sha256Digest::new([0; 32]),
            history,
            pending,
            receipts,
            failures,
            retired_receipts,
        })
    }

    /// Returns all finalized artifact roots required by hot state and immutable history.
    #[must_use]
    pub fn artifact_roots(&self) -> BTreeSet<Sha256Digest> {
        let mut roots = BTreeSet::new();
        for revision in self.history.revisions() {
            for root in revision.artifact_roots() {
                roots.insert(root.content_digest());
                if let Some(executable) = root.executable_artifact_digest() {
                    roots.insert(executable.digest());
                }
            }
        }
        for pending in self.pending.values() {
            for operation in pending.plan.operations() {
                if let Some(digest) = operation.artifact_digest() {
                    roots.insert(digest);
                }
            }
        }
        for receipt in self.receipts.values() {
            roots.insert(receipt.workspace_manifest_artifact());
        }
        roots
    }
}

fn decode_pending(
    reader: &mut CanonicalReader<'_>,
    limits: HarnessLimits,
) -> Result<BTreeMap<MaterializationPlanId, PendingMaterialization>, AggregateError> {
    let count = reader.read_collection_len().map_err(codec)?;
    if u64::try_from(count).unwrap_or(u64::MAX) > limits.max_receipt_history() {
        return Err(limit_error("pending materializations exceed receipt-history limit"));
    }
    let mut values = BTreeMap::new();
    for _ in 0..count {
        let plan = MaterializationPlan::decode_canonical(reader.read_bytes().map_err(codec)?)
            .map_err(nested)?;
        let delivery = match reader.read_u8().map_err(codec)? {
            1 => DeliveryState::Pending,
            2 => DeliveryState::Acknowledged {
                delivered_at_millis: reader.read_u64().map_err(codec)?,
            },
            _ => return Err(replay_error("unknown pending delivery state")),
        };
        if values.insert(plan.id(), PendingMaterialization { plan, delivery }).is_some() {
            return Err(replay_error("state repeats a pending plan"));
        }
    }
    Ok(values)
}

fn decode_receipts(
    reader: &mut CanonicalReader<'_>,
    limits: HarnessLimits,
) -> Result<BTreeMap<MaterializationReceiptId, MaterializationReceipt>, AggregateError> {
    let count = reader.read_collection_len().map_err(codec)?;
    if u64::try_from(count).unwrap_or(u64::MAX) > limits.max_receipt_history() {
        return Err(limit_error("hot receipts exceed receipt-history limit"));
    }
    let mut values = BTreeMap::new();
    for _ in 0..count {
        let receipt = MaterializationReceipt::decode_canonical(reader.read_bytes().map_err(codec)?)
            .map_err(nested)?;
        if values.insert(receipt.id(), receipt).is_some() {
            return Err(replay_error("state repeats a receipt"));
        }
    }
    Ok(values)
}

fn decode_failures(
    reader: &mut CanonicalReader<'_>,
    limits: HarnessLimits,
) -> Result<Vec<MaterializationFailure>, AggregateError> {
    let count = reader.read_collection_len().map_err(codec)?;
    if u64::try_from(count).unwrap_or(u64::MAX) > limits.max_retained_diagnostics() {
        return Err(limit_error("failures exceed retained-diagnostics limit"));
    }
    (0..count)
        .map(|_| {
            MaterializationFailure::decode_canonical(reader.read_bytes().map_err(codec)?)
                .map_err(nested)
        })
        .collect()
}

fn decode_retired(
    reader: &mut CanonicalReader<'_>,
) -> Result<BTreeSet<MaterializationReceiptId>, AggregateError> {
    let count = reader.read_collection_len().map_err(codec)?;
    let mut values = BTreeSet::new();
    for _ in 0..count {
        let id = MaterializationReceiptId::decode(reader.read_fixed().map_err(codec)?)
            .map_err(nested)?;
        if !values.insert(id) {
            return Err(replay_error("state repeats a retired receipt"));
        }
    }
    Ok(values)
}

const LIMIT_KINDS: [HarnessLimitKind; 11] = [
    HarnessLimitKind::ManifestBytes,
    HarnessLimitKind::Components,
    HarnessLimitKind::DependencyEdges,
    HarnessLimitKind::DependencyFanOut,
    HarnessLimitKind::ComponentBytes,
    HarnessLimitKind::TotalMaterializedBytes,
    HarnessLimitKind::RevisionHistory,
    HarnessLimitKind::ReceiptHistory,
    HarnessLimitKind::EventBytes,
    HarnessLimitKind::StateBytes,
    HarnessLimitKind::RetainedDiagnostics,
];

fn encode_limits(
    writer: &mut CanonicalWriter,
    limits: HarnessLimits,
) -> Result<(), AggregateError> {
    for kind in LIMIT_KINDS {
        writer.write_u64(limits.value(kind)).map_err(codec)?;
    }
    Ok(())
}

fn decode_limits(reader: &mut CanonicalReader<'_>) -> Result<HarnessLimits, AggregateError> {
    let mut values = Vec::with_capacity(LIMIT_KINDS.len());
    for kind in LIMIT_KINDS {
        values.push((kind, reader.read_u64().map_err(codec)?));
    }
    HarnessLimits::compiled().tightened(&values).map_err(nested)
}

fn codec(error: impl core::fmt::Display) -> AggregateError {
    AggregateError::new(AggregateErrorKind::Codec, AggregateRecovery::Quarantine, error.to_string())
}
fn nested(error: impl core::fmt::Display) -> AggregateError {
    codec(error)
}
fn replay_error(detail: &'static str) -> AggregateError {
    AggregateError::new(AggregateErrorKind::Replay, AggregateRecovery::Quarantine, detail)
}
fn limit_error(detail: &'static str) -> AggregateError {
    AggregateError::new(AggregateErrorKind::LimitExceeded, AggregateRecovery::Quarantine, detail)
}
