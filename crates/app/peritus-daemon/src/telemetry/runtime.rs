//! Lossless C7 projection recovery, caller-owned spill, local export, and checkpoints.

use std::{num::NonZeroUsize, path::Path};

use peritus_journal::{SqliteJournal, StoreId};
use peritus_telemetry::{
    BufferConfig, CheckpointStore, DrainAction, ExportPollControl, ExportStreamId, FlushOutcome,
    ObservationLossPolicy, PendingBatchStore, ShutdownOutcome, SpillStore, TelemetryBuffer,
    TelemetryPump, project_telemetry, recover_buffer,
};

use super::local_file::LocalFileExporter;
use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

const MEMORY_PAGE_BYTES: usize = 4_096;
const MEMORY_PAGES: usize = 4_096;
const EXPORT_BATCH_BYTES: usize = 512 * 1_024;
const EXPORT_POLL_BYTES: usize = 64 * 1_024;

pub struct TelemetryRuntime {
    pump: TelemetryPump,
    exporter: LocalFileExporter,
    checkpoints: CheckpointStore,
}

impl TelemetryRuntime {
    pub(crate) fn open(
        journal: &mut SqliteJournal,
        store_id: StoreId,
        directory: &Path,
        quota_bytes: u64,
    ) -> Result<Self, DaemonError> {
        let traces = peritus_trace::recover_all(journal).map_err(component_error)?;
        let projection = project_telemetry(&traces).map_err(component_error)?;
        let stream = ExportStreamId::new(*store_id.as_bytes()).map_err(component_error)?;
        let checkpoints = CheckpointStore::open(
            directory.join("checkpoints"),
            stream,
            NonZeroUsize::new(8).expect("positive checkpoint retention"),
        )
        .map_err(component_error)?;
        let checkpoint = checkpoints.load_latest().map_err(component_error)?;
        let config = BufferConfig::new(
            NonZeroUsize::new(MEMORY_PAGE_BYTES).expect("positive telemetry page size"),
            NonZeroUsize::new(MEMORY_PAGES).expect("positive telemetry page count"),
            NonZeroUsize::new(EXPORT_BATCH_BYTES).expect("positive telemetry batch bytes"),
            ObservationLossPolicy::LosslessSpill,
        )
        .map_err(component_error)?;
        let spill = SpillStore::open(directory.join("spill"), stream).map_err(component_error)?;
        let buffer = TelemetryBuffer::with_spill(config, spill).map_err(component_error)?;
        let ownership =
            PendingBatchStore::open(directory.join("pending"), stream).map_err(component_error)?;
        let pump = recover_buffer(buffer, ownership, stream, checkpoint, &projection)
            .map_err(component_error)?
            .into_pump();
        let exporter = LocalFileExporter::open(&directory.join("batches"), quota_bytes)
            .map_err(component_error)?;
        let mut runtime = Self { pump, exporter, checkpoints };
        runtime.flush_pending()?;
        Ok(runtime)
    }

    pub(crate) fn flush_pending(&mut self) -> Result<(), DaemonError> {
        loop {
            match self
                .pump
                .poll_flush(&mut self.exporter, poll_control())
                .map_err(component_error)?
            {
                FlushOutcome::Empty => return Ok(()),
                FlushOutcome::Pending(_) => {}
                FlushOutcome::Exported { .. } => self.persist_checkpoint()?,
                FlushOutcome::Cancelled { remaining, .. } => {
                    return Err(cancelled_drain_error(remaining.total()));
                }
            }
        }
    }

    pub(crate) fn shutdown(&mut self) -> Result<(), DaemonError> {
        loop {
            let outcome = self
                .pump
                .poll_shutdown(&mut self.exporter, poll_control())
                .map_err(component_error)?;
            match outcome {
                ShutdownOutcome::Complete => {
                    self.persist_checkpoint()?;
                    return Ok(());
                }
                ShutdownOutcome::Pending {
                    next: DrainAction::PersistCheckpoint,
                    ..
                } => self.persist_checkpoint()?,
                ShutdownOutcome::Pending { .. } => {}
                ShutdownOutcome::Cancelled { remaining, .. } => {
                    return Err(cancelled_drain_error(remaining.total()));
                }
            }
        }
    }

    fn persist_checkpoint(&mut self) -> Result<(), DaemonError> {
        self.checkpoints
            .persist_and_retire(&mut self.pump)
            .map_err(component_error)
    }
}

fn poll_control() -> ExportPollControl {
    ExportPollControl::continue_with(
        NonZeroUsize::new(EXPORT_POLL_BYTES).expect("positive telemetry poll bytes"),
    )
}

fn cancelled_drain_error(remaining: u64) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::UncleanShutdown,
        DaemonRecovery::Retry,
        "drain telemetry exporter",
        format!("telemetry export was cancelled with {remaining} records still in custody"),
    )
}

fn component_error(error: impl std::error::Error + Send + Sync + 'static) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Worker,
        DaemonRecovery::Retry,
        "operate local telemetry export",
        error.to_string(),
        error,
    )
}
