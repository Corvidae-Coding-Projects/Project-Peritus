//! Queue state and contiguous final-disposition tracking.

use std::collections::VecDeque;

use peritus_types::Sha256Digest;

use super::{BackpressurePolicy, BufferConfig, BufferCounters, EnqueueOutcome};
use crate::{ExportBatch, ExportRecord, ExportStreamId, TelemetryError, TelemetryErrorKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DispositionPrefix {
    sequence: u64,
    prefix_digest: Sha256Digest,
    accepted_total: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BufferedRecord {
    pub(crate) sequence: u64,
    pub(crate) record: ExportRecord,
    prefix_digest: Sha256Digest,
    accepted_total: u64,
    gap_before: Option<DispositionPrefix>,
}

impl BufferedRecord {
    const fn disposition_prefix(&self) -> DispositionPrefix {
        DispositionPrefix {
            sequence: self.sequence,
            prefix_digest: self.prefix_digest,
            accepted_total: self.accepted_total,
        }
    }
}

/// Exact contiguous prefix whose records have all been exported or dropped.
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

/// Bounded redaction-safe telemetry queue.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TelemetryBuffer {
    config: BufferConfig,
    queue: VecDeque<BufferedRecord>,
    counters: BufferCounters,
    submitted_prefix: Sha256Digest,
    disposed: DispositionBoundary,
    trailing_gap: Option<DispositionPrefix>,
}

impl TelemetryBuffer {
    /// Creates an empty queue.
    #[must_use]
    pub fn new(config: BufferConfig) -> Self {
        Self {
            config,
            queue: VecDeque::with_capacity(config.capacity()),
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
    /// Returns queued record count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.queue.len()
    }
    /// Returns whether no records await export.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
    /// Returns monotonic accounting, including records beyond the checkpointable prefix.
    #[must_use]
    pub const fn counters(&self) -> BufferCounters {
        self.counters
    }

    /// Submits one record under deterministic bounded backpressure.
    ///
    /// # Errors
    ///
    /// Returns sequence, encoding, or accounting overflow without partially accepting the record.
    pub fn enqueue(&mut self, record: ExportRecord) -> Result<EnqueueOutcome, TelemetryError> {
        let sequence = checked_add(self.counters.submitted, 1)?;
        let next_prefix = record_prefix(self.submitted_prefix, sequence, &record)?;
        let full = self.queue.len() == self.config.capacity();
        let accepted_delta = match (full, self.config.policy()) {
            (true, BackpressurePolicy::RejectNewest) => 0,
            _ => 1,
        };
        let accepted = checked_add(self.counters.accepted, accepted_delta)?;
        let dropped = checked_add(self.counters.dropped, u64::from(full))?;

        let outcome = match (full, self.config.policy()) {
            (true, BackpressurePolicy::RejectNewest) => {
                self.trailing_gap = Some(DispositionPrefix {
                    sequence,
                    prefix_digest: next_prefix,
                    accepted_total: accepted,
                });
                EnqueueOutcome::RejectedNewest { rejected_sequence: sequence }
            }
            (true, BackpressurePolicy::DropOldest) => {
                let evicted = self.queue.front().ok_or_else(buffer_invariant)?;
                let disposed_prefix = self
                    .queue
                    .get(1)
                    .and_then(|record| record.gap_before)
                    .unwrap_or_else(|| evicted.disposition_prefix());
                let disposed = disposition_boundary(disposed_prefix, self.counters.exported)?;
                let dropped_sequence = evicted.sequence;
                self.queue.pop_front();
                if let Some(front) = self.queue.front_mut() {
                    front.gap_before = None;
                }
                self.disposed = disposed;
                self.queue.push_back(BufferedRecord {
                    sequence,
                    record,
                    prefix_digest: next_prefix,
                    accepted_total: accepted,
                    gap_before: self.trailing_gap.take(),
                });
                EnqueueOutcome::DroppedOldest { accepted_sequence: sequence, dropped_sequence }
            }
            (false, _) => {
                let queue_was_empty = self.queue.is_empty();
                let advanced_disposition = self
                    .trailing_gap
                    .filter(|_| queue_was_empty)
                    .map(|gap| disposition_boundary(gap, self.counters.exported))
                    .transpose()?;
                let gap_before = if queue_was_empty { None } else { self.trailing_gap.take() };
                if let Some(disposed) = advanced_disposition {
                    self.disposed = disposed;
                    self.trailing_gap = None;
                }
                self.queue.push_back(BufferedRecord {
                    sequence,
                    record,
                    prefix_digest: next_prefix,
                    accepted_total: accepted,
                    gap_before,
                });
                EnqueueOutcome::Accepted { sequence }
            }
        };
        self.counters.submitted = sequence;
        self.counters.accepted = accepted;
        self.counters.dropped = dropped;
        self.submitted_prefix = next_prefix;
        Ok(outcome)
    }

    pub(crate) fn batch(
        &self,
        stream_id: ExportStreamId,
    ) -> Result<Option<ExportBatch>, TelemetryError> {
        let records = self.queue.iter().take(self.config.batch_size()).cloned().collect::<Vec<_>>();
        if records.is_empty() {
            Ok(None)
        } else {
            ExportBatch::from_buffered(stream_id, records).map(Some)
        }
    }

    pub(crate) fn acknowledge(&mut self, batch: &ExportBatch) -> Result<(), TelemetryError> {
        let queued = self.queue.iter().take(batch.len()).collect::<Vec<_>>();
        if queued.len() != batch.len()
            || queued.iter().zip(batch.items()).any(|(queued, exported)| {
                queued.sequence != exported.sequence() || queued.record != *exported.record()
            })
        {
            return Err(TelemetryError::new(
                TelemetryErrorKind::AckMismatch,
                "acknowledge telemetry batch",
                "pending queue does not match acknowledged batch",
            ));
        }
        if queued.is_empty() {
            return Err(TelemetryError::new(
                TelemetryErrorKind::AckMismatch,
                "acknowledge telemetry batch",
                "empty batch cannot be acknowledged",
            ));
        }

        let mut exported = self.counters.exported;
        let mut disposed = self.disposed;
        for record in &queued {
            if let Some(gap) = record.gap_before {
                disposition_boundary(gap, exported)?;
            }
            exported = checked_add(exported, 1)?;
            disposed = disposition_boundary(record.disposition_prefix(), exported)?;
        }
        if let Some(next) = self.queue.get(batch.len()) {
            if let Some(gap) = next.gap_before {
                disposed = disposition_boundary(gap, exported)?;
            }
        } else if let Some(gap) = self.trailing_gap {
            disposed = disposition_boundary(gap, exported)?;
        }

        self.queue.drain(..batch.len());
        if let Some(front) = self.queue.front_mut() {
            front.gap_before = None;
        } else {
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

pub(crate) fn record_prefix(
    prior: Sha256Digest,
    sequence: u64,
    record: &ExportRecord,
) -> Result<Sha256Digest, TelemetryError> {
    let encoded = record.canonical_bytes()?;
    let encoded_length = u64::try_from(encoded.len()).map_err(|_| {
        TelemetryError::new(
            TelemetryErrorKind::SequenceOverflow,
            "encode telemetry prefix",
            "record length exceeds the portable prefix representation",
        )
    })?;
    let mut bytes = Vec::with_capacity(72 + encoded.len());
    bytes.extend_from_slice(b"PERITUS-C7-EXPORT-PREFIX-V1\0");
    bytes.extend_from_slice(prior.as_bytes());
    bytes.extend_from_slice(&sequence.to_be_bytes());
    bytes.extend_from_slice(&encoded_length.to_be_bytes());
    bytes.extend_from_slice(&encoded);
    Ok(peritus_codec::sha256(&bytes))
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
    value.checked_add(added).ok_or_else(|| {
        TelemetryError::new(
            TelemetryErrorKind::SequenceOverflow,
            "update telemetry counters",
            "telemetry accounting overflow",
        )
    })
}

const fn buffer_invariant() -> TelemetryError {
    TelemetryError::new(
        TelemetryErrorKind::RecoveryMismatch,
        "update telemetry dispositions",
        "buffer disposition state is internally inconsistent",
    )
}
