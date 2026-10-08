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
mod spill;
mod storage;
pub mod verified;

pub use buffer::{
    BufferConfig, BufferCounters, EnqueueOutcome, ObservationLossPolicy, RejectionReason,
    TelemetryBuffer,
};
pub use error::{RecoveryClass, TelemetryError, TelemetryErrorKind};
pub use export::{
    DrainAction, ExportAck, ExportBatch, ExportItem, ExportPhase, ExportPoll, ExportPollControl,
    ExportProgress, ExportRecord, ExportStreamId, Exporter, ExporterError, ExporterErrorCode,
    ExporterShutdownPoll, FlushOutcome, PendingTelemetry, ShutdownOutcome, TelemetryPump,
};
pub use metrics::{MetricIter, MetricName, MetricPoint, MetricState};
pub use projection::{OtelEvent, OtelSpan, TelemetryProjection, project_telemetry};
pub use recovery::{RecoveryReport, recover_buffer};
pub use spill::SpillStore;
pub use storage::{CheckpointStore, ExportCheckpoint};
