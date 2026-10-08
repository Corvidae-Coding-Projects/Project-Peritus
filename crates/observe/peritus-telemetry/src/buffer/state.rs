//! Resident-page accounting, durable overflow, and contiguous disposition tracking.

use std::{collections::VecDeque, mem};

use peritus_types::Sha256Digest;

use super::{
    BufferConfig, BufferCounters, EnqueueOutcome, ObservationLossPolicy, RejectionReason,
};
use crate::{
    ExportBatch, ExportRecord, ExportStreamId, SpillStore, TelemetryError, TelemetryErrorKind,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DispositionPrefix {
    pub(crate) sequence: u64,
    pub(crate) prefix_digest: Sha256Digest,
    pub(crate) accepted_total: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BufferedRecord {
    pub(crate) sequence: u64,
    pub(crate) canonical: Box<[u8]>,
    pub(crate) prefix_digest: Sha256Digest,
    pub(crate) accepted_total: u64,
    pub(crate) gap_before: Option<DispositionPrefix>,
    pub(crate) resident_bytes: usize,
}

impl BufferedRecord {
    pub(crate) const fn disposition_prefix(&self) -> DispositionPrefix {
        DispositionPrefix {
            sequence: self.sequence,
            prefix_digest: self.prefix_digest,
            accepted_total: self.accepted_total,
        }
    }
}

/// Exact contiguous prefix whose observations have all been exported or explicitly lost.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DispositionBoundary {
    sequence: u64,
    prefix_digest: Sha256Digest,
    counters: BufferCounters,
}

impl Default for DispositionBoundary {
    fn default() -> Self {
        Self {
            sequence: 0,
            prefix_digest: Sha256Digest::new([0; 32]),
            counters: BufferCounters::default(),
        }
    }
}

/// Byte-accounted telemetry queue with optional caller-owned durable overflow.
pub struct TelemetryBuffer {
    config: BufferConfig,
    queue: VecDeque<BufferedRecord>,
    resident_records: u64,
    resident_bytes: usize,
    pinned_records: usize,
    spill: Option<SpillStore>,
    counters: BufferCounters,
    submitted_prefix: Sha256Digest,
    disposed: DispositionBoundary,
    trailing_gap: Option<DispositionPrefix>,
}

impl TelemetryBuffer {
    /// Creates an empty buffer for an explicitly lossy policy.
    ///
    /// # Errors
    ///
    /// Rejects `LosslessSpill`, which requires [`Self::with_spill`] and caller-owned storage.
    pub fn new(config: BufferConfig) -> Result<Self, TelemetryError> {
        if config.loss_policy() == ObservationLossPolicy::LosslessSpill {
            return Err(TelemetryError::new(
                TelemetryErrorKind::InvalidConfiguration,
                "create telemetry buffer",
                "lossless spill policy requires caller-owned spill storage",
            ));
        }
        Ok(Self::empty(config, None))
    }

    /// Creates an empty lossless buffer backed by a caller-owned durable spill store.
    ///
    /// # Errors
    ///
    /// Rejects a configuration that did not explicitly select `LosslessSpill`.
    pub fn with_spill(
        config: BufferConfig,
        spill: SpillStore,
    ) -> Result<Self, TelemetryError> {
        if config.loss_policy() != ObservationLossPolicy::LosslessSpill {
            return Err(TelemetryError::new(
                TelemetryErrorKind::InvalidConfiguration,
                "create telemetry buffer",
                "spill storage requires the lossless spill policy",
            ));
        }
        Ok(Self::empty(config, Some(spill)))
    }

    fn empty(config: BufferConfig, spill: Option<SpillStore>) -> Self {
        Self {
            config,
            queue: VecDeque::new(),
            resident_records: 0,
            resident_bytes: 0,
            pinned_records: 0,
            spill,
            counters: BufferCounters::default(),
            submitted_prefix: Sha256Digest::new([0; 32]),
            disposed: DispositionBoundary::default(),
            trailing_gap: None,
        }
    }

    /// Returns the fixed configuration.
    #[must_use]
    pub const fn config(&self) -> BufferConfig {
        self.config
    }
    /// Returns all pending resident and durable-spill observations.
    #[must_use]
    pub fn len(&self) -> u64 {
        self.resident_records.saturating_add(self.spilled_len())
    }
    /// Returns pending resident observation count.
    #[must_use]
    pub const fn resident_len(&self) -> u64 {
        self.resident_records
    }
    /// Returns exact resident physical-page bytes currently charged.
    #[must_use]
    pub const fn resident_bytes(&self) -> usize {
        self.resident_bytes
    }
    /// Returns pending durable-spill observation count.
    #[must_use]
    pub fn spilled_len(&self) -> u64 {
        self.spill.as_ref().map_or(0, SpillStore::pending_records)
    }
    /// Returns whether no resident or durable-spill observations await export.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.resident_records == 0 && self.spilled_len() == 0
    }
    /// Returns monotonic accounting, including observations beyond the checkpointable prefix.
    #[must_use]
    pub const fn counters(&self) -> BufferCounters {
        self.counters
    }

    /// Submits one observation under the caller's explicit loss or spill policy.
    ///
    /// Canonical encoding is deferred when a reject-newest buffer has no free physical page. An
    /// accepted observation is encoded exactly once and thereafter retained as canonical bytes.
    ///
    /// # Errors
    ///
    /// Returns encoding, storage, sequence, or accounting failures without reporting acceptance.
    pub fn enqueue(&mut self, record: ExportRecord) -> Result<EnqueueOutcome, TelemetryError> {
        let sequence = checked_add(self.counters.submitted, 1)?;
        if self.config.loss_policy() == ObservationLossPolicy::RejectNewest
            && self.resident_bytes == self.config.memory_capacity_bytes().get()
        {
            return self.reject(sequence, RejectionReason::MemoryFull);
        }

        let canonical = record.canonical_bytes()?.into_boxed_slice();
        let resident_bytes = resident_charge(self.config, canonical.len())?;
        match self.config.loss_policy() {
            ObservationLossPolicy::RejectNewest => {
                if resident_bytes > self.config.memory_capacity_bytes().get() {
                    return self.reject(sequence, RejectionReason::RecordExceedsMemory);
                }
                if !self.fits(resident_bytes) {
                    return self.reject(sequence, RejectionReason::MemoryFull);
                }
                self.accept_resident(sequence, canonical, resident_bytes, 0)
            }
            ObservationLossPolicy::DropOldest => {
                if resident_bytes > self.config.memory_capacity_bytes().get() {
                    return self.reject(sequence, RejectionReason::RecordExceedsMemory);
                }
                let mut evictions = 0_usize;
                let mut projected_bytes = self.resident_bytes;
                while projected_bytes
                    .checked_add(resident_bytes)
                    .is_none_or(|bytes| bytes > self.config.memory_capacity_bytes().get())
                {
                    if self.pinned_records > 0 {
                        return self.reject(sequence, RejectionReason::InFlightPrefix);
                    }
                    let evicted = self
                        .queue
                        .get(evictions)
                        .ok_or_else(buffer_invariant)?;
                    projected_bytes = projected_bytes
                        .checked_sub(evicted.resident_bytes)
                        .ok_or_else(buffer_invariant)?;
                    evictions = evictions.checked_add(1).ok_or_else(sequence_overflow)?;
                }
                self.accept_resident(sequence, canonical, resident_bytes, evictions)
            }
            ObservationLossPolicy::LosslessSpill => {
                let spill_required = self.spilled_len() > 0
                    || resident_bytes > self.config.memory_capacity_bytes().get()
                    || !self.fits(resident_bytes);
                if spill_required {
                    self.accept_spilled(sequence, canonical)
                } else {
                    self.accept_resident(sequence, canonical, resident_bytes, 0)
                }
            }
        }
    }

    fn accept_resident(
        &mut self,
        sequence: u64,
        canonical: Box<[u8]>,
        resident_bytes: usize,
        evictions: usize,
    ) -> Result<EnqueueOutcome, TelemetryError> {
        let accepted = checked_add(self.counters.accepted, 1)?;
        let evicted_count = u64::try_from(evictions).map_err(|_| sequence_overflow())?;
        let dropped = checked_add(self.counters.dropped, evicted_count)?;
        let next_prefix = accepted_record_prefix(self.submitted_prefix, sequence, &canonical)?;
        let first_dropped_sequence = self.queue.front().map(|record| record.sequence);
        let last_dropped_sequence = evictions
            .checked_sub(1)
            .and_then(|index| self.queue.get(index))
            .map(|record| record.sequence);

        for _ in 0..evictions {
            self.evict_front()?;
        }
        let record = BufferedRecord {
            sequence,
            canonical,
            prefix_digest: next_prefix,
            accepted_total: accepted,
            gap_before: None,
            resident_bytes,
        };
        self.push_resident(record)?;
        self.counters.submitted = sequence;
        self.counters.accepted = accepted;
        self.counters.dropped = dropped;
        self.submitted_prefix = next_prefix;

        match (first_dropped_sequence, last_dropped_sequence) {
            (Some(first), Some(last)) => Ok(EnqueueOutcome::DroppedOldest {
                accepted_sequence: sequence,
                first_dropped_sequence: first,
                last_dropped_sequence: last,
                count: evicted_count,
            }),
            (None, None) => Ok(EnqueueOutcome::Accepted { sequence, resident_bytes }),
            _ => Err(buffer_invariant()),
        }
    }

    fn accept_spilled(
        &mut self,
        sequence: u64,
        canonical: Box<[u8]>,
    ) -> Result<EnqueueOutcome, TelemetryError> {
        let accepted = checked_add(self.counters.accepted, 1)?;
        let next_prefix = accepted_record_prefix(self.submitted_prefix, sequence, &canonical)?;
        let canonical_bytes = u64::try_from(canonical.len()).map_err(|_| sequence_overflow())?;
        let record = BufferedRecord {
            sequence,
            canonical,
            prefix_digest: next_prefix,
            accepted_total: accepted,
            gap_before: None,
            resident_bytes: 0,
        };
        self.spill
            .as_mut()
            .ok_or_else(buffer_invariant)?
            .persist(&record)?;
        self.counters.submitted = sequence;
        self.counters.accepted = accepted;
        self.submitted_prefix = next_prefix;
        Ok(EnqueueOutcome::Spilled { sequence, canonical_bytes })
    }

    fn reject(
        &mut self,
        sequence: u64,
        reason: RejectionReason,
    ) -> Result<EnqueueOutcome, TelemetryError> {
        let dropped = checked_add(self.counters.dropped, 1)?;
        let next_prefix = lost_record_prefix(self.submitted_prefix, sequence);
        self.trailing_gap = Some(DispositionPrefix {
            sequence,
            prefix_digest: next_prefix,
            accepted_total: self.counters.accepted,
        });
        if self.is_empty() {
            self.disposed = disposition_boundary(
                self.trailing_gap.ok_or_else(buffer_invariant)?,
                self.counters.exported,
            )?;
            self.trailing_gap = None;
        }
        self.counters.submitted = sequence;
        self.counters.dropped = dropped;
        self.submitted_prefix = next_prefix;
        Ok(EnqueueOutcome::RejectedNewest { rejected_sequence: sequence, reason })
    }

    fn push_resident(&mut self, mut record: BufferedRecord) -> Result<(), TelemetryError> {
        let queue_was_empty = self.queue.is_empty();
        let advanced_disposition = self
            .trailing_gap
            .filter(|_| queue_was_empty)
            .map(|gap| disposition_boundary(gap, self.counters.exported))
            .transpose()?;
        record.gap_before = if queue_was_empty { None } else { self.trailing_gap.take() };
        if let Some(disposed) = advanced_disposition {
            self.disposed = disposed;
            self.trailing_gap = None;
        }
        self.resident_bytes = self
            .resident_bytes
            .checked_add(record.resident_bytes)
            .ok_or_else(sequence_overflow)?;
        self.resident_records = checked_add(self.resident_records, 1)?;
        self.queue.push_back(record);
        Ok(())
    }

    fn evict_front(&mut self) -> Result<(), TelemetryError> {
        if self.pinned_records > 0 {
            return Err(buffer_invariant());
        }
        let evicted = self.queue.front().ok_or_else(buffer_invariant)?;
        let disposed_prefix = self
            .queue
            .get(1)
            .and_then(|record| record.gap_before)
            .unwrap_or_else(|| evicted.disposition_prefix());
        let disposed = disposition_boundary(disposed_prefix, self.counters.exported)?;
        let resident_bytes = evicted.resident_bytes;
        self.queue.pop_front();
        if let Some(front) = self.queue.front_mut() {
            front.gap_before = None;
        }
        self.resident_bytes = self
            .resident_bytes
            .checked_sub(resident_bytes)
            .ok_or_else(buffer_invariant)?;
        self.resident_records = self
            .resident_records
            .checked_sub(1)
            .ok_or_else(buffer_invariant)?;
        self.disposed = disposed;
        Ok(())
    }

    fn fits(&self, resident_bytes: usize) -> bool {
        self.resident_bytes
            .checked_add(resident_bytes)
            .is_some_and(|bytes| bytes <= self.config.memory_capacity_bytes().get())
    }

    pub(crate) fn batch(
        &self,
        stream_id: ExportStreamId,
    ) -> Result<Option<ExportBatch>, TelemetryError> {
        let target_bytes = self.config.batch_bytes().get();
        let mut records = Vec::new();
        let mut payload_bytes = 0_usize;
        for record in &self.queue {
            let next_bytes = payload_bytes
                .checked_add(record.canonical.len())
                .ok_or_else(sequence_overflow)?;
            if !records.is_empty() && next_bytes > target_bytes {
                break;
            }
            records.push(record.clone());
            payload_bytes = next_bytes;
        }
        if records.len() == self.queue.len() {
            if let Some(spill) = &self.spill {
                let remaining = target_bytes.saturating_sub(payload_bytes);
                if records.is_empty() || remaining > 0 {
                    let mut spilled = spill.load_prefix(if records.is_empty() {
                        target_bytes
                    } else {
                        remaining
                    })?;
                    if !records.is_empty()
                        && spilled
                            .first()
                            .is_some_and(|record| record.canonical.len() > remaining)
                    {
                        spilled.clear();
                    }
                    records.extend(spilled);
                }
            }
        }
        if records.is_empty() {
            Ok(None)
        } else {
            ExportBatch::from_buffered(stream_id, records).map(Some)
        }
    }

    pub(crate) fn pin(&mut self, batch: &ExportBatch) -> Result<(), TelemetryError> {
        if self.pinned_records != 0 {
            return Err(buffer_invariant());
        }
        let resident = batch.len().min(self.queue.len());
        if self.queue.iter().take(resident).zip(batch.items()).any(|(queued, item)| {
            queued.sequence != item.sequence() || queued.canonical.as_ref() != item.canonical_record()
        }) {
            return Err(buffer_invariant());
        }
        self.pinned_records = resident;
        Ok(())
    }

    pub(crate) fn unpin(&mut self) {
        self.pinned_records = 0;
    }

    pub(crate) fn acknowledge(&mut self, batch: &ExportBatch) -> Result<(), TelemetryError> {
        let records = batch.buffered_records();
        if records.is_empty() {
            return Err(TelemetryError::new(
                TelemetryErrorKind::AckMismatch,
                "acknowledge telemetry batch",
                "empty batch cannot be acknowledged",
            ));
        }
        let resident_acked = records.len().min(self.queue.len());
        if self
            .queue
            .iter()
            .take(resident_acked)
            .zip(&records)
            .any(|(queued, exported)| {
                queued.sequence != exported.sequence || queued.canonical != exported.canonical
            })
        {
            return Err(TelemetryError::new(
                TelemetryErrorKind::AckMismatch,
                "acknowledge telemetry batch",
                "resident queue does not match acknowledged canonical bytes",
            ));
        }
        if resident_acked < records.len() && resident_acked != self.queue.len() {
            return Err(buffer_invariant());
        }

        let mut exported = self.counters.exported;
        let mut disposed = self.disposed;
        for record in &records {
            if let Some(gap) = record.gap_before {
                disposition_boundary(gap, exported)?;
            }
            exported = checked_add(exported, 1)?;
            disposed = disposition_boundary(record.disposition_prefix(), exported)?;
        }
        if let Some(next) = self.queue.get(resident_acked) {
            if let Some(gap) = next.gap_before {
                disposed = disposition_boundary(gap, exported)?;
            }
        } else if records.len() == resident_acked && self.spilled_len() == 0 {
            if let Some(gap) = self.trailing_gap {
                disposed = disposition_boundary(gap, exported)?;
            }
        }

        let removed_bytes = self
            .queue
            .iter()
            .take(resident_acked)
            .try_fold(0_usize, |total, record| {
                total.checked_add(record.resident_bytes).ok_or_else(sequence_overflow)
            })?;
        let remaining_resident_bytes = self
            .resident_bytes
            .checked_sub(removed_bytes)
            .ok_or_else(buffer_invariant)?;
        let resident_acked_u64 =
            u64::try_from(resident_acked).map_err(|_| sequence_overflow())?;
        let remaining_resident_records = self
            .resident_records
            .checked_sub(resident_acked_u64)
            .ok_or_else(buffer_invariant)?;

        let spill_records = &records[resident_acked..];
        if !spill_records.is_empty() {
            self.spill
                .as_mut()
                .ok_or_else(buffer_invariant)?
                .remove_prefix(spill_records)?;
        }
        self.queue.drain(..resident_acked);
        self.resident_bytes = remaining_resident_bytes;
        self.resident_records = remaining_resident_records;
        self.pinned_records = 0;
        if let Some(front) = self.queue.front_mut() {
            front.gap_before = None;
        } else if self.spilled_len() == 0 {
            self.trailing_gap = None;
        }
        self.counters.exported = exported;
        self.disposed = disposed;
        Ok(())
    }

    pub(crate) const fn disposed_through_sequence(&self) -> u64 {
        self.disposed.sequence
    }

    pub(crate) const fn disposed_prefix(&self) -> Sha256Digest {
        self.disposed.prefix_digest
    }

    pub(crate) const fn disposed_counters(&self) -> BufferCounters {
        self.disposed.counters
    }

    pub(crate) fn spill_stream_id(&self) -> Option<ExportStreamId> {
        self.spill.as_ref().map(SpillStore::stream_id)
    }

    pub(crate) const fn restore_boundary(
        &mut self,
        counters: BufferCounters,
        prefix_digest: Sha256Digest,
    ) {
        self.counters = counters;
        self.submitted_prefix = prefix_digest;
        self.disposed =
            DispositionBoundary { sequence: counters.submitted(), prefix_digest, counters };
        self.trailing_gap = None;
    }
}

pub(crate) fn accepted_record_prefix(
    prior: Sha256Digest,
    sequence: u64,
    canonical: &[u8],
) -> Result<Sha256Digest, TelemetryError> {
    let encoded_length = u64::try_from(canonical.len()).map_err(|_| {
        TelemetryError::new(
            TelemetryErrorKind::SequenceOverflow,
            "encode telemetry prefix",
            "record length exceeds the portable prefix representation",
        )
    })?;
    let capacity = 72_usize.checked_add(canonical.len()).ok_or_else(sequence_overflow)?;
    let mut bytes = Vec::with_capacity(capacity);
    bytes.extend_from_slice(b"PERITUS-C7-EXPORT-PREFIX-V1\0");
    bytes.extend_from_slice(prior.as_bytes());
    bytes.extend_from_slice(&sequence.to_be_bytes());
    bytes.extend_from_slice(&encoded_length.to_be_bytes());
    bytes.extend_from_slice(canonical);
    Ok(peritus_codec::sha256(&bytes))
}

pub(crate) fn lost_record_prefix(prior: Sha256Digest, sequence: u64) -> Sha256Digest {
    let mut bytes = Vec::with_capacity(70);
    bytes.extend_from_slice(b"PERITUS-C7-EXPORT-LOSS-PREFIX-V1\0");
    bytes.extend_from_slice(prior.as_bytes());
    bytes.extend_from_slice(&sequence.to_be_bytes());
    Ok(peritus_codec::sha256(&bytes))
}

fn resident_charge(config: BufferConfig, canonical_bytes: usize) -> Result<usize, TelemetryError> {
    let exact = mem::size_of::<BufferedRecord>()
        .checked_add(canonical_bytes)
        .ok_or_else(sequence_overflow)?;
    let page = config.memory_page_bytes().get();
    let pages = exact
        .checked_add(page - 1)
        .and_then(|rounded| rounded.checked_div(page))
        .ok_or_else(sequence_overflow)?;
    pages.checked_mul(page).ok_or_else(sequence_overflow)
}

fn disposition_boundary(
    prefix: DispositionPrefix,
    exported: u64,
) -> Result<DispositionBoundary, TelemetryError> {
    let dropped = prefix.sequence.checked_sub(exported).ok_or_else(buffer_invariant)?;
    let counters =
        BufferCounters::from_parts(prefix.sequence, prefix.accepted_total, dropped, exported)?;
    Ok(DispositionBoundary {
        sequence: prefix.sequence,
        prefix_digest: prefix.prefix_digest,
        counters,
    })
}

fn checked_add(value: u64, added: u64) -> Result<u64, TelemetryError> {
    value.checked_add(added).ok_or_else(sequence_overflow)
}

const fn sequence_overflow() -> TelemetryError {
    TelemetryError::new(
        TelemetryErrorKind::SequenceOverflow,
        "update telemetry buffer",
        "telemetry sequence or byte accounting overflow",
    )
}

const fn buffer_invariant() -> TelemetryError {
    TelemetryError::new(
        TelemetryErrorKind::RecoveryMismatch,
        "update telemetry dispositions",
        "buffer disposition state is internally inconsistent",
    )
}
