//! Redaction-safe bounded local telemetry export.

mod local_file;
mod runtime;

pub(crate) use runtime::TelemetryRuntime;
