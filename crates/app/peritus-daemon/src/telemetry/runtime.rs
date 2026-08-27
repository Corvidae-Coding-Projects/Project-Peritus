//! C7 projection recovery, local export, and exact checkpoint ownership.

use std::{num::NonZeroUsize, path::Path};

use peritus_journal::{SqliteJournal, StoreId};
use peritus_telemetry::{
    BackpressurePolicy, BufferConfig, CheckpointStore, ExportCheckpoint, ExportStreamId,
    FlushOutcome, ShutdownOutcome, TelemetryPump, project_telemetry, recover_buffer,
};

use super::local_file::LocalFileExporter;
use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

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
        let checkpoint_directory = directory.join("checkpoints");
        let checkpoints = CheckpointStore::open(
            checkpoint_directory,
            stream,
            NonZeroUsize::new(8).expect("positive checkpoint retention"),
        )
        .map_err(component_error)?;
        let checkpoint = checkpoints.load_latest().map_err(component_error)?;
        let config = BufferConfig::new(
            NonZeroUsize::new(4_096).expect("positive telemetry capacity"),
            NonZeroUsize::new(128).expect("positive telemetry batch"),
            BackpressurePolicy::DropOldest,
        )
        .map_err(component_error)?;
        let pump = recover_buffer(config, stream, checkpoint, &projection)
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
            match self.pump.flush_one(&mut self.exporter).map_err(component_error)? {
                FlushOutcome::Empty => return Ok(()),
                FlushOutcome::Exported { .. } => self
                    .checkpoints
                    .persist(ExportCheckpoint::from_pump(&self.pump))
                    .map_err(component_error)?,
            }
        }
    }

    pub(crate) fn shutdown(&mut self) -> Result<(), DaemonError> {
        let batches = u64::try_from(self.pump.buffer().len()).unwrap_or(u64::MAX).max(1);
        match self.pump.shutdown(&mut self.exporter, batches).map_err(component_error)? {
            ShutdownOutcome::Complete => self
                .checkpoints
                .persist(ExportCheckpoint::from_pump(&self.pump))
                .map_err(component_error),
            ShutdownOutcome::Pending { remaining } => Err(DaemonError::new(
                DaemonErrorCode::UncleanShutdown,
                DaemonRecovery::Retry,
                "shutdown telemetry exporter",
                format!("{remaining} telemetry records remain queued"),
            )),
        }
    }
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
