//! Caller-polled export and shutdown state machine without cumulative work ceilings.

use peritus_types::Sha256Digest;

use super::{
    ExportBatch, ExportPoll, ExportPollControl, ExportProgress, ExportStreamId, Exporter,
    ExporterShutdownPoll,
};
use crate::{
    BufferCounters, EnqueueOutcome, ExportRecord, TelemetryBuffer, TelemetryError,
    TelemetryErrorKind,
};

/// Exact resident and durable-spill records awaiting acknowledgement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PendingTelemetry {
    resident: u64,
    spilled: u64,
}

impl PendingTelemetry {
    /// Returns pending byte-accounted resident records.
    #[must_use]
    pub const fn resident(self) -> u64 {
        self.resident
    }
    /// Returns pending caller-owned durable-spill records.
    #[must_use]
    pub const fn spilled(self) -> u64 {
        self.spilled
    }
    /// Returns all pending records.
    #[must_use]
    pub const fn total(self) -> u64 {
        self.resident.saturating_add(self.spilled)
    }
}

/// Action a caller can take to continue an incomplete drain.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DrainAction {
    /// Poll the active or next exact export batch again.
    PollExport,
    /// Poll exporter-owned resource shutdown again.
    PollExporterShutdown,
}

/// Result of one finite caller-owned flush poll.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FlushOutcome {
    /// Queue was already empty.
    Empty,
    /// The exporter owns an incomplete operation and must be polled again.
    Pending(ExportProgress),
    /// One exact batch was acknowledged.
    Exported {
        /// Acknowledged record count.
        count: u64,
        /// Highest acknowledged stable sequence.
        through_sequence: u64,
        /// Final exporter progress for this batch.
        progress: ExportProgress,
    },
    /// Export cancellation completed without losing or accepting the pending batch.
    Cancelled {
        /// Records that remain under buffer or durable-spill custody.
        remaining: PendingTelemetry,
        /// Final truthful cleanup progress.
        progress: ExportProgress,
    },
}

/// Result of one finite caller-owned shutdown poll.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShutdownOutcome {
    /// Every queued record was acknowledged and exporter cleanup completed.
    Complete,
    /// Drain work remains and the caller receives the exact next action.
    Pending {
        /// Records that remain under buffer or durable-spill custody.
        remaining: PendingTelemetry,
        /// Operation the caller should poll next.
        next: DrainAction,
        /// Latest content-free exporter progress.
        progress: ExportProgress,
    },
    /// Caller-requested cancellation completed without claiming a full shutdown.
    Cancelled {
        /// Records that remain under buffer or durable-spill custody.
        remaining: PendingTelemetry,
        /// Operation that remains necessary for a later drain.
        next: DrainAction,
        /// Final truthful cleanup progress for the cancelled operation.
        progress: ExportProgress,
    },
}

struct InFlight {
    batch: ExportBatch,
    operation_active: bool,
}

/// Owns canonical export progress around one byte-accounted queue.
pub struct TelemetryPump {
    stream_id: ExportStreamId,
    buffer: TelemetryBuffer,
    in_flight: Option<InFlight>,
    exporter_shutdown_started: bool,
    closed: bool,
}

impl TelemetryPump {
    /// Creates a pump at genesis and validates durable-spill ownership.
    ///
    /// # Errors
    ///
    /// Rejects a spill store owned by a different logical export stream.
    pub fn new(
        stream_id: ExportStreamId,
        buffer: TelemetryBuffer,
    ) -> Result<Self, TelemetryError> {
        if buffer.spill_stream_id().is_some_and(|owner| owner != stream_id) {
            return Err(TelemetryError::new(
                TelemetryErrorKind::InvalidConfiguration,
                "create telemetry pump",
                "durable spill store belongs to another export stream",
            ));
        }
        Ok(Self {
            stream_id,
            buffer,
            in_flight: None,
            exporter_shutdown_started: false,
            closed: false,
        })
    }

    /// Borrows the byte-accounted queue.
    #[must_use]
    pub const fn buffer(&self) -> &TelemetryBuffer {
        &self.buffer
    }
    /// Submits one redaction-safe observation while preserving any pinned export prefix.
    ///
    /// # Errors
    ///
    /// Returns explicit encoding, storage, sequence, or accounting failures.
    pub fn enqueue(&mut self, record: ExportRecord) -> Result<EnqueueOutcome, TelemetryError> {
        if self.closed {
            return Err(TelemetryError::new(
                TelemetryErrorKind::InvalidConfiguration,
                "enqueue telemetry",
                "telemetry exporter has completed shutdown",
            ));
        }
        self.buffer.enqueue(record)
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
    /// Returns exact pending resident and spill counts.
    #[must_use]
    pub fn pending(&self) -> PendingTelemetry {
        PendingTelemetry {
            resident: self.buffer.resident_len(),
            spilled: self.buffer.spilled_len(),
        }
    }

    /// Advances at most one exporter-owned operation poll, retaining the exact batch on failure.
    ///
    /// The control byte count bounds only this poll. It never rejects pending records or ends the
    /// export session. Cancellation must be acknowledged by the exporter after owned cleanup.
    ///
    /// # Errors
    ///
    /// Returns explicit exporter, acknowledgement, storage, or sequence failures.
    pub fn poll_flush<E: Exporter>(
        &mut self,
        exporter: &mut E,
        control: ExportPollControl,
    ) -> Result<FlushOutcome, TelemetryError> {
        if self.closed {
            return Err(TelemetryError::new(
                TelemetryErrorKind::InvalidConfiguration,
                "poll telemetry export",
                "telemetry exporter has completed shutdown",
            ));
        }
        if self.in_flight.is_none() {
            let Some(batch) = self.buffer.batch(self.stream_id)? else {
                return Ok(FlushOutcome::Empty);
            };
            self.buffer.pin(&batch)?;
            self.in_flight = Some(InFlight { batch, operation_active: false });
        }

        let needs_begin = self
            .in_flight
            .as_ref()
            .is_some_and(|in_flight| !in_flight.operation_active);
        if needs_begin {
            let begin = exporter.begin_export(
                &self.in_flight.as_ref().ok_or_else(pump_invariant)?.batch,
            );
            if let Err(error) = begin {
                self.in_flight = None;
                self.buffer.unpin();
                return Err(TelemetryError::exporter("begin telemetry export", error));
            }
            self.in_flight.as_mut().ok_or_else(pump_invariant)?.operation_active = true;
        }

        let polled = exporter.poll_export(control);
        let polled = match polled {
            Ok(polled) => polled,
            Err(error) => {
                if error.cleanup_complete() {
                    self.in_flight = None;
                    self.buffer.unpin();
                }
                return Err(TelemetryError::exporter("poll telemetry export", error));
            }
        };
        match polled {
            ExportPoll::Pending(progress) => Ok(FlushOutcome::Pending(progress)),
            ExportPoll::Cancelled(progress) => {
                self.in_flight = None;
                self.buffer.unpin();
                Ok(FlushOutcome::Cancelled { remaining: self.pending(), progress })
            }
            ExportPoll::Accepted { ack, progress } => {
                let in_flight = self.in_flight.as_mut().ok_or_else(pump_invariant)?;
                in_flight.operation_active = false;
                if !ack.matches(&in_flight.batch) {
                    return Err(TelemetryError::new(
                        TelemetryErrorKind::AckMismatch,
                        "poll telemetry export",
                        "export acknowledgement does not match the complete pending batch",
                    ));
                }
                let count = u64::try_from(in_flight.batch.len()).map_err(|_| {
                    TelemetryError::new(
                        TelemetryErrorKind::SequenceOverflow,
                        "poll telemetry export",
                        "export batch length cannot be represented by telemetry counters",
                    )
                })?;
                let through_sequence = in_flight.batch.last_sequence();
                self.buffer.acknowledge(&in_flight.batch)?;
                self.in_flight = None;
                Ok(FlushOutcome::Exported { count, through_sequence, progress })
            }
        }
    }

    /// Advances one finite drain or exporter-cleanup poll and returns an actionable continuation.
    ///
    /// # Errors
    ///
    /// Returns the first explicit exporter, acknowledgement, storage, or shutdown failure.
    pub fn poll_shutdown<E: Exporter>(
        &mut self,
        exporter: &mut E,
        control: ExportPollControl,
    ) -> Result<ShutdownOutcome, TelemetryError> {
        if self.closed {
            return Ok(ShutdownOutcome::Complete);
        }
        if self.in_flight.is_some() || !self.buffer.is_empty() {
            match self.poll_flush(exporter, control)? {
                FlushOutcome::Empty => {}
                FlushOutcome::Pending(progress) => {
                    return Ok(ShutdownOutcome::Pending {
                        remaining: self.pending(),
                        next: DrainAction::PollExport,
                        progress,
                    });
                }
                FlushOutcome::Exported { progress, .. } => {
                    return Ok(ShutdownOutcome::Pending {
                        remaining: self.pending(),
                        next: if self.buffer.is_empty() {
                            DrainAction::PollExporterShutdown
                        } else {
                            DrainAction::PollExport
                        },
                        progress,
                    });
                }
                FlushOutcome::Cancelled { remaining, progress } => {
                    return Ok(ShutdownOutcome::Cancelled {
                        remaining,
                        next: DrainAction::PollExport,
                        progress,
                    });
                }
            }
        }

        if !self.exporter_shutdown_started {
            exporter
                .begin_shutdown()
                .map_err(|error| TelemetryError::exporter("begin telemetry shutdown", error))?;
            self.exporter_shutdown_started = true;
        }
        match exporter.poll_shutdown(control) {
            Ok(ExporterShutdownPoll::Pending(progress)) => Ok(ShutdownOutcome::Pending {
                remaining: self.pending(),
                next: DrainAction::PollExporterShutdown,
                progress,
            }),
            Ok(ExporterShutdownPoll::Complete(_)) => {
                self.exporter_shutdown_started = false;
                self.closed = true;
                Ok(ShutdownOutcome::Complete)
            }
            Ok(ExporterShutdownPoll::Cancelled(progress)) => {
                self.exporter_shutdown_started = false;
                Ok(ShutdownOutcome::Cancelled {
                    remaining: self.pending(),
                    next: DrainAction::PollExporterShutdown,
                    progress,
                })
            }
            Err(error) => {
                if error.cleanup_complete() {
                    self.exporter_shutdown_started = false;
                }
                Err(TelemetryError::exporter("poll telemetry shutdown", error))
            }
        }
    }

    pub(crate) const fn restore_disposition(
        &mut self,
        prefix: Sha256Digest,
        counters: BufferCounters,
    ) {
        self.buffer.restore_boundary(counters, prefix);
    }
}

const fn pump_invariant() -> TelemetryError {
    TelemetryError::new(
        TelemetryErrorKind::RecoveryMismatch,
        "advance telemetry export",
        "telemetry export ownership state is internally inconsistent",
    )
}
