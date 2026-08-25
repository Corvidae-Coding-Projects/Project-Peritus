//! Bounded OpenTelemetry-compatible projections and exporters for Peritus.
//!
//! The crate accepts only checked, redaction-safe C7 projections. Export is at-least-once with a
//! stable batch identity and exact acknowledgement. Telemetry has no route back into execution or
//! authority state.

pub(crate) mod buffer;
mod error;
mod export;
mod metrics;
mod projection;
mod recovery;
mod storage;
pub mod verified;

pub use buffer::{
    BackpressurePolicy, BufferConfig, BufferCounters, EnqueueOutcome, TelemetryBuffer,
};
pub use error::{RecoveryClass, TelemetryError, TelemetryErrorKind};
pub use export::{
    ExportAck, ExportBatch, ExportItem, ExportRecord, ExportStreamId, Exporter, ExporterError,
    ExporterErrorCode, FlushOutcome, ShutdownOutcome, TelemetryPump,
};
pub use metrics::{MetricIter, MetricName, MetricPoint, MetricState};
pub use projection::{OtelEvent, OtelSpan, TelemetryProjection, project_telemetry};
pub use recovery::{RecoveryReport, recover_buffer};
pub use storage::{CheckpointStore, ExportCheckpoint};
