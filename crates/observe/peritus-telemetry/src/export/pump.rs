//! Bounded export and shutdown state machine.

use peritus_types::Sha256Digest;

use super::{ExportStreamId, Exporter};
use crate::{BufferCounters, TelemetryBuffer, TelemetryError, TelemetryErrorKind};

/// Result of one bounded flush attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FlushOutcome {
    /// Queue was already empty.
    Empty,
    /// One exact batch was acknowledged.
    Exported {
        /// Acknowledged record count.
        count: u64,
        /// Highest acknowledged stable sequence.
        through_sequence: u64,
    },
}

/// Result of bounded shutdown flushing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShutdownOutcome {
    /// Every queued record was acknowledged and exporter shutdown succeeded.
    Complete,
    /// Flush bound was exhausted with records still pending.
    Pending {
        /// Remaining in-memory records.
        remaining: usize,
    },
}

/// Owns export progress around one bounded queue.
pub struct TelemetryPump {
    stream_id: ExportStreamId,
    buffer: TelemetryBuffer,
}

impl TelemetryPump {
    /// Creates a pump at genesis.
    #[must_use]
    pub const fn new(stream_id: ExportStreamId, buffer: TelemetryBuffer) -> Self {
        Self { stream_id, buffer }
    }

    /// Borrows the bounded queue.
    #[must_use]
    pub const fn buffer(&self) -> &TelemetryBuffer {
        &self.buffer
    }
    /// Mutably borrows the bounded queue for enqueue operations.
    #[must_use]
    pub const fn buffer_mut(&mut self) -> &mut TelemetryBuffer {
        &mut self.buffer
    }
    /// Returns export-stream identity.
    #[must_use]
    pub const fn stream_id(&self) -> ExportStreamId {
        self.stream_id
    }
    /// Returns the highest stable sequence in the contiguous final-disposition prefix.
    #[must_use]
    pub const fn disposed_through_sequence(&self) -> u64 {
        self.buffer.disposed_through_sequence()
    }
    /// Returns the projection-prefix digest through the final-disposition boundary.
    #[must_use]
    pub const fn disposed_prefix(&self) -> Sha256Digest {
        self.buffer.disposed_prefix()
    }
    /// Returns counters captured at the final-disposition boundary.
    #[must_use]
    pub const fn disposed_counters(&self) -> BufferCounters {
        self.buffer.disposed_counters()
    }

    /// Exports at most one configured batch, retaining it on every failure.
    ///
    /// # Errors
    ///
    /// Returns explicit exporter or acknowledgement mismatch failures.
    pub fn flush_one<E: Exporter>(
        &mut self,
        exporter: &mut E,
    ) -> Result<FlushOutcome, TelemetryError> {
        let Some(batch) = self.buffer.batch(self.stream_id)? else {
            return Ok(FlushOutcome::Empty);
        };
        let ack = exporter
            .export(&batch)
            .map_err(|error| TelemetryError::exporter("export telemetry batch", error))?;
        if !ack.matches(&batch) {
            return Err(TelemetryError::new(
                TelemetryErrorKind::AckMismatch,
                "flush telemetry batch",
                "export acknowledgement does not match the complete pending batch",
            ));
        }
        let count = u64::try_from(batch.len()).map_err(|_| {
            TelemetryError::new(
                TelemetryErrorKind::SequenceOverflow,
                "flush telemetry batch",
                "export batch length cannot be represented by telemetry counters",
            )
        })?;
        let through_sequence = batch.last_sequence();
        self.buffer.acknowledge(&batch)?;
        Ok(FlushOutcome::Exported { count, through_sequence })
    }

    /// Flushes at most `maximum_batches`, then shuts down only after the queue is empty.
    ///
    /// # Errors
    ///
    /// Returns the first explicit exporter, acknowledgement, or shutdown failure.
    pub fn shutdown<E: Exporter>(
        &mut self,
        exporter: &mut E,
        maximum_batches: u64,
    ) -> Result<ShutdownOutcome, TelemetryError> {
        let mut flushed = 0_u64;
        while !self.buffer.is_empty() && flushed < maximum_batches {
            self.flush_one(exporter)?;
            flushed = flushed.checked_add(1).ok_or_else(|| {
                TelemetryError::new(
                    TelemetryErrorKind::SequenceOverflow,
                    "shutdown telemetry",
                    "shutdown batch count overflow",
                )
            })?;
        }
        if !self.buffer.is_empty() {
            return Ok(ShutdownOutcome::Pending { remaining: self.buffer.len() });
        }
        exporter
            .shutdown()
            .map_err(|error| TelemetryError::exporter("shutdown telemetry exporter", error))?;
        Ok(ShutdownOutcome::Complete)
    }

    pub(crate) const fn restore_disposition(
        &mut self,
        prefix: Sha256Digest,
        counters: BufferCounters,
    ) {
        self.buffer.restore_boundary(counters, prefix);
    }
}
