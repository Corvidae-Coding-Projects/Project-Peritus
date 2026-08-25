//! Stable idempotent exporter batches, acknowledgements, and shutdown pumping.

mod encoding;
mod pump;
mod types;

pub use pump::{FlushOutcome, ShutdownOutcome, TelemetryPump};
pub use types::{
    ExportAck, ExportBatch, ExportItem, ExportRecord, ExportStreamId, Exporter, ExporterError,
    ExporterErrorCode,
};
