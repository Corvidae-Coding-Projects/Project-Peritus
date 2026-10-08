//! Deterministic restart recovery from projection history, checkpoints, and caller-owned spill.

use crate::{
    BufferConfig, EnqueueOutcome, ExportCheckpoint, ExportStreamId, ObservationLossPolicy,
    TelemetryBuffer, TelemetryError, TelemetryErrorKind, TelemetryProjection, TelemetryPump,
};

/// Successful reconstruction report.
pub struct RecoveryReport {
    pump: TelemetryPump,
    replayed: u64,
    dropped_during_recovery: u64,
    spilled_during_recovery: u64,
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
    /// Returns records explicitly lost under a caller-selected lossy recovery policy.
    #[must_use]
    pub const fn dropped_during_recovery(&self) -> u64 {
        self.dropped_during_recovery
    }
    /// Returns replayed records placed under durable-spill custody.
    #[must_use]
    pub const fn spilled_during_recovery(&self) -> u64 {
        self.spilled_during_recovery
    }
}

/// Rebuilds export custody from deterministic projection records after a checkpoint.
///
/// The supplied buffer already owns any caller-selected durable spill directory. Existing spill
/// files are idempotently verified while the projection suffix is replayed, so no global path or
/// second owner is invented. The checkpoint prefix must exactly match the configured historical
/// loss decisions and accepted canonical bytes.
///
/// # Errors
///
/// Returns stream, future-checkpoint, prefix, counter, spill, or enqueue failures.
pub fn recover_buffer(
    buffer: TelemetryBuffer,
    stream_id: ExportStreamId,
    checkpoint: Option<ExportCheckpoint>,
    projection: &TelemetryProjection,
) -> Result<RecoveryReport, TelemetryError> {
    let config = buffer.config();
    let mut pump = TelemetryPump::new(stream_id, buffer)?;
    let checkpoint = checkpoint.unwrap_or_else(|| ExportCheckpoint::from_pump(&pump));
    if checkpoint.stream_id() != stream_id {
        return Err(recovery_error("checkpoint belongs to another export stream"));
    }
    let disposed = usize::try_from(checkpoint.disposed_through_sequence())
        .map_err(|_| recovery_error("checkpoint sequence cannot index projection records"))?;
    if disposed > projection.records().len() {
        return Err(recovery_error("checkpoint is ahead of the rebuilt projection"));
    }
    let prefix = rebuilt_prefix(config, &projection.records()[..disposed])?;
    if prefix != checkpoint.prefix_digest() {
        return Err(recovery_error(
            "checkpoint prefix differs from rebuilt canonical and loss history",
        ));
    }

    pump.restore_disposition(checkpoint.prefix_digest(), checkpoint.counters());
    let mut replayed = 0_u64;
    let mut dropped = 0_u64;
    let mut spilled = 0_u64;
    for record in projection.records().iter().skip(disposed) {
        let outcome = pump.enqueue(record.clone())?;
        replayed = replayed.checked_add(1).ok_or_else(replay_overflow)?;
        match outcome {
            EnqueueOutcome::DroppedOldest { count, .. } => {
                dropped = dropped.checked_add(count).ok_or_else(replay_overflow)?;
            }
            EnqueueOutcome::RejectedNewest { .. } => {
                dropped = dropped.checked_add(1).ok_or_else(replay_overflow)?;
            }
            EnqueueOutcome::Spilled { .. } => {
                spilled = spilled.checked_add(1).ok_or_else(replay_overflow)?;
            }
            EnqueueOutcome::Accepted { .. } => {}
        }
    }
    if config.loss_policy() == ObservationLossPolicy::LosslessSpill
        && (dropped != 0 || pump.pending().total() != replayed)
    {
        return Err(recovery_error(
            "lossless spill custody does not match the projection suffix",
        ));
    }
    Ok(RecoveryReport {
        pump,
        replayed,
        dropped_during_recovery: dropped,
        spilled_during_recovery: spilled,
    })
}

fn rebuilt_prefix(
    config: BufferConfig,
    records: &[crate::ExportRecord],
) -> Result<peritus_types::Sha256Digest, TelemetryError> {
    let mut prefix = peritus_types::Sha256Digest::new([0; 32]);
    if config.loss_policy() == ObservationLossPolicy::LosslessSpill {
        for (index, record) in records.iter().enumerate() {
            let sequence = replay_sequence(index)?;
            let canonical = record.canonical_bytes()?;
            prefix = crate::buffer::accepted_record_prefix(prefix, sequence, &canonical)?;
        }
        return Ok(prefix);
    }

    let mut shadow = TelemetryBuffer::new(config)?;
    for (index, record) in records.iter().enumerate() {
        let sequence = replay_sequence(index)?;
        let outcome = shadow.enqueue(record.clone())?;
        prefix = match outcome {
            EnqueueOutcome::RejectedNewest { .. } => {
                crate::buffer::lost_record_prefix(prefix, sequence)
            }
            EnqueueOutcome::Accepted { .. } | EnqueueOutcome::DroppedOldest { .. } => {
                let canonical = record.canonical_bytes()?;
                crate::buffer::accepted_record_prefix(prefix, sequence, &canonical)?
            }
            EnqueueOutcome::Spilled { .. } => {
                return Err(recovery_error("lossy prefix simulation unexpectedly used spill"));
            }
        };
    }
    Ok(prefix)
}

fn replay_sequence(index: usize) -> Result<u64, TelemetryError> {
    u64::try_from(index)
        .ok()
        .and_then(|value| value.checked_add(1))
        .ok_or_else(replay_overflow)
}

const fn replay_overflow() -> TelemetryError {
    TelemetryError::new(
        TelemetryErrorKind::SequenceOverflow,
        "recover telemetry buffer",
        "recovery replay accounting overflow",
    )
}

const fn recovery_error(detail: &'static str) -> TelemetryError {
    TelemetryError::new(TelemetryErrorKind::RecoveryMismatch, "recover telemetry buffer", detail)
}
