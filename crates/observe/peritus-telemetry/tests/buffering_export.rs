//! Focused bounded-buffer, exporter, acknowledgement, and shutdown tests.

mod support;

use std::num::NonZeroUsize;

use peritus_telemetry::{
    BackpressurePolicy, BufferConfig, EnqueueOutcome, ExportAck, ExportBatch, ExportStreamId,
    Exporter, ExporterError, ExporterErrorCode, FlushOutcome, ShutdownOutcome, TelemetryBuffer,
    TelemetryErrorKind, TelemetryPump,
};

use support::metric_record;

#[test]
fn both_backpressure_policies_are_bounded_and_account_exact_drops() {
    let mut reject = TelemetryBuffer::new(config(2, 2, BackpressurePolicy::RejectNewest));
    reject.enqueue(metric_record(1)).expect("first");
    reject.enqueue(metric_record(2)).expect("second");
    assert_eq!(
        reject.enqueue(metric_record(3)).expect("bounded rejection"),
        EnqueueOutcome::RejectedNewest { rejected_sequence: 3 },
    );
    assert_eq!(reject.len(), 2);
    assert_eq!(reject.counters().submitted(), 3);
    assert_eq!(reject.counters().accepted(), 2);
    assert_eq!(reject.counters().dropped(), 1);

    let mut oldest = TelemetryBuffer::new(config(2, 2, BackpressurePolicy::DropOldest));
    oldest.enqueue(metric_record(1)).expect("first");
    oldest.enqueue(metric_record(2)).expect("second");
    assert_eq!(
        oldest.enqueue(metric_record(3)).expect("bounded eviction"),
        EnqueueOutcome::DroppedOldest { accepted_sequence: 3, dropped_sequence: 1 },
    );
    assert_eq!(oldest.len(), 2);
    assert_eq!(oldest.counters().accepted(), 3);
    assert_eq!(oldest.counters().dropped(), 1);
}

#[test]
fn exporter_failure_is_explicit_and_retains_the_exact_batch_for_retry() {
    let stream = ExportStreamId::new([31; 16]).expect("stream");
    let mut pump = TelemetryPump::new(
        stream,
        TelemetryBuffer::new(config(4, 2, BackpressurePolicy::RejectNewest)),
    );
    pump.buffer_mut().enqueue(metric_record(1)).expect("enqueue");
    pump.buffer_mut().enqueue(metric_record(2)).expect("enqueue");
    let mut exporter = RecordingExporter::failing_once();
    let error = pump.flush_one(&mut exporter).expect_err("first export fails");
    assert_eq!(error.kind(), TelemetryErrorKind::ExportFailed);
    assert_eq!(error.exporter_code(), Some(ExporterErrorCode::Unavailable));
    assert_eq!(error.exporter_retryable(), Some(true));
    assert_eq!(pump.buffer().len(), 2);

    assert_eq!(
        pump.flush_one(&mut exporter).expect("retry exact batch"),
        FlushOutcome::Exported { count: 2, through_sequence: 2 },
    );
    assert_eq!(exporter.batch_ids.len(), 2);
    assert_eq!(exporter.batch_ids[0], exporter.batch_ids[1]);
    assert!(pump.buffer().is_empty());
}

#[test]
fn contradictory_acknowledgement_keeps_the_batch_pending() {
    let stream = ExportStreamId::new([32; 16]).expect("stream");
    let mut pump = TelemetryPump::new(
        stream,
        TelemetryBuffer::new(config(2, 2, BackpressurePolicy::RejectNewest)),
    );
    pump.buffer_mut().enqueue(metric_record(1)).expect("enqueue");
    let mut exporter = BadAckExporter;
    let error = pump.flush_one(&mut exporter).expect_err("bad acknowledgement");
    assert_eq!(error.kind(), TelemetryErrorKind::AckMismatch);
    assert_eq!(pump.buffer().len(), 1);
    assert_eq!(pump.disposed_through_sequence(), 0);
}

#[test]
fn shutdown_is_bounded_and_only_releases_an_empty_exporter() {
    let stream = ExportStreamId::new([33; 16]).expect("stream");
    let mut pump = TelemetryPump::new(
        stream,
        TelemetryBuffer::new(config(4, 1, BackpressurePolicy::RejectNewest)),
    );
    pump.buffer_mut().enqueue(metric_record(1)).expect("enqueue");
    pump.buffer_mut().enqueue(metric_record(2)).expect("enqueue");
    let mut exporter = RecordingExporter::success();
    assert_eq!(
        pump.shutdown(&mut exporter, 1).expect("bounded shutdown"),
        ShutdownOutcome::Pending { remaining: 1 },
    );
    assert_eq!(exporter.shutdowns, 0);
    assert_eq!(pump.shutdown(&mut exporter, 1).expect("finish"), ShutdownOutcome::Complete);
    assert_eq!(exporter.shutdowns, 1);
}

fn config(capacity: usize, batch: usize, policy: BackpressurePolicy) -> BufferConfig {
    BufferConfig::new(
        NonZeroUsize::new(capacity).expect("capacity"),
        NonZeroUsize::new(batch).expect("batch"),
        policy,
    )
    .expect("buffer config")
}

struct RecordingExporter {
    failures_remaining: u64,
    batch_ids: Vec<peritus_types::Sha256Digest>,
    shutdowns: u64,
}

impl RecordingExporter {
    const fn failing_once() -> Self {
        Self { failures_remaining: 1, batch_ids: Vec::new(), shutdowns: 0 }
    }

    const fn success() -> Self {
        Self { failures_remaining: 0, batch_ids: Vec::new(), shutdowns: 0 }
    }
}

impl Exporter for RecordingExporter {
    fn export(&mut self, batch: &ExportBatch) -> Result<ExportAck, ExporterError> {
        self.batch_ids.push(batch.batch_id());
        if self.failures_remaining > 0 {
            self.failures_remaining -= 1;
            Err(ExporterError::new(ExporterErrorCode::Unavailable, true))
        } else {
            Ok(ExportAck::accept(batch))
        }
    }

    fn shutdown(&mut self) -> Result<(), ExporterError> {
        self.shutdowns += 1;
        Ok(())
    }
}

struct BadAckExporter;

impl Exporter for BadAckExporter {
    fn export(&mut self, batch: &ExportBatch) -> Result<ExportAck, ExporterError> {
        Ok(ExportAck::new(
            batch.stream_id(),
            batch.batch_id(),
            batch.first_sequence(),
            batch.last_sequence(),
            batch.len() + 1,
        ))
    }

    fn shutdown(&mut self) -> Result<(), ExporterError> {
        Ok(())
    }
}
