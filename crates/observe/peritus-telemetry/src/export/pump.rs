//! Caller-polled export with durable exact-batch ownership and checkpoint-backed retirement.

use peritus_types::Sha256Digest;

use super::{
    ExportBatch, ExportPhase, ExportPoll, ExportPollControl, ExportProgress, ExportStreamId,
    Exporter, ExporterShutdownPoll,
};
use crate::{
    BufferCounters, EnqueueOutcome, ExportCheckpoint, ExportRecord, PendingBatchStore,
    TelemetryBuffer, TelemetryError, TelemetryErrorKind,
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
    /// Persist the pump checkpoint and retire its acknowledged durable batch ownership.
    PersistCheckpoint,
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
    /// One exact batch was acknowledged and now requires checkpoint-backed retirement.
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
        /// Operation the caller should perform next.
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

enum InFlightPhase {
    Ready,
    Exporting,
    AckObserved {
        progress: ExportProgress,
        durable: bool,
    },
    AwaitingCheckpoint {
        progress: ExportProgress,
    },
}

struct InFlight {
    batch: ExportBatch,
    phase: InFlightPhase,
}

/// Owns canonical export progress around one byte-accounted queue.
pub struct TelemetryPump {
    stream_id: ExportStreamId,
    buffer: TelemetryBuffer,
    ownership: PendingBatchStore,
    in_flight: Option<InFlight>,
    exporter_shutdown_started: bool,
    closed: bool,
}

impl TelemetryPump {
    /// Creates a pump at genesis with an empty caller-owned pending-batch store.
    ///
    /// # Errors
    ///
    /// Rejects mismatched stream ownership or durable state that requires restart recovery.
    pub fn new(
        stream_id: ExportStreamId,
        buffer: TelemetryBuffer,
        ownership: PendingBatchStore,
    ) -> Result<Self, TelemetryError> {
        ownership.require_empty()?;
        if !buffer.is_empty() {
            return Err(TelemetryError::new(
                TelemetryErrorKind::InvalidConfiguration,
                "create telemetry pump",
                "durable spill state requires checkpoint-aware recovery",
            ));
        }
        Self::construct(stream_id, buffer, ownership)
    }

    pub(crate) fn from_recovery(
        stream_id: ExportStreamId,
        buffer: TelemetryBuffer,
        ownership: PendingBatchStore,
    ) -> Result<Self, TelemetryError> {
        Self::construct(stream_id, buffer, ownership)
    }

    fn construct(
        stream_id: ExportStreamId,
        buffer: TelemetryBuffer,
        ownership: PendingBatchStore,
    ) -> Result<Self, TelemetryError> {
        if buffer.spill_stream_id().is_some_and(|owner| owner != stream_id)
            || ownership.stream_id() != stream_id
        {
            return Err(TelemetryError::new(
                TelemetryErrorKind::InvalidConfiguration,
                "create telemetry pump",
                "durable telemetry storage belongs to another export stream",
            ));
        }
        Ok(Self {
            stream_id,
            buffer,
            ownership,
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
    /// Returns explicit encoding, storage, sequence, accounting, or checkpoint-order failures.
    pub fn enqueue(&mut self, record: ExportRecord) -> Result<EnqueueOutcome, TelemetryError> {
        if self.closed {
            return Err(TelemetryError::new(
                TelemetryErrorKind::InvalidConfiguration,
                "enqueue telemetry",
                "telemetry exporter has completed shutdown",
            ));
        }
        if self.awaiting_checkpoint() {
            return Err(TelemetryError::new(
                TelemetryErrorKind::InvalidCheckpoint,
                "enqueue telemetry",
                "acknowledged export must be checkpointed before accepting another observation",
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

    /// Advances at most one exporter-owned operation poll while durably retaining its exact batch.
    ///
    /// # Errors
    ///
    /// Returns explicit exporter, acknowledgement, storage, checkpoint, or sequence failures.
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
        if self.awaiting_checkpoint() || self.ack_observed() {
            return self.finish_acknowledgement();
        }
        if self.in_flight.is_none() {
            let Some(batch) = self.buffer.batch(self.stream_id)? else {
                return Ok(FlushOutcome::Empty);
            };
            self.buffer.pin(&batch)?;
            if let Err(error) = self.ownership.persist(&batch) {
                self.buffer.unpin();
                return Err(error);
            }
            self.in_flight = Some(InFlight { batch, phase: InFlightPhase::Ready });
        }

        if matches!(control, ExportPollControl::Cancel) && self.ready_to_begin() {
            let batch = self.in_flight.as_ref().ok_or_else(pump_invariant)?.batch.clone();
            self.ownership.discard_unaccepted(&batch)?;
            self.in_flight = None;
            self.buffer.unpin();
            return Ok(FlushOutcome::Cancelled {
                remaining: self.pending(),
                progress: complete_progress(),
            });
        }

        if self.ready_to_begin() {
            let begin = exporter.begin_export(
                &self.in_flight.as_ref().ok_or_else(pump_invariant)?.batch,
            );
            if let Err(error) = begin {
                return Err(TelemetryError::exporter("begin telemetry export", error));
            }
            self.in_flight.as_mut().ok_or_else(pump_invariant)?.phase =
                InFlightPhase::Exporting;
        }

        let polled = match exporter.poll_export(control) {
            Ok(polled) => polled,
            Err(error) => {
                if error.cleanup_complete() {
                    self.in_flight.as_mut().ok_or_else(pump_invariant)?.phase =
                        InFlightPhase::Ready;
                }
                return Err(TelemetryError::exporter("poll telemetry export", error));
            }
        };
        match polled {
            ExportPoll::Pending(progress) => Ok(FlushOutcome::Pending(progress)),
            ExportPoll::Cancelled(progress) => {
                let batch = self.in_flight.as_ref().ok_or_else(pump_invariant)?.batch.clone();
                self.in_flight.as_mut().ok_or_else(pump_invariant)?.phase =
                    InFlightPhase::Ready;
                self.ownership.discard_unaccepted(&batch)?;
                self.in_flight = None;
                self.buffer.unpin();
                Ok(FlushOutcome::Cancelled { remaining: self.pending(), progress })
            }
            ExportPoll::Accepted { ack, progress } => {
                let in_flight = self.in_flight.as_mut().ok_or_else(pump_invariant)?;
                if !ack.matches(&in_flight.batch) {
                    in_flight.phase = InFlightPhase::Ready;
                    return Err(TelemetryError::new(
                        TelemetryErrorKind::AckMismatch,
                        "poll telemetry export",
                        "export acknowledgement does not match the durable pending batch",
                    ));
                }
                in_flight.phase = InFlightPhase::AckObserved { progress, durable: false };
                self.finish_acknowledgement()
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
                        next: DrainAction::PersistCheckpoint,
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

    pub(crate) fn restore_ownership(
        &mut self,
        durable: ExportBatch,
        acknowledged: bool,
    ) -> Result<(), TelemetryError> {
        if self.in_flight.is_some() {
            return Err(pump_invariant());
        }
        let batch = self.buffer.restore_exact_batch(self.stream_id, &durable)?;
        self.in_flight = Some(InFlight {
            batch,
            phase: if acknowledged {
                InFlightPhase::AckObserved { progress: complete_progress(), durable: true }
            } else {
                InFlightPhase::Ready
            },
        });
        Ok(())
    }

    pub(crate) fn retire_checkpointed(
        &mut self,
        checkpoint: ExportCheckpoint,
    ) -> Result<(), TelemetryError> {
        let Some(in_flight) = self.in_flight.as_ref() else { return Ok(()) };
        if !matches!(&in_flight.phase, InFlightPhase::AwaitingCheckpoint { .. }) {
            return Ok(());
        }
        if checkpoint != ExportCheckpoint::from_pump(self) {
            return Err(TelemetryError::new(
                TelemetryErrorKind::InvalidCheckpoint,
                "retire pending telemetry export",
                "persisted checkpoint does not match acknowledged pump disposition",
            ));
        }
        let batch = in_flight.batch.clone();
        self.ownership.retire_acknowledged(&batch)?;
        self.in_flight = None;
        Ok(())
    }

    pub(crate) const fn restore_disposition(
        &mut self,
        prefix: Sha256Digest,
        counters: BufferCounters,
    ) {
        self.buffer.restore_boundary(counters, prefix);
    }

    fn finish_acknowledgement(&mut self) -> Result<FlushOutcome, TelemetryError> {
        if self.awaiting_checkpoint() {
            return self.exported_outcome();
        }
        let (batch, progress, durable) = match self.in_flight.as_ref() {
            Some(InFlight {
                batch,
                phase: InFlightPhase::AckObserved { progress, durable },
            }) => (batch.clone(), *progress, *durable),
            _ => return Err(pump_invariant()),
        };
        if !durable {
            self.ownership.mark_acknowledged(&batch)?;
            self.in_flight.as_mut().ok_or_else(pump_invariant)?.phase =
                InFlightPhase::AckObserved { progress, durable: true };
        }
        self.buffer.acknowledge(&batch)?;
        self.in_flight.as_mut().ok_or_else(pump_invariant)?.phase =
            InFlightPhase::AwaitingCheckpoint { progress };
        self.exported_outcome()
    }

    fn exported_outcome(&self) -> Result<FlushOutcome, TelemetryError> {
        let Some(InFlight {
            batch,
            phase: InFlightPhase::AwaitingCheckpoint { progress },
        }) = self.in_flight.as_ref()
        else {
            return Err(pump_invariant());
        };
        let count = u64::try_from(batch.len()).map_err(|_| {
            TelemetryError::new(
                TelemetryErrorKind::SequenceOverflow,
                "complete telemetry export",
                "export batch length cannot be represented by telemetry counters",
            )
        })?;
        Ok(FlushOutcome::Exported {
            count,
            through_sequence: batch.last_sequence(),
            progress: *progress,
        })
    }

    fn ready_to_begin(&self) -> bool {
        self.in_flight
            .as_ref()
            .is_some_and(|in_flight| matches!(&in_flight.phase, InFlightPhase::Ready))
    }

    fn ack_observed(&self) -> bool {
        self.in_flight.as_ref().is_some_and(|in_flight| {
            matches!(&in_flight.phase, InFlightPhase::AckObserved { .. })
        })
    }

    fn awaiting_checkpoint(&self) -> bool {
        self.in_flight.as_ref().is_some_and(|in_flight| {
            matches!(&in_flight.phase, InFlightPhase::AwaitingCheckpoint { .. })
        })
    }
}

const fn complete_progress() -> ExportProgress {
    ExportProgress::new(ExportPhase::Complete, 0, None)
}

const fn pump_invariant() -> TelemetryError {
    TelemetryError::new(
        TelemetryErrorKind::RecoveryMismatch,
        "advance telemetry export",
        "telemetry export ownership state is internally inconsistent",
    )
}
