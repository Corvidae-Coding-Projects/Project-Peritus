//! Focused checkpoint, caller-owned spill, and restart-recovery adversarial tests.

mod support;

use std::{
    fs::{self, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    num::NonZeroUsize,
};

use peritus_telemetry::{
    BufferConfig, CheckpointStore, ExportAck, ExportBatch, ExportCheckpoint, ExportPhase,
    ExportPoll, ExportPollControl, ExportProgress, ExportStreamId, Exporter, ExporterError,
    ExporterErrorCode, ExporterShutdownPoll, ObservationLossPolicy, SpillStore, TelemetryBuffer,
    PendingBatchStore, TelemetryErrorKind, TelemetryPump, recover_buffer,
};
use peritus_trace::DiagnosticCode;
use tempfile::TempDir;

use support::projection;

#[test]
fn checkpoint_restart_replays_only_the_undisposed_projection_suffix() {
    let temporary = TempDir::new().expect("temporary directory");
    let stream = ExportStreamId::new([41; 16]).expect("stream");
    let projection = projection(DiagnosticCode::RecoveryCompleted);
    assert_eq!(projection.records().len(), 3);
    let config = config(8, 1, ObservationLossPolicy::RejectNewest);
    let buffer = TelemetryBuffer::new(config).expect("buffer");
    let ownership = pending_store(temporary.path(), stream);
    let mut pump = TelemetryPump::new(stream, buffer, ownership).expect("pump");
    for record in projection.records() {
        pump.enqueue(record.clone()).expect("enqueue projection");
    }
    flush_one(&mut pump);
    let checkpoint = ExportCheckpoint::from_pump(&pump);
    assert_eq!(checkpoint.disposed_through_sequence(), 1);

    let store = CheckpointStore::open(
        temporary.path().join("checkpoints"),
        stream,
        NonZeroUsize::new(2).expect("retention"),
    )
    .expect("checkpoint store");
    store.persist_and_retire(&mut pump).expect("persist checkpoint and retire batch");
    drop(pump);
    drop(store);

    let reopened = CheckpointStore::open(
        temporary.path().join("checkpoints"),
        stream,
        NonZeroUsize::new(2).expect("retention"),
    )
    .expect("reopen checkpoint store");
    let loaded = reopened.load_latest().expect("load checkpoint");
    let recovered_buffer = TelemetryBuffer::new(config).expect("recovered buffer");
    let recovered_ownership = pending_store(temporary.path(), stream);
    let report = recover_buffer(recovered_buffer, recovered_ownership, stream, loaded, &projection)
        .expect("recover queue");
    assert_eq!(report.replayed(), 2);
    assert_eq!(report.dropped_during_recovery(), 0);
    let recovered = report.into_pump();
    assert_eq!(recovered.disposed_through_sequence(), 1);
    assert_eq!(recovered.buffer().len(), 2);
    assert_eq!(recovered.buffer().counters().submitted(), 3);
}

#[test]
fn caller_owned_spill_recovers_all_canonical_records_without_loss() {
    let temporary = TempDir::new().expect("temporary directory");
    let stream = ExportStreamId::new([47; 16]).expect("stream");
    let projection = projection(DiagnosticCode::RecoveryCompleted);
    let config = config(1, 512, ObservationLossPolicy::LosslessSpill);
    let spill = SpillStore::open(temporary.path().join("spill"), stream).expect("spill");
    let buffer = TelemetryBuffer::with_spill(config, spill).expect("buffer");
    let ownership = pending_store(temporary.path(), stream);
    let mut pump = TelemetryPump::new(stream, buffer, ownership).expect("pump");
    for record in projection.records() {
        pump.enqueue(record.clone()).expect("enqueue projection");
    }
    assert_eq!(pump.buffer().resident_len(), 1);
    assert_eq!(pump.buffer().spilled_len(), 2);
    drop(pump);

    let spill = SpillStore::open(temporary.path().join("spill"), stream).expect("reopen spill");
    let buffer = TelemetryBuffer::with_spill(config, spill).expect("recovered buffer");
    let ownership = pending_store(temporary.path(), stream);
    let report = recover_buffer(buffer, ownership, stream, None, &projection)
        .expect("recover spill");
    assert_eq!(report.replayed(), 3);
    assert_eq!(report.dropped_during_recovery(), 0);
    assert_eq!(report.spilled_during_recovery(), 3);
    let recovered = report.into_pump();
    assert_eq!(recovered.pending().total(), 3);
    assert_eq!(recovered.buffer().resident_len(), 0);
    assert_eq!(recovered.buffer().spilled_len(), 3);
}

#[test]
fn acknowledged_batch_restart_replays_interleaved_overflow_without_duplicate_export() {
    let temporary = TempDir::new().expect("temporary directory");
    let stream = ExportStreamId::new([48; 16]).expect("stream");
    let projection = projection(DiagnosticCode::RecoveryCompleted);
    let config = config(2, 1, ObservationLossPolicy::DropOldest);
    let buffer = TelemetryBuffer::new(config).expect("buffer");
    let ownership = pending_store(temporary.path(), stream);
    let mut pump = TelemetryPump::new(stream, buffer, ownership).expect("pump");
    pump.enqueue(projection.records()[0].clone()).expect("enqueue first");
    pump.enqueue(projection.records()[1].clone()).expect("enqueue second");

    let mut exporter = PendingOnceExporter { pending: None, poll_count: 0 };
    assert!(matches!(
        pump.poll_flush(
            &mut exporter,
            ExportPollControl::continue_with(NonZeroUsize::new(4_096).expect("poll bytes")),
        )
        .expect("start export"),
        peritus_telemetry::FlushOutcome::Pending(_),
    ));
    assert!(matches!(
        pump.enqueue(projection.records()[2].clone()).expect("interleaved enqueue"),
        peritus_telemetry::EnqueueOutcome::RejectedNewest {
            reason: peritus_telemetry::RejectionReason::InFlightPrefix,
            ..
        },
    ));
    assert!(matches!(
        pump.poll_flush(
            &mut exporter,
            ExportPollControl::continue_with(NonZeroUsize::new(4_096).expect("poll bytes")),
        )
        .expect("accept export"),
        peritus_telemetry::FlushOutcome::Exported {
            count: 1,
            through_sequence: 1,
            ..
        },
    ));
    drop(pump);

    let buffer = TelemetryBuffer::new(config).expect("recovered buffer");
    let ownership = pending_store(temporary.path(), stream);
    let report = recover_buffer(buffer, ownership, stream, None, &projection)
        .expect("recover acknowledged batch");
    assert_eq!(report.replayed(), 3);
    assert_eq!(report.dropped_during_recovery(), 1);
    let mut recovered = report.into_pump();
    let mut exporter = NoCallExporter;
    assert!(matches!(
        recovered
            .poll_flush(
                &mut exporter,
                ExportPollControl::continue_with(
                    NonZeroUsize::new(4_096).expect("poll bytes"),
                ),
            )
            .expect("finish durable acknowledgement"),
        peritus_telemetry::FlushOutcome::Exported {
            count: 1,
            through_sequence: 1,
            ..
        },
    ));
    checkpoint_store(temporary.path(), stream)
        .persist_and_retire(&mut recovered)
        .expect("checkpoint acknowledgement and retire ownership");
    assert_eq!(recovered.pending().total(), 1);
}

#[test]
fn drop_oldest_checkpoint_preserves_eviction_before_later_export_across_restart() {
    let temporary = TempDir::new().expect("temporary directory");
    let stream = ExportStreamId::new([44; 16]).expect("stream");
    let projection = projection(DiagnosticCode::RecoveryCompleted);
    let config = config(2, 1, ObservationLossPolicy::DropOldest);
    let buffer = TelemetryBuffer::new(config).expect("buffer");
    let ownership = pending_store(temporary.path(), stream);
    let mut pump = TelemetryPump::new(stream, buffer, ownership).expect("pump");
    for record in projection.records() {
        pump.enqueue(record.clone()).expect("enqueue projection");
    }
    assert_eq!(pump.buffer().counters().dropped(), 1);
    flush_one(&mut pump);

    let checkpoint = ExportCheckpoint::from_pump(&pump);
    assert_eq!(checkpoint.disposed_through_sequence(), 2);
    assert_eq!(checkpoint.counters().submitted(), 2);
    assert_eq!(checkpoint.counters().accepted(), 2);
    assert_eq!(checkpoint.counters().dropped(), 1);
    assert_eq!(checkpoint.counters().exported(), 1);
    let checkpoints = checkpoint_store(temporary.path(), stream);
    checkpoints
        .persist_and_retire(&mut pump)
        .expect("persist checkpoint and retire batch");
    drop(pump);

    let recovered_buffer = TelemetryBuffer::new(config).expect("recovered buffer");
    let ownership = pending_store(temporary.path(), stream);
    let report = recover_buffer(
        recovered_buffer,
        ownership,
        stream,
        Some(checkpoint),
        &projection,
    )
    .expect("recover");
    assert_eq!(report.replayed(), 1);
    assert_eq!(report.dropped_during_recovery(), 0);
    let recovered = report.into_pump();
    assert_eq!(recovered.disposed_through_sequence(), 2);
    assert_eq!(recovered.buffer().len(), 1);
    assert_eq!(recovered.buffer().counters().submitted(), 3);
    assert_eq!(recovered.buffer().counters().accepted(), 3);
    assert_eq!(recovered.buffer().counters().dropped(), 1);
    assert_eq!(recovered.buffer().counters().exported(), 1);
}

#[test]
fn reject_newest_checkpoint_preserves_loss_markers_after_earlier_exports() {
    let temporary = TempDir::new().expect("temporary directory");
    let stream = ExportStreamId::new([45; 16]).expect("stream");
    let projection = projection(DiagnosticCode::RecoveryCompleted);
    let config = config(2, 1, ObservationLossPolicy::RejectNewest);
    let buffer = TelemetryBuffer::new(config).expect("buffer");
    let ownership = pending_store(temporary.path(), stream);
    let checkpoints = checkpoint_store(temporary.path(), stream);
    let mut pump = TelemetryPump::new(stream, buffer, ownership).expect("pump");
    for record in projection.records() {
        pump.enqueue(record.clone()).expect("enqueue projection");
    }
    flush_one(&mut pump);
    checkpoints
        .persist_and_retire(&mut pump)
        .expect("persist first checkpoint and retire batch");
    flush_one(&mut pump);

    let checkpoint = ExportCheckpoint::from_pump(&pump);
    assert_eq!(checkpoint.disposed_through_sequence(), 3);
    assert_eq!(checkpoint.counters().submitted(), 3);
    assert_eq!(checkpoint.counters().accepted(), 2);
    assert_eq!(checkpoint.counters().dropped(), 1);
    assert_eq!(checkpoint.counters().exported(), 2);
    checkpoints
        .persist_and_retire(&mut pump)
        .expect("persist second checkpoint and retire batch");
    drop(pump);

    let recovered_buffer = TelemetryBuffer::new(config).expect("recovered buffer");
    let ownership = pending_store(temporary.path(), stream);
    let report = recover_buffer(
        recovered_buffer,
        ownership,
        stream,
        Some(checkpoint),
        &projection,
    )
    .expect("recover");
    assert_eq!(report.replayed(), 0);
    assert_eq!(report.dropped_during_recovery(), 0);
    let recovered = report.into_pump();
    assert_eq!(recovered.disposed_through_sequence(), 3);
    assert!(recovered.buffer().is_empty());
    assert_eq!(recovered.buffer().counters(), checkpoint.counters());
}

#[test]
fn recovery_rejects_a_checkpoint_from_changed_projection_history() {
    let temporary = TempDir::new().expect("temporary directory");
    let stream = ExportStreamId::new([42; 16]).expect("stream");
    let original = projection(DiagnosticCode::RecoveryCompleted);
    let changed = projection(DiagnosticCode::RecoveryFailed);
    let config = config(8, 1, ObservationLossPolicy::RejectNewest);
    let buffer = TelemetryBuffer::new(config).expect("buffer");
    let ownership = pending_store(temporary.path(), stream);
    let checkpoints = checkpoint_store(temporary.path(), stream);
    let mut pump = TelemetryPump::new(stream, buffer, ownership).expect("pump");
    pump.enqueue(original.records()[0].clone()).expect("enqueue");
    flush_one(&mut pump);
    let checkpoint = ExportCheckpoint::from_pump(&pump);
    checkpoints
        .persist_and_retire(&mut pump)
        .expect("persist checkpoint and retire batch");
    drop(pump);

    let recovered_buffer = TelemetryBuffer::new(config).expect("recovered buffer");
    let ownership = pending_store(temporary.path(), stream);
    let error = recover_buffer(recovered_buffer, ownership, stream, Some(checkpoint), &changed)
        .err()
        .expect("changed projection prefix");
    assert_eq!(error.kind(), TelemetryErrorKind::RecoveryMismatch);
}

#[test]
fn corrupted_latest_generation_fails_closed_and_abandoned_temp_is_removed() {
    let temporary = TempDir::new().expect("temporary directory");
    let stream = ExportStreamId::new([43; 16]).expect("stream");
    let store =
        CheckpointStore::open(temporary.path(), stream, NonZeroUsize::new(1).expect("retention"))
            .expect("checkpoint store");
    let buffer = TelemetryBuffer::new(config(2, 512, ObservationLossPolicy::RejectNewest))
        .expect("buffer");
    let ownership = pending_store(temporary.path(), stream);
    let pump = TelemetryPump::new(stream, buffer, ownership).expect("pump");
    store.persist(ExportCheckpoint::from_pump(&pump)).expect("persist genesis");
    let checkpoint_path = fs::read_dir(temporary.path())
        .expect("directory")
        .map(|entry| entry.expect("entry").path())
        .find(|path| path.extension().is_some_and(|extension| extension == "checkpoint"))
        .expect("checkpoint path");
    let mut file =
        OpenOptions::new().read(true).write(true).open(&checkpoint_path).expect("open checkpoint");
    let mut first = [0_u8; 1];
    file.read_exact(&mut first).expect("read byte");
    file.seek(SeekFrom::Start(0)).expect("seek");
    file.write_all(&[first[0] ^ 0xff]).expect("corrupt byte");
    file.sync_all().expect("sync corruption");
    assert_eq!(
        store.load_latest().expect_err("corrupt checkpoint").kind(),
        TelemetryErrorKind::InvalidCheckpoint,
    );
    drop(store);

    let abandoned = temporary
        .path()
        .join(format!(".{}-00000000000000000000-999-1.temporary", hex(stream.as_bytes())));
    fs::write(&abandoned, b"partial").expect("abandoned temporary");
    CheckpointStore::open(temporary.path(), stream, NonZeroUsize::new(1).expect("retention"))
        .expect("startup cleans temporary");
    assert!(!abandoned.exists());
}

#[test]
fn version_one_checkpoint_marker_is_explicitly_unsupported() {
    let temporary = TempDir::new().expect("temporary directory");
    let stream = ExportStreamId::new([46; 16]).expect("stream");
    let store =
        CheckpointStore::open(temporary.path(), stream, NonZeroUsize::new(1).expect("retention"))
            .expect("checkpoint store");
    let path = temporary
        .path()
        .join(format!("{}-00000000000000000000.checkpoint", hex(stream.as_bytes())));
    let mut version_one = vec![0_u8; 152];
    let marker = b"PERITUS-C7-EXPORT-CHECKPOINT-V1\0";
    version_one[..marker.len()].copy_from_slice(marker);
    fs::write(path, version_one).expect("write legacy checkpoint");

    assert_eq!(
        store.load_latest().expect_err("V1 checkpoint is unsupported").kind(),
        TelemetryErrorKind::InvalidCheckpoint,
    );
}

fn config(
    pages: usize,
    batch_bytes: usize,
    policy: ObservationLossPolicy,
) -> BufferConfig {
    BufferConfig::new(
        NonZeroUsize::new(512).expect("page bytes"),
        NonZeroUsize::new(pages).expect("pages"),
        NonZeroUsize::new(batch_bytes).expect("batch bytes"),
        policy,
    )
    .expect("buffer config")
}

fn pending_store(root: &std::path::Path, stream: ExportStreamId) -> PendingBatchStore {
    PendingBatchStore::open(root.join("pending"), stream).expect("pending batch store")
}

fn checkpoint_store(root: &std::path::Path, stream: ExportStreamId) -> CheckpointStore {
    CheckpointStore::open(
        root.join("checkpoints"),
        stream,
        NonZeroUsize::new(2).expect("checkpoint retention"),
    )
    .expect("checkpoint store")
}

fn flush_one(pump: &mut TelemetryPump) {
    let mut exporter = AcceptingExporter { pending: None };
    let outcome = pump
        .poll_flush(
            &mut exporter,
            ExportPollControl::continue_with(NonZeroUsize::new(4_096).expect("poll bytes")),
        )
        .expect("flush one batch");
    assert!(matches!(outcome, peritus_telemetry::FlushOutcome::Exported { .. }));
}

struct AcceptingExporter {
    pending: Option<ExportAck>,
}

struct PendingOnceExporter {
    pending: Option<ExportAck>,
    poll_count: u64,
}

impl Exporter for PendingOnceExporter {
    fn begin_export(&mut self, batch: &ExportBatch) -> Result<ExportProgress, ExporterError> {
        self.pending = Some(ExportAck::accept(batch));
        Ok(ExportProgress::new(ExportPhase::Prepared, 0, None))
    }

    fn poll_export(&mut self, _control: ExportPollControl) -> Result<ExportPoll, ExporterError> {
        self.poll_count = self.poll_count.checked_add(1).expect("poll count");
        if self.poll_count == 1 {
            return Ok(ExportPoll::Pending(ExportProgress::new(
                ExportPhase::Writing,
                0,
                None,
            )));
        }
        Ok(ExportPoll::Accepted {
            ack: self.pending.take().expect("pending acknowledgement"),
            progress: ExportProgress::new(ExportPhase::Complete, 0, None),
        })
    }

    fn begin_shutdown(&mut self) -> Result<ExportProgress, ExporterError> {
        panic!("restart regression must not shut down the exporter")
    }

    fn poll_shutdown(
        &mut self,
        _control: ExportPollControl,
    ) -> Result<ExporterShutdownPoll, ExporterError> {
        panic!("restart regression must not poll exporter shutdown")
    }
}

struct NoCallExporter;

impl Exporter for NoCallExporter {
    fn begin_export(&mut self, _batch: &ExportBatch) -> Result<ExportProgress, ExporterError> {
        panic!("durably acknowledged batch must not be exported again")
    }

    fn poll_export(&mut self, _control: ExportPollControl) -> Result<ExportPoll, ExporterError> {
        panic!("durably acknowledged batch must not poll an exporter operation")
    }

    fn begin_shutdown(&mut self) -> Result<ExportProgress, ExporterError> {
        panic!("restart regression must not shut down the exporter")
    }

    fn poll_shutdown(
        &mut self,
        _control: ExportPollControl,
    ) -> Result<ExporterShutdownPoll, ExporterError> {
        panic!("restart regression must not poll exporter shutdown")
    }
}

impl Exporter for AcceptingExporter {
    fn begin_export(&mut self, batch: &ExportBatch) -> Result<ExportProgress, ExporterError> {
        self.pending = Some(ExportAck::accept(batch));
        Ok(ExportProgress::new(ExportPhase::Prepared, 0, None))
    }

    fn poll_export(&mut self, control: ExportPollControl) -> Result<ExportPoll, ExporterError> {
        let ack = self.pending.take().ok_or_else(|| {
            ExporterError::new(ExporterErrorCode::Protocol, false)
        })?;
        Ok(match control {
            ExportPollControl::Continue { .. } => ExportPoll::Accepted {
                ack,
                progress: ExportProgress::new(ExportPhase::Complete, 0, None),
            },
            ExportPollControl::Cancel => ExportPoll::Cancelled(ExportProgress::new(
                ExportPhase::Complete,
                0,
                None,
            )),
        })
    }

    fn begin_shutdown(&mut self) -> Result<ExportProgress, ExporterError> {
        Ok(ExportProgress::new(ExportPhase::Cleaning, 0, None))
    }

    fn poll_shutdown(
        &mut self,
        _control: ExportPollControl,
    ) -> Result<ExporterShutdownPoll, ExporterError> {
        Ok(ExporterShutdownPoll::Complete(ExportProgress::new(
            ExportPhase::Complete,
            0,
            None,
        )))
    }
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}
