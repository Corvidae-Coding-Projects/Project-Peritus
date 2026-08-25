//! Deterministic restart recovery from a projection and final-disposition checkpoint.

use crate::{
    BufferConfig, EnqueueOutcome, ExportCheckpoint, ExportStreamId, TelemetryBuffer,
    TelemetryError, TelemetryErrorKind, TelemetryProjection, TelemetryPump,
};

/// Successful bounded reconstruction report.
pub struct RecoveryReport {
    pump: TelemetryPump,
    replayed: u64,
    dropped_during_recovery: u64,
}

impl RecoveryReport {
    /// Consumes the report and returns the recovered pump.
    #[must_use]
    pub fn into_pump(self) -> TelemetryPump {
        self.pump
    }
    /// Returns records replayed after the checkpoint.
    #[must_use]
    pub const fn replayed(&self) -> u64 {
        self.replayed
    }
    /// Returns records rejected or evicted by bounded recovery.
    #[must_use]
    pub const fn dropped_during_recovery(&self) -> u64 {
        self.dropped_during_recovery
    }
}

/// Rebuilds a bounded export queue from deterministic projection records after a checkpoint.
///
/// The checkpoint prefix must exactly match the current projection through its disposed
/// sequence. Later projection records are replayed in stable order under the configured
/// backpressure policy, with every recovery drop counted.
///
/// # Errors
///
/// Returns stream, future-checkpoint, prefix, counter, or enqueue-overflow failures.
pub fn recover_buffer(
    config: BufferConfig,
    stream_id: ExportStreamId,
    checkpoint: Option<ExportCheckpoint>,
    projection: &TelemetryProjection,
) -> Result<RecoveryReport, TelemetryError> {
    let checkpoint = checkpoint.unwrap_or_else(|| {
        let buffer = TelemetryBuffer::new(config);
        ExportCheckpoint::from_pump(&TelemetryPump::new(stream_id, buffer))
    });
    if checkpoint.stream_id() != stream_id {
        return Err(recovery_error("checkpoint belongs to another export stream"));
    }
    let disposed = usize::try_from(checkpoint.disposed_through_sequence())
        .map_err(|_| recovery_error("checkpoint sequence cannot index projection records"))?;
    if disposed > projection.records().len() {
        return Err(recovery_error("checkpoint is ahead of the rebuilt projection"));
    }
    let mut prefix = peritus_types::Sha256Digest::new([0; 32]);
    for (index, record) in projection.records().iter().take(disposed).enumerate() {
        let sequence = u64::try_from(index)
            .ok()
            .and_then(|value| value.checked_add(1))
            .ok_or_else(|| recovery_error("projection sequence overflow"))?;
        prefix = crate::buffer::record_prefix(prefix, sequence, record)?;
    }
    if prefix != checkpoint.prefix_digest() {
        return Err(recovery_error("checkpoint prefix differs from rebuilt projection history"));
    }

    let mut pump = TelemetryPump::new(stream_id, TelemetryBuffer::new(config));
    pump.restore_disposition(checkpoint.prefix_digest(), checkpoint.counters());
    let mut replayed = 0_u64;
    let mut dropped = 0_u64;
    for record in projection.records().iter().skip(disposed) {
        let outcome = pump.buffer_mut().enqueue(record.clone())?;
        replayed = replayed.checked_add(1).ok_or_else(|| {
            TelemetryError::new(
                TelemetryErrorKind::SequenceOverflow,
                "recover telemetry buffer",
                "recovery replay count overflow",
            )
        })?;
        if matches!(
            outcome,
            EnqueueOutcome::DroppedOldest { .. } | EnqueueOutcome::RejectedNewest { .. }
        ) {
            dropped = dropped.checked_add(1).ok_or_else(|| {
                TelemetryError::new(
                    TelemetryErrorKind::SequenceOverflow,
                    "recover telemetry buffer",
                    "recovery drop count overflow",
                )
            })?;
        }
    }
    Ok(RecoveryReport { pump, replayed, dropped_during_recovery: dropped })
}

const fn recovery_error(detail: &'static str) -> TelemetryError {
    TelemetryError::new(TelemetryErrorKind::RecoveryMismatch, "recover telemetry buffer", detail)
}
