//! Stable idempotent exporter batches, acknowledgements, and shutdown pumping.

mod encoding;
mod pump;
mod types;

pub use pump::{DrainAction, FlushOutcome, PendingTelemetry, ShutdownOutcome, TelemetryPump};
pub use types::{
    ExportAck, ExportBatch, ExportItem, ExportPhase, ExportPoll, ExportPollControl, ExportProgress,
    ExportRecord, ExportStreamId, Exporter, ExporterError, ExporterErrorCode,
    ExporterShutdownPoll,
};
