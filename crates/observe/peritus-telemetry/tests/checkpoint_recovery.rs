//! Focused durable-checkpoint and restart-recovery adversarial tests.

mod support;

use std::{
    fs::{self, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    num::NonZeroUsize,
};

use peritus_telemetry::{
    BackpressurePolicy, BufferConfig, CheckpointStore, ExportAck, ExportBatch, ExportCheckpoint,
    ExportStreamId, Exporter, ExporterError, TelemetryBuffer, TelemetryErrorKind, TelemetryPump,
    recover_buffer,
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
    let config = config(8, 1);
    let mut pump = TelemetryPump::new(stream, TelemetryBuffer::new(config));
    for record in projection.records() {
        pump.buffer_mut().enqueue(record.clone()).expect("enqueue projection");
    }
    pump.flush_one(&mut AcceptingExporter).expect("ack first record");
    let checkpoint = ExportCheckpoint::from_pump(&pump);
    assert_eq!(checkpoint.disposed_through_sequence(), 1);

    {
        let store = CheckpointStore::open(
            temporary.path(),
            stream,
            NonZeroUsize::new(2).expect("retention"),
        )
        .expect("checkpoint store");
        store.persist(checkpoint).expect("persist checkpoint");
    }

    let reopened =
        CheckpointStore::open(temporary.path(), stream, NonZeroUsize::new(2).expect("retention"))
            .expect("reopen checkpoint store");
    let loaded = reopened.load_latest().expect("load checkpoint");
    let report = recover_buffer(config, stream, loaded, &projection).expect("recover queue");
    assert_eq!(report.replayed(), 2);
    assert_eq!(report.dropped_during_recovery(), 0);
    let recovered = report.into_pump();
    assert_eq!(recovered.disposed_through_sequence(), 1);
    assert_eq!(recovered.buffer().len(), 2);
    assert_eq!(recovered.buffer().counters().submitted(), 3);
}

#[test]
fn drop_oldest_checkpoint_preserves_eviction_before_later_export_across_restart() {
    let stream = ExportStreamId::new([44; 16]).expect("stream");
    let projection = projection(DiagnosticCode::RecoveryCompleted);
    let config = config_with_policy(2, 1, BackpressurePolicy::DropOldest);
    let mut pump = TelemetryPump::new(stream, TelemetryBuffer::new(config));
    for record in projection.records() {
        pump.buffer_mut().enqueue(record.clone()).expect("enqueue projection");
    }
    assert_eq!(pump.buffer().counters().dropped(), 1);
    pump.flush_one(&mut AcceptingExporter).expect("export sequence two");

    let checkpoint = ExportCheckpoint::from_pump(&pump);
    assert_eq!(checkpoint.disposed_through_sequence(), 2);
    assert_eq!(checkpoint.counters().submitted(), 2);
    assert_eq!(checkpoint.counters().accepted(), 2);
    assert_eq!(checkpoint.counters().dropped(), 1);
    assert_eq!(checkpoint.counters().exported(), 1);

    let report = recover_buffer(config, stream, Some(checkpoint), &projection).expect("recover");
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
fn reject_newest_checkpoint_preserves_trailing_gap_after_earlier_exports() {
    let stream = ExportStreamId::new([45; 16]).expect("stream");
    let projection = projection(DiagnosticCode::RecoveryCompleted);
    let config = config_with_policy(2, 1, BackpressurePolicy::RejectNewest);
    let mut pump = TelemetryPump::new(stream, TelemetryBuffer::new(config));
    for record in projection.records() {
        pump.buffer_mut().enqueue(record.clone()).expect("enqueue projection");
    }
    pump.flush_one(&mut AcceptingExporter).expect("export sequence one");
    pump.flush_one(&mut AcceptingExporter).expect("export sequence two");

    let checkpoint = ExportCheckpoint::from_pump(&pump);
    assert_eq!(checkpoint.disposed_through_sequence(), 3);
    assert_eq!(checkpoint.counters().submitted(), 3);
    assert_eq!(checkpoint.counters().accepted(), 2);
    assert_eq!(checkpoint.counters().dropped(), 1);
    assert_eq!(checkpoint.counters().exported(), 2);

    let report = recover_buffer(config, stream, Some(checkpoint), &projection).expect("recover");
    assert_eq!(report.replayed(), 0);
    assert_eq!(report.dropped_during_recovery(), 0);
    let recovered = report.into_pump();
    assert_eq!(recovered.disposed_through_sequence(), 3);
    assert!(recovered.buffer().is_empty());
    assert_eq!(recovered.buffer().counters(), checkpoint.counters());
}

#[test]
fn recovery_rejects_a_checkpoint_from_changed_projection_history() {
    let stream = ExportStreamId::new([42; 16]).expect("stream");
    let original = projection(DiagnosticCode::RecoveryCompleted);
    let changed = projection(DiagnosticCode::RecoveryFailed);
    let config = config(8, 1);
    let mut pump = TelemetryPump::new(stream, TelemetryBuffer::new(config));
    pump.buffer_mut().enqueue(original.records()[0].clone()).expect("enqueue");
    pump.flush_one(&mut AcceptingExporter).expect("ack original prefix");
    let checkpoint = ExportCheckpoint::from_pump(&pump);

    let error = recover_buffer(config, stream, Some(checkpoint), &changed)
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
    let pump = TelemetryPump::new(stream, TelemetryBuffer::new(config(2, 1)));
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

fn config(capacity: usize, batch: usize) -> BufferConfig {
    config_with_policy(capacity, batch, BackpressurePolicy::RejectNewest)
}

fn config_with_policy(capacity: usize, batch: usize, policy: BackpressurePolicy) -> BufferConfig {
    BufferConfig::new(
        NonZeroUsize::new(capacity).expect("capacity"),
        NonZeroUsize::new(batch).expect("batch"),
        policy,
    )
    .expect("buffer config")
}

struct AcceptingExporter;

impl Exporter for AcceptingExporter {
    fn export(&mut self, batch: &ExportBatch) -> Result<ExportAck, ExporterError> {
        Ok(ExportAck::accept(batch))
    }

    fn shutdown(&mut self) -> Result<(), ExporterError> {
        Ok(())
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
