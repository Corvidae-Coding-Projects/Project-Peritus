//! Focused byte-accounting, explicit-loss, exporter ownership, and drain tests.

mod support;

use std::num::NonZeroUsize;

use peritus_telemetry::{
    BufferConfig, DrainAction, EnqueueOutcome, ExportAck, ExportBatch, ExportPhase, ExportPoll,
    ExportPollControl, ExportProgress, ExportStreamId, Exporter, ExporterError, ExporterErrorCode,
    ExporterShutdownPoll, FlushOutcome, ObservationLossPolicy, RejectionReason, ShutdownOutcome,
    TelemetryBuffer, TelemetryErrorKind, TelemetryPump,
};

use support::metric_record;

#[test]
fn lossy_policies_account_physical_pages_and_exact_loss() {
    let mut reject = TelemetryBuffer::new(config(2, ObservationLossPolicy::RejectNewest))
        .expect("reject buffer");
    assert!(matches!(
        reject.enqueue(metric_record(1)).expect("first"),
        EnqueueOutcome::Accepted { resident_bytes: 512, .. },
    ));
    reject.enqueue(metric_record(2)).expect("second");
    assert_eq!(
        reject.enqueue(metric_record(3)).expect("explicit rejection"),
        EnqueueOutcome::RejectedNewest {
            rejected_sequence: 3,
            reason: RejectionReason::MemoryFull,
        },
    );
    assert_eq!(reject.len(), 2);
    assert_eq!(reject.resident_bytes(), 1_024);
    assert_eq!(reject.counters().submitted(), 3);
    assert_eq!(reject.counters().accepted(), 2);
    assert_eq!(reject.counters().dropped(), 1);

    let mut oldest = TelemetryBuffer::new(config(2, ObservationLossPolicy::DropOldest))
        .expect("drop buffer");
    oldest.enqueue(metric_record(1)).expect("first");
    oldest.enqueue(metric_record(2)).expect("second");
    assert_eq!(
        oldest.enqueue(metric_record(3)).expect("explicit eviction"),
        EnqueueOutcome::DroppedOldest {
            accepted_sequence: 3,
            first_dropped_sequence: 1,
            last_dropped_sequence: 1,
            count: 1,
        },
    );
    assert_eq!(oldest.len(), 2);
    assert_eq!(oldest.counters().accepted(), 3);
    assert_eq!(oldest.counters().dropped(), 1);
}

#[test]
fn exporter_failure_is_explicit_and_retains_the_exact_batch_for_retry() {
    let stream = ExportStreamId::new([31; 16]).expect("stream");
    let buffer = TelemetryBuffer::new(config(4, ObservationLossPolicy::RejectNewest))
        .expect("buffer");
    let mut pump = TelemetryPump::new(stream, buffer).expect("pump");
    pump.enqueue(metric_record(1)).expect("enqueue");
    pump.enqueue(metric_record(2)).expect("enqueue");
    let mut exporter = RecordingExporter::failing_once();
    let error = pump
        .poll_flush(&mut exporter, continue_poll())
        .expect_err("first export fails");
    assert_eq!(error.kind(), TelemetryErrorKind::ExportFailed);
    assert_eq!(error.exporter_code(), Some(ExporterErrorCode::Unavailable));
    assert_eq!(error.exporter_retryable(), Some(true));
    assert_eq!(error.exporter_cleanup_complete(), Some(true));
    assert_eq!(pump.buffer().len(), 2);

    assert!(matches!(
        pump.poll_flush(&mut exporter, continue_poll()).expect("retry exact batch"),
        FlushOutcome::Exported { count: 2, through_sequence: 2, .. },
    ));
    assert_eq!(exporter.batch_ids.len(), 2);
    assert_eq!(exporter.batch_ids[0], exporter.batch_ids[1]);
    assert!(pump.buffer().is_empty());
}

#[test]
fn contradictory_acknowledgement_keeps_the_batch_pinned_for_exact_retry() {
    let stream = ExportStreamId::new([32; 16]).expect("stream");
    let buffer = TelemetryBuffer::new(config(2, ObservationLossPolicy::DropOldest))
        .expect("buffer");
    let mut pump = TelemetryPump::new(stream, buffer).expect("pump");
    pump.enqueue(metric_record(1)).expect("enqueue");
    let mut exporter = BadAckExporter { pending: None };
    let error = pump
        .poll_flush(&mut exporter, continue_poll())
        .expect_err("bad acknowledgement");
    assert_eq!(error.kind(), TelemetryErrorKind::AckMismatch);
    assert_eq!(pump.buffer().len(), 1);
    assert_eq!(pump.disposed_through_sequence(), 0);
    assert!(matches!(
        pump.enqueue(metric_record(2)).expect("room remains without eviction"),
        EnqueueOutcome::Accepted { .. },
    ));
    assert_eq!(
        pump.enqueue(metric_record(3)).expect("pinned prefix prevents eviction"),
        EnqueueOutcome::RejectedNewest {
            rejected_sequence: 3,
            reason: RejectionReason::InFlightPrefix,
        },
    );
}

#[test]
fn shutdown_returns_an_actionable_continuation_until_cleanup_completes() {
    let stream = ExportStreamId::new([33; 16]).expect("stream");
    let config = BufferConfig::new(
        NonZeroUsize::new(512).expect("page bytes"),
        NonZeroUsize::new(4).expect("pages"),
        NonZeroUsize::new(1).expect("one canonical byte target still admits one record"),
        ObservationLossPolicy::RejectNewest,
    )
    .expect("config");
    let buffer = TelemetryBuffer::new(config).expect("buffer");
    let mut pump = TelemetryPump::new(stream, buffer).expect("pump");
    pump.enqueue(metric_record(1)).expect("enqueue");
    pump.enqueue(metric_record(2)).expect("enqueue");
    let mut exporter = RecordingExporter::success();
    assert!(matches!(
        pump.poll_shutdown(&mut exporter, continue_poll()).expect("first batch"),
        ShutdownOutcome::Pending {
            remaining,
            next: DrainAction::PollExport,
            ..
        } if remaining.total() == 1
    ));
    assert!(matches!(
        pump.poll_shutdown(&mut exporter, continue_poll()).expect("second batch"),
        ShutdownOutcome::Pending {
            remaining,
            next: DrainAction::PollExporterShutdown,
            ..
        } if remaining.total() == 0
    ));
    assert_eq!(
        pump.poll_shutdown(&mut exporter, continue_poll()).expect("cleanup"),
        ShutdownOutcome::Complete,
    );
    assert_eq!(exporter.shutdowns, 1);
}

fn config(pages: usize, policy: ObservationLossPolicy) -> BufferConfig {
    BufferConfig::new(
        NonZeroUsize::new(512).expect("page bytes"),
        NonZeroUsize::new(pages).expect("pages"),
        NonZeroUsize::new(4_096).expect("batch bytes"),
        policy,
    )
    .expect("buffer config")
}

fn continue_poll() -> ExportPollControl {
    ExportPollControl::continue_with(NonZeroUsize::new(4_096).expect("poll bytes"))
}

struct RecordingExporter {
    failures_remaining: u64,
    batch_ids: Vec<peritus_types::Sha256Digest>,
    pending: Option<ExportAck>,
    shutdown_pending: bool,
    shutdowns: u64,
}

impl RecordingExporter {
    const fn failing_once() -> Self {
        Self {
            failures_remaining: 1,
            batch_ids: Vec::new(),
            pending: None,
            shutdown_pending: false,
            shutdowns: 0,
        }
    }

    const fn success() -> Self {
        Self {
            failures_remaining: 0,
            batch_ids: Vec::new(),
            pending: None,
            shutdown_pending: false,
            shutdowns: 0,
        }
    }
}

impl Exporter for RecordingExporter {
    fn begin_export(&mut self, batch: &ExportBatch) -> Result<ExportProgress, ExporterError> {
        self.batch_ids.push(batch.batch_id());
        if self.failures_remaining > 0 {
            self.failures_remaining -= 1;
            return Err(ExporterError::new(ExporterErrorCode::Unavailable, true));
        }
        self.pending = Some(ExportAck::accept(batch));
        Ok(progress(ExportPhase::Prepared))
    }

    fn poll_export(&mut self, control: ExportPollControl) -> Result<ExportPoll, ExporterError> {
        let ack = self.pending.take().ok_or_else(|| {
            ExporterError::new(ExporterErrorCode::Protocol, false)
        })?;
        Ok(match control {
            ExportPollControl::Continue { .. } => {
                ExportPoll::Accepted { ack, progress: progress(ExportPhase::Complete) }
            }
            ExportPollControl::Cancel => ExportPoll::Cancelled(progress(ExportPhase::Complete)),
        })
    }

    fn begin_shutdown(&mut self) -> Result<ExportProgress, ExporterError> {
        self.shutdown_pending = true;
        Ok(progress(ExportPhase::Cleaning))
    }

    fn poll_shutdown(
        &mut self,
        control: ExportPollControl,
    ) -> Result<ExporterShutdownPoll, ExporterError> {
        if !self.shutdown_pending {
            return Err(ExporterError::new(ExporterErrorCode::Protocol, false));
        }
        self.shutdown_pending = false;
        Ok(match control {
            ExportPollControl::Continue { .. } => {
                self.shutdowns += 1;
                ExporterShutdownPoll::Complete(progress(ExportPhase::Complete))
            }
            ExportPollControl::Cancel => {
                ExporterShutdownPoll::Cancelled(progress(ExportPhase::Complete))
            }
        })
    }
}

struct BadAckExporter {
    pending: Option<ExportAck>,
}

impl Exporter for BadAckExporter {
    fn begin_export(&mut self, batch: &ExportBatch) -> Result<ExportProgress, ExporterError> {
        self.pending = Some(ExportAck::new(
            batch.stream_id(),
            batch.batch_id(),
            batch.first_sequence(),
            batch.last_sequence(),
            batch.len() + 1,
        ));
        Ok(progress(ExportPhase::Prepared))
    }

    fn poll_export(&mut self, _control: ExportPollControl) -> Result<ExportPoll, ExporterError> {
        Ok(ExportPoll::Accepted {
            ack: self.pending.take().expect("pending bad acknowledgement"),
            progress: progress(ExportPhase::Complete),
        })
    }

    fn begin_shutdown(&mut self) -> Result<ExportProgress, ExporterError> {
        Ok(progress(ExportPhase::Cleaning))
    }

    fn poll_shutdown(
        &mut self,
        _control: ExportPollControl,
    ) -> Result<ExporterShutdownPoll, ExporterError> {
        Ok(ExporterShutdownPoll::Complete(progress(ExportPhase::Complete)))
    }
}

const fn progress(phase: ExportPhase) -> ExportProgress {
    ExportProgress::new(phase, 0, None)
}
